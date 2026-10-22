import { describe, it, expect, vi, afterEach } from 'vitest';
import { createRemoteAgentService, getRemoteAgentService } from './remote-agent-service.js';
const live = () => ({ watchId: 'w1', sessionId: 's1', phase: 'idle', ready: true, messages: [], interactions: [] });
function fixture() {
  let unlocked = true;
  let profiles = [{ id: 'p1', name: 'Test', endpoint: 'https://test.invalid/', sessionId: 's1', autoReconnect: true, uncertainSessions: [] }];
  const api = {
    remoteAgentProfiles: vi.fn(async () => ({ unlocked, profiles: structuredClone(profiles) })),
    remoteAgentUnlock: vi.fn(async () => { unlocked = true; return { unlocked, profiles: structuredClone(profiles) }; }),
    remoteAgentLock: vi.fn(async () => { unlocked = false; }),
    remoteAgentReconnect: vi.fn(async () => ({ connectionId: 'c1', profileId: 'p1', capabilities: { catalog: true, history: true, send: true } })),
    remoteAgentCatalog: vi.fn(async () => ({ workspaces: [], sessions: [{ sessionId: 's1' }] })),
    remoteAgentWatch: vi.fn(async () => live()), remoteAgentLive: vi.fn(async () => live()),
    remoteAgentAction: vi.fn(async () => live()), remoteAgentDisconnect: vi.fn(async () => {}),
    remoteAgentProfileUpdate: vi.fn(async (id, patch) => { const p = profiles.find(p => p.id === id); Object.assign(p, patch); if (patch.reviewedSession) p.uncertainSessions = []; }),
    remoteAgentForget: vi.fn(async id => { profiles = profiles.filter(p => p.id !== id); }),
  };
  return { api, profiles, service: createRemoteAgentService(api) };
}
afterEach(() => vi.useRealTimers());
describe('saved connections and safe reconnect', () => {
  it('unlocks with the native system without a Bob password', async () => {
    const { api, service } = fixture();
    expect(await service.unlock()).toBe(true);
    expect(api.remoteAgentUnlock).toHaveBeenCalledWith(undefined);
    expect(service.state.unlocked).toBe(true);
    expect(api.remoteAgentAction).not.toHaveBeenCalled();
    await service.dispose();
  });
  it('preserves locked state on cancellation and invalid native responses', async () => {
    for (const response of [null, {}, { unlocked: false, profiles: [] }, { unlocked: true }]) {
      const { api, service } = fixture(); api.remoteAgentUnlock.mockResolvedValue(response);
      expect(await service.unlock()).toBe(false); expect(service.state.unlocked).toBe(false);
      expect(service.state.storageBusy).toBe(false); expect(api.remoteAgentReconnect).not.toHaveBeenCalled();
      await service.dispose();
    }
    const { api, service } = fixture(); api.remoteAgentUnlock.mockRejectedValue('REMOTE_SYSTEM_AUTH_CANCELLED');
    expect(await service.unlock()).toBe(false); expect(service.state.unlocked).toBe(false);
    expect(service.state.error).toBe('REMOTE_SYSTEM_AUTH_CANCELLED'); await service.dispose();
  });
  it('offers legacy migration only when native storage requests it', async () => {
    const { api, service } = fixture();
    api.remoteAgentUnlock.mockRejectedValueOnce('REMOTE_VAULT_MIGRATION_REQUIRED');
    expect(await service.unlock()).toBe(false); expect(service.state.migrationRequired).toBe(true);
    expect(await service.unlock('one-time-old-password')).toBe(true);
    expect(service.state.migrationRequired).toBe(false);
    expect(JSON.stringify(service.state)).not.toContain('one-time-old-password');
    await service.dispose();
  });
  it('is lazy and shares a single application lifetime', async () => {
    const { api, service } = fixture(); expect(api.remoteAgentProfiles).not.toHaveBeenCalled();
    expect(getRemoteAgentService(api)).toBe(getRemoteAgentService(api));
    await service.resume(); expect(service.state.sessionId).toBe('s1');
    await service.resume(); expect(api.remoteAgentReconnect).toHaveBeenCalledTimes(1);
    await service.dispose();
  });
  it('does not connect while locked or replay an execution', async () => {
    const { api, service } = fixture(); api.remoteAgentProfiles.mockResolvedValue({ unlocked: false, profiles: [] });
    await service.resume(); expect(api.remoteAgentReconnect).not.toHaveBeenCalled(); expect(api.remoteAgentAction).not.toHaveBeenCalled(); await service.dispose();
  });
  it('restores sessions after bounded retry, never resending a prompt', async () => {
    vi.useFakeTimers(); const { api, service } = fixture(); await service.resume();
    api.remoteAgentLive.mockRejectedValueOnce('REMOTE_DISCONNECTED');
    await vi.advanceTimersByTimeAsync(250); expect(service.state.reconnecting).toBe(true);
    await vi.advanceTimersByTimeAsync(1000); expect(api.remoteAgentReconnect).toHaveBeenCalledTimes(2);
    expect(api.remoteAgentAction).not.toHaveBeenCalled(); expect(service.state.sessionId).toBe('s1'); await service.dispose();
  });
  it('stops retries for expired authentication and protocol failures', async () => {
    vi.useFakeTimers();
    for (const error of ['REMOTE_AUTH', 'REMOTE_PROTOCOL']) {
      const { api, service } = fixture(); api.remoteAgentReconnect.mockRejectedValue(error);
      await service.resume(); await vi.advanceTimersByTimeAsync(60000); await service.resume();
      expect(api.remoteAgentReconnect).toHaveBeenCalledTimes(1); expect(service.state.error).toBe(error); await service.dispose();
    }
  });
  it('exhausts network retries and preserves saved connections', async () => {
    vi.useFakeTimers(); const { api, service } = fixture(); api.remoteAgentReconnect.mockRejectedValue('REMOTE_NETWORK');
    await service.resume(); await vi.advanceTimersByTimeAsync(90000);
    expect(api.remoteAgentReconnect).toHaveBeenCalledTimes(6); expect(service.state.profiles).toHaveLength(1); await service.dispose();
  });
  it('explicit disconnect disables automatic restore and preserves profiles', async () => {
    const { api, service } = fixture(); await service.resume(); await service.disconnect(); await service.resume();
    expect(api.remoteAgentReconnect).toHaveBeenCalledTimes(1); expect(service.state.profiles[0].autoReconnect).toBe(false); await service.dispose();
  });
  it('does not reset its retry budget when every restored watch immediately fails', async () => {
    vi.useFakeTimers(); const { api, service } = fixture();
    api.remoteAgentWatch.mockRejectedValue('REMOTE_DISCONNECTED');
    await service.resume(); await vi.advanceTimersByTimeAsync(90000);
    expect(api.remoteAgentReconnect).toHaveBeenCalledTimes(6);
    expect(service.state.retries).toBe(5); await service.dispose();
  });
  it('surfaces a failed disconnect preference write after closing the socket', async () => {
    const { api, service } = fixture(); await service.resume();
    api.remoteAgentProfileUpdate.mockRejectedValueOnce('REMOTE_STORAGE');
    await service.disconnect(); expect(service.state.error).toBe('REMOTE_STORAGE');
    expect(service.state.connection).toBeNull(); await service.dispose();
  });
  it('does not publish a late save after the user locks the vault', async () => {
    const { api, service } = fixture(); let resolve;
    api.remoteAgentSave = vi.fn(() => new Promise(r => { resolve = r; }));
    const save = service.save({ endpoint: 'https://test.invalid/', token: 'test' }, 'Test');
    await vi.waitFor(() => expect(api.remoteAgentSave).toHaveBeenCalledTimes(1));
    await service.lock(); resolve({ connectionId: 'late', profileId: 'p1' });
    expect(await save).toBe(false); expect(service.state.connection).toBeNull();
    expect(service.state.selectedProfileId).toBe(''); await service.dispose();
  });
  it('unknown outcomes require review even after reconnect', async () => {
    const { api, service, profiles } = fixture(); profiles[0].uncertainSessions = ['s1'];
    await service.resume(); expect(await service.send('again', 'r2')).toBe(false); expect(api.remoteAgentAction).not.toHaveBeenCalled();
    await service.reviewed(); expect(service.needsReview()).toBe(false); expect(await service.send('new input', 'r3')).toBe(true); await service.dispose();
  });
  it('forget removes local profile without sending cancel or deleting remote work', async () => {
    const { api, service } = fixture(); await service.resume(); await service.forget('p1');
    expect(service.state.profiles).toEqual([]); expect(service.state.connection).toBeNull(); expect(api.remoteAgentAction).not.toHaveBeenCalled(); await service.dispose();
  });
  it('lock closes the connection and clears visible saved profiles', async () => {
    const { api, service } = fixture(); await service.resume(); await service.lock();
    expect(service.state.unlocked).toBe(false); expect(service.state.connection).toBeNull(); expect(api.remoteAgentLock).toHaveBeenCalledTimes(1); await service.dispose();
  });
});
