// Only a new durable receipt may cause execution. Refresh/restart never dispatch.
export function createExecutionController(bridge) {
  const attempted = new Map();
  const cancelling = new Map();
  function dispatch(receipt) {
    const id = receipt?.request_id;
    if (!id) return Promise.reject(new Error('REQUEST_ID_REQUIRED'));
    if (attempted.has(id)) return attempted.get(id);
    const pending = Promise.resolve().then(() => bridge.execute(id)).then(execution => {
      if (execution?.request_id !== id || !execution?.id) throw new Error('EXECUTION_RECEIPT_MISMATCH');
      return execution;
    });
    // Retain rejected attempts too: missing IPC acknowledgement is not non-execution.
    attempted.set(id, pending);
    return pending;
  }
  function cancel(execution) {
    if (!canCancelExecution(execution)) return Promise.reject(new Error('EXECUTION_NOT_CANCELLABLE'));
    if (cancelling.has(execution.id)) return cancelling.get(execution.id);
    const pending = Promise.resolve().then(() => bridge.cancel(execution.id)).then(receipt => {
      if (receipt?.id !== execution.id) throw new Error('CANCEL_RECEIPT_MISMATCH');
      return receipt;
    }).finally(() => cancelling.delete(execution.id));
    cancelling.set(execution.id, pending);
    return pending;
  }
  return { dispatch, cancel };
}

export function canCancelExecution(execution) {
  return !!execution && !execution.stop_requested && ['pending', 'dispatching', 'running'].includes(execution.status);
}

export function executionStatusKey(execution) {
  if (!execution) return null;
  if (execution.status === 'unknown') return 'executionUnknown';
  if (execution.stop_requested && ['pending', 'dispatching', 'running'].includes(execution.status)) return 'executionStopRequested';
  if (execution.status === 'cancelled') return 'executionCancelled';
  if (execution.status === 'failed' && execution.error === 'restart_interrupted') return 'executionInterrupted';
  if (execution.status === 'failed' && /timeout/i.test(execution.error || '')) return 'executionTimeout';
  return ({pending:'executionQueued',dispatching:'executionStarting',running:'executionRunning',completed:'executionCompleted',failed:'executionFailed'})[execution.status] || 'executionUnknown';
}

export function singleFlight(work) {
  let pending = null;
  return (...args) => {
    if (!pending) pending = Promise.resolve().then(() => work(...args)).finally(() => { pending = null; });
    return pending;
  };
}

export function blocksNewExecution(executions) {
  return executions.some(execution => ['pending', 'dispatching', 'running', 'paused', 'unknown'].includes(execution.status));
}
