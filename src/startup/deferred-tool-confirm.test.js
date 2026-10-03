import { describe, expect, it, vi } from 'vitest';
import { registerToolConfirmListener } from './deferred-tool-confirm.js';

describe('deferred tool confirmation', () => {
  it('does not register until called and forwards the decision', async () => {
    const listen = vi.fn().mockResolvedValue(() => {});
    const invoke = vi.fn().mockResolvedValue(undefined);
    const showConfirm = vi.fn().mockResolvedValue(false);
    expect(listen).not.toHaveBeenCalled();

    await registerToolConfirmListener({ listen, invoke, showConfirm });
    expect(listen).toHaveBeenCalledOnce();
    expect(listen).toHaveBeenCalledWith('tool:confirm_required', expect.any(Function));

    const handler = listen.mock.calls[0][1];
    await handler({ payload: { request_id: 'request-1', tool_name: 'write_file', args_preview: 'sample', risk_level: 'R3' } });
    expect(showConfirm).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith('tool_confirm_response', { requestId: 'request-1', approved: false });
  });

  it('propagates registration failure', async () => {
    const error = new Error('listener unavailable');
    const listen = vi.fn().mockRejectedValue(error);
    await expect(registerToolConfirmListener({ listen, invoke: vi.fn(), showConfirm: vi.fn() })).rejects.toBe(error);
  });
});
