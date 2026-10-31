// HTTP + SSE RPC transport for Unified Web App hosted by `bob-node` (`/api/rpc/:command`).
// Only activates when served by a real `bob-node` instance (`window.__BOB_NODE_RUNTIME__ === true`),
// never fabricating a browser-only mock store.

export function isBobNodeRuntime() {
  if (typeof globalThis.window === 'undefined') return false;
  return Boolean(
    globalThis.window.__BOB_NODE_RUNTIME__ === true ||
      globalThis.document?.documentElement?.dataset?.bobNode === 'true',
  );
}

export async function httpInvoke(command, args = {}) {
  if (!isBobNodeRuntime()) {
    throw new Error('NATIVE_HOST_REQUIRED');
  }
  const headers = {
    'Content-Type': 'application/json',
    Accept: 'application/json',
    'X-Bob-Rpc': '1',
  };
  const sessionToken =
    globalThis.window?.__BOB_NODE_TOKEN__ ||
    (typeof globalThis.localStorage !== 'undefined'
      ? globalThis.localStorage.getItem('bob_node_session_token')
      : null);
  if (sessionToken) {
    headers.Authorization = `Bearer ${sessionToken}`;
  }
  const response = await globalThis.fetch(`/api/rpc/${encodeURIComponent(command)}`, {
    method: 'POST',
    credentials: 'same-origin',
    headers,
    body: JSON.stringify(args ?? {}),
  });
  const payload = await response.json().catch(() => null);
  if (!response.ok) {
    const code =
      (payload && (payload.error || payload.code || payload.message)) ||
      `HTTP_${response.status}`;
    throw new Error(String(code));
  }
  if (payload && typeof payload === 'object' && 'ok' in payload) {
    if (!payload.ok) {
      throw new Error(String(payload.error || 'RPC_FAILED'));
    }
    return payload.result;
  }
  return payload;
}
