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

export async function streamJarvisTurn(payload, onEvent, options = {}) {
  if (!isBobNodeRuntime()) {
    throw new Error('NATIVE_HOST_REQUIRED');
  }
  const headers = {
    'Content-Type': 'application/json',
    Accept: 'text/event-stream',
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

  const response = await globalThis.fetch('/api/jarvis/v1/stream', {
    method: 'POST',
    credentials: 'same-origin',
    headers,
    body: JSON.stringify(payload ?? {}),
    signal: options.signal,
  });

  if (!response.ok) {
    const errPayload = await response.json().catch(() => null);
    const code =
      (errPayload && (errPayload.error || errPayload.code || errPayload.message)) ||
      `HTTP_${response.status}`;
    throw new Error(String(code));
  }

  const reader = response.body ? response.body.getReader() : null;
  if (!reader) {
    return { ok: true };
  }

  const decoder = new TextDecoder('utf-8');
  let buffer = '';
  let lastDone = null;

  function processBlock(block) {
    const lines = block.split(/\r?\n/);
    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed.startsWith('data:')) {
        const jsonStr = trimmed.slice(5).trim();
        if (!jsonStr) continue;
        try {
          const evt = JSON.parse(jsonStr);
          onEvent?.(evt);
          if (evt.type === 'done' || evt.event === 'done') {
            lastDone = evt;
          }
        } catch {}
      }
    }
  }

  while (true) {
    const { done, value } = await reader.read();
    if (done) {
      buffer += decoder.decode();
      if (buffer.trim()) {
        processBlock(buffer);
      }
      break;
    }
    buffer += decoder.decode(value, { stream: true });
    let boundaryMatch;
    while ((boundaryMatch = buffer.match(/\r?\n\r?\n/)) !== null) {
      const idx = boundaryMatch.index;
      const matchLen = boundaryMatch[0].length;
      const block = buffer.slice(0, idx);
      buffer = buffer.slice(idx + matchLen);
      processBlock(block);
    }
  }

  return lastDone || { ok: true };
}
