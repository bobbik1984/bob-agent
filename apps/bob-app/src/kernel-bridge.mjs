// Pure IPC mapping; production injects Tauri invoke, tests may inject a recorder.
// A browser/mock store must never impersonate the native SQLite kernel.
export function createKernelBridge(invoke, isAvailable) {
  function call(command, args) {
    if (!isAvailable()) return Promise.reject(new Error('NATIVE_HOST_REQUIRED'));
    return invoke(command, args);
  }
  return {
    available: isAvailable,
    bootstrap: () => call('app_bootstrap'),
    readiness: () => call('app_readiness'),
    history: sessionId => call('app_history', { sessionId }),
    acceptMessage: request => call('app_accept', { requestId: request.request_id, messageId: request.message_id, sessionId: request.session_id, content: request.content }),
    prewarm: () => call('app_prewarm'),
    execute: requestId => call('app_execute', { requestId }),
    executions: sessionId => call('app_executions', { sessionId }),
    cancel: executionId => call('app_cancel', { executionId }),
  };
}
