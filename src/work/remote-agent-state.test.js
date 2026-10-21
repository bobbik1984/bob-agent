import { describe, it, expect, vi } from 'vitest';
import { createRemoteAgentController } from './remote-agent-state.js';

const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
function fixture() {
  const state = {};
  const api = {
    remoteAgentConnect: vi.fn(async () => ({ connectionId: 'c1', adapter: 'test-adapter', capabilities: { catalog: true, history: true } })),
    remoteAgentCatalog: vi.fn(async () => ({ workspaces: [], sessions: [] })),
    remoteAgentHistory: vi.fn(async (_, sessionId) => ({ sessionId, messages: [] })),
    remoteAgentDisconnect: vi.fn(async () => {}),
  };
  return { api, state, controller: createRemoteAgentController(api, patch => Object.assign(state, patch)) };
}

describe('remote agent panel lifetime', () => {
  it('does not guess or call unsupported adapter capabilities', async () => {
    const { api, state, controller } = fixture();
    api.remoteAgentConnect.mockResolvedValue({ connectionId: 'c1', capabilities: {} });
    await controller.connect({});
    expect(api.remoteAgentCatalog).not.toHaveBeenCalled();
    expect(state.error).toBe('REMOTE_UNSUPPORTED_CAPABILITY');
    api.remoteAgentConnect.mockResolvedValue({ connectionId: 'c2', capabilities: { catalog: true, history: false } });
    await controller.connect({});
    await controller.select('s1');
    expect(api.remoteAgentHistory).not.toHaveBeenCalled();
    expect(state.error).toBe('REMOTE_UNSUPPORTED_CAPABILITY');
  });
  it('does no eager work and uses a brand-neutral bridge without persisting credentials', async () => {
    const { api, state, controller } = fixture();
    expect(api.remoteAgentConnect).not.toHaveBeenCalled();
    await controller.connect({ adapter: 'test-adapter', token: 'private' });
    expect(state.connection.adapter).toBe('test-adapter');
    expect(JSON.stringify(state)).not.toContain('private');
    await controller.select('s1');
    expect(api.remoteAgentHistory).toHaveBeenCalledWith('c1', 's1');
    await controller.disconnect();
    expect(state.connection).toBeNull();
    expect(state.history).toBeNull();
  });
  it('cleans a late connection after the panel closes', async () => {
    const { api, state, controller } = fixture();
    const wait = deferred(); api.remoteAgentConnect.mockReturnValue(wait.promise);
    const connecting = controller.connect({});
    await Promise.resolve();
    controller.dispose();
    wait.resolve({ connectionId: 'late' });
    await connecting;
    expect(api.remoteAgentDisconnect).toHaveBeenCalledWith('late');
    expect(state.connection).toBeNull();
  });
  it('does not let old session history overwrite the new selection', async () => {
    const { api, state, controller } = fixture();
    await controller.connect({});
    const first = deferred();
    api.remoteAgentHistory.mockReturnValueOnce(first.promise);
    const selecting = controller.select('s1');
    await controller.select('s2');
    first.resolve({ sessionId: 's1' }); await selecting;
    expect(state.history.sessionId).toBe('s2');
    expect(state.sessionId).toBe('s2');
  });
  it('closes partial authentication on catalog failure and never displays raw errors', async () => {
    const { api, state, controller } = fixture();
    api.remoteAgentCatalog.mockRejectedValue('URL?token=private');
    await controller.connect({});
    expect(state.error).toBe('REMOTE_UNKNOWN');
    expect(state.connection).toBeNull();
    expect(api.remoteAgentDisconnect).toHaveBeenCalledWith('c1');
    expect(JSON.stringify(state)).not.toContain('private');
  });
  it('preserves history as a snapshot, not a fake online status, when refresh fails', async () => {
    const { api, state, controller } = fixture();
    await controller.connect({}); await controller.select('s1');
    api.remoteAgentCatalog.mockRejectedValue('REMOTE_DISCONNECTED');
    await controller.refresh();
    expect(state.error).toBe('REMOTE_DISCONNECTED');
    expect(state.history.sessionId).toBe('s1');
  });
  it('ignores late history after disconnect', async () => {
    const { api, state, controller } = fixture();
    await controller.connect({});
    const waiting = deferred(); api.remoteAgentHistory.mockReturnValue(waiting.promise);
    const selecting = controller.select('s1');
    await controller.disconnect(); waiting.resolve({ sessionId: 's1' }); await selecting;
    expect(state.history).toBeNull(); expect(state.historyBusy).toBe(false);
  });
});

