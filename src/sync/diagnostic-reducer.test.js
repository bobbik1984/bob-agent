import { describe, expect, it } from 'vitest';
import { DIAGNOSTIC_STAGE, DIAGNOSTIC_STATUS } from './diagnostic-contract.js';
import { createDiagnosticTrace, reduceDiagnosticEvent } from './diagnostic-reducer.js';

function event(status, sequence, overrides = {}) {
  return { protocol_version: 2, trace_id: 'trace-1', message_id: 'message-1', sync_id: 'sync-1',
    from_device_id: 'mobile', target_device_id: 'pc', transport: 'relay',
    stage: DIAGNOSTIC_STAGE.MOBILE_TO_RELAY, status, sequence, timestamp: sequence, ...overrides };
}

describe('diagnostic reducer', () => {
  it('creates every independently rendered path', () => {
    expect(Object.keys(createDiagnosticTrace('trace-1').paths)).toEqual(Object.values(DIAGNOSTIC_STAGE));
  });
  it('is idempotent for duplicate events', () => {
    const once = reduceDiagnosticEvent(createDiagnosticTrace('trace-1'), event(DIAGNOSTIC_STATUS.RUNNING, 1));
    expect(reduceDiagnosticEvent(once, event(DIAGNOSTIC_STATUS.RUNNING, 1))).toBe(once);
  });
  it('does not let an older event overwrite a terminal state', () => {
    const success = reduceDiagnosticEvent(createDiagnosticTrace('trace-1'), event(DIAGNOSTIC_STATUS.SUCCESS, 4));
    const stale = reduceDiagnosticEvent(success, event(DIAGNOSTIC_STATUS.RUNNING, 3));
    expect(stale).toBe(success);
    expect(stale.paths.mobile_to_relay.status).toBe(DIAGNOSTIC_STATUS.SUCCESS);
  });
  it('keeps failures local to their path', () => {
    const failed = reduceDiagnosticEvent(createDiagnosticTrace('trace-1'), event(DIAGNOSTIC_STATUS.FAILED, 2, {
      stage: DIAGNOSTIC_STAGE.RELAY_TO_PC, error_code: 'RLY-ROUTE-001',
    }));
    expect(failed.paths.relay_to_pc.status).toBe(DIAGNOSTIC_STATUS.FAILED);
    expect(failed.paths.mobile_to_relay.status).toBe(DIAGNOSTIC_STATUS.PENDING);
  });
  it('ignores other traces and unknown stages', () => {
    const initial = createDiagnosticTrace('trace-1');
    expect(reduceDiagnosticEvent(initial, event(DIAGNOSTIC_STATUS.RUNNING, 1, { trace_id: 'other' }))).toBe(initial);
    expect(reduceDiagnosticEvent(initial, event(DIAGNOSTIC_STATUS.RUNNING, 1, { stage: 'not_a_stage' }))).toBe(initial);
  });
  it('represents timeout separately from explicit failure', () => {
    const result = reduceDiagnosticEvent(createDiagnosticTrace('trace-1'), event(DIAGNOSTIC_STATUS.TIMEOUT, 5));
    expect(result.paths.mobile_to_relay.status).toBe(DIAGNOSTIC_STATUS.TIMEOUT);
    expect(result.overall_status).toBe(DIAGNOSTIC_STATUS.TIMEOUT);
  });
});
