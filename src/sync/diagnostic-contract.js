export const DIAGNOSTIC_STATUS = Object.freeze({
  PENDING: 'pending', RUNNING: 'running', SUCCESS: 'success', FAILED: 'failed',
  TIMEOUT: 'timeout', UNKNOWN: 'unknown', SKIPPED: 'skipped',
});

export const DIAGNOSTIC_STAGE = Object.freeze({
  LAN_DIRECT: 'lan_direct', MOBILE_TO_RELAY: 'mobile_to_relay', RELAY_TO_PC: 'relay_to_pc',
  PC_PROCESSING: 'pc_processing', PC_TO_RELAY: 'pc_to_relay', RELAY_TO_MOBILE: 'relay_to_mobile',
  MOBILE_PROCESSING: 'mobile_processing', LOCAL_COMMIT: 'local_commit',
});

export const ALL_DIAGNOSTIC_STAGES = Object.freeze(Object.values(DIAGNOSTIC_STAGE));
export const TERMINAL_DIAGNOSTIC_STATUSES = Object.freeze([
  DIAGNOSTIC_STATUS.SUCCESS, DIAGNOSTIC_STATUS.FAILED,
  DIAGNOSTIC_STATUS.TIMEOUT, DIAGNOSTIC_STATUS.SKIPPED,
]);