describe('remote live execution', () => {
  function liveFixture() {
    const f = fixture();
    f.api.remoteAgentConnect.mockResolvedValue({ connectionId: 'c1', capabilities: { catalog: true, history: true, send: true } });
    f.api.remoteAgentWatch = vi.fn(async (_, sessionId) => ({ watchId: `w-${sessionId}`, sessionId, ready: true, phase: 'idle', messages: [], interactions: [] }));
    f.api.remoteAgentLive = vi.fn(async (_, watchId) => ({ watchId, sessionId: 's1', ready: true, phase: 'running', messages: [], interactions: [], draft: 'streamed' }));
    f.api.remoteAgentAction = vi.fn(async (_, input) => ({ watchId: input.watchId, sessionId: 's1', ready: true, phase: input.kind === 'cancel' ? 'cancel_requested' : 'accepted', messages: [], interactions: [] }));
    return f;
  }
  it('subscribes before send and does not promote accepted or cancel acknowledgements to completed', async () => {
    const { api, state, controller } = liveFixture();
    await controller.connect({});
    expect(await controller.send('hello', 'r1')).toBe(false);
    await controller.select('s1');
    await controller.send('hello', 'r1');
    expect(api.remoteAgentAction).toHaveBeenCalledWith('c1', { watchId: 'w-s1', kind: 'send', text: 'hello', requestId: 'r1' });
    expect(state.live.phase).toBe('accepted');
    await controller.cancel(); expect(state.live.phase).toBe('cancel_requested');
    await controller.dispose();
  });
  it('keeps unknown outcomes visible, disables actions and never automatically retries', async () => {
    const { api, state, controller } = liveFixture();
    await controller.connect({}); await controller.select('s1');
    api.remoteAgentAction.mockRejectedValue('REMOTE_OUTCOME_UNKNOWN');
    expect(await controller.send('hello', 'r1')).toBe(false);
    expect(state.live.phase).toBe('unknown'); expect(state.live.ready).toBe(false);
    expect(await controller.send('hello', 'r2')).toBe(false);
    expect(api.remoteAgentAction).toHaveBeenCalledTimes(1);
    await controller.dispose();
  });
  it('serializes double clicks and ignores late action responses after session switch', async () => {
    const { api, state, controller } = liveFixture();
    await controller.connect({}); await controller.select('s1');
    const waiting = deferred(); api.remoteAgentAction.mockReturnValue(waiting.promise);
    const sending = controller.send('hello', 'r1');
    expect(await controller.send('hello', 'r2')).toBe(false);
    await controller.select('s2');
    waiting.resolve({ watchId: 'w-s1', sessionId: 's1', phase: 'completed', ready: true });
    expect(await sending).toBe(false); expect(state.live.sessionId).toBe('s2');
    await controller.dispose();
  });
  it('polls native live state only and stops after disconnection', async () => {
    vi.useFakeTimers();
    try {
      const { api, state, controller } = liveFixture();
      await controller.connect({}); await controller.select('s1');
      await vi.advanceTimersByTimeAsync(250); expect(state.live.draft).toBe('streamed');
      api.remoteAgentLive.mockRejectedValue('REMOTE_DISCONNECTED');
      await vi.advanceTimersByTimeAsync(250); expect(state.live.ready).toBe(false);
      await vi.advanceTimersByTimeAsync(5000); expect(api.remoteAgentLive).toHaveBeenCalledTimes(2);
      await controller.dispose();
    } finally { vi.useRealTimers(); }
  });
  it('answers only with the currently selected watch and clears interactions on close', async () => {
    const { api, state, controller } = liveFixture();
    await controller.connect({}); await controller.select('s1');
    await controller.answer('event1', 'rejected');
    expect(api.remoteAgentAction).toHaveBeenCalledWith('c1', { kind: 'answer', watchId: 'w-s1', eventId: 'event1', answer: 'rejected' });
    await controller.disconnect(); expect(state.live).toBeNull();
    expect(await controller.answer('event1', 'allowed-once')).toBe(false);
  });
});
