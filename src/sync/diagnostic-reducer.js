import { ALL_DIAGNOSTIC_STAGES, DIAGNOSTIC_STATUS, TERMINAL_DIAGNOSTIC_STATUSES } from './diagnostic-contract.js';

export function createDiagnosticTrace(traceId, protocolVersion = 2) {
  return {
    trace_id: traceId,
    protocol_version: protocolVersion,
    overall_status: DIAGNOSTIC_STATUS.PENDING,
    paths: Object.fromEntries(ALL_DIAGNOSTIC_STAGES.map((stage) => [stage, {
      stage, status: DIAGNOSTIC_STATUS.PENDING, sequence: 0, updated_at: 0,
      error_code: null, detail: null,
    }])),
    seen_events: [],
  };
}

export function reduceDiagnosticEvent(trace, event) {
  if (!trace || !event || event.trace_id !== trace.trace_id || !trace.paths[event.stage]) return trace;
  const key = `${event.message_id}:${event.stage}:${event.status}:${event.sequence}`;
  if (trace.seen_events.includes(key)) return trace;
  const current = trace.paths[event.stage];
  const terminal = TERMINAL_DIAGNOSTIC_STATUSES.includes(current.status);
  if (event.sequence < current.sequence
    || (terminal && event.sequence <= current.sequence && current.status !== event.status)) return trace;

  const paths = { ...trace.paths, [event.stage]: {
    stage: event.stage, status: event.status, sequence: event.sequence,
    updated_at: event.timestamp, error_code: event.error_code ?? null, detail: event.detail ?? null,
  } };
  return { ...trace, paths, seen_events: [...trace.seen_events, key], overall_status: deriveOverallStatus(paths) };
}

export function deriveOverallStatus(paths) {
  const statuses = Object.values(paths).map((path) => path.status);
  if (statuses.includes(DIAGNOSTIC_STATUS.RUNNING)) return DIAGNOSTIC_STATUS.RUNNING;
  if (statuses.includes(DIAGNOSTIC_STATUS.FAILED)) return DIAGNOSTIC_STATUS.FAILED;
  if (statuses.includes(DIAGNOSTIC_STATUS.TIMEOUT)) return DIAGNOSTIC_STATUS.TIMEOUT;
  if (statuses.includes(DIAGNOSTIC_STATUS.UNKNOWN)) return DIAGNOSTIC_STATUS.UNKNOWN;
  if (statuses.includes(DIAGNOSTIC_STATUS.SUCCESS)) return DIAGNOSTIC_STATUS.SUCCESS;
  if (statuses.every((status) => status === DIAGNOSTIC_STATUS.SKIPPED)) return DIAGNOSTIC_STATUS.SKIPPED;
  return DIAGNOSTIC_STATUS.PENDING;
}
