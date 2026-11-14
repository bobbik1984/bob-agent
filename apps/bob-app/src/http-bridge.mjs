// HTTP + SSE RPC transport for Unified Web App hosted by `bob-node` (`/api/rpc/:command`).
// Only activates when served by a real `bob-node` instance (`window.__BOB_NODE_RUNTIME__ === true`),
// never fabricating a browser-only mock store.

export const BOB_NODE_TOKEN_STORAGE_KEY = 'bob_node_session_token';

export function isBobNodeRuntime() {
  if (typeof globalThis.window === 'undefined') return false;
  return Boolean(
    globalThis.window.__BOB_NODE_RUNTIME__ === true ||
      globalThis.document?.documentElement?.dataset?.bobNode === 'true',
  );
}

export function getBobNodeSessionToken() {
  try {
    if (typeof globalThis.localStorage !== 'undefined') {
      const stored = globalThis.localStorage.getItem(BOB_NODE_TOKEN_STORAGE_KEY);
      if (stored) return stored;
    }
  } catch {}
  return globalThis.window?.__BOB_NODE_TOKEN__ || null;
}

export function setBobNodeSessionToken(token) {
  let persisted = false;
  try {
    if (typeof globalThis.localStorage !== 'undefined') {
      if (token) {
        globalThis.localStorage.setItem(BOB_NODE_TOKEN_STORAGE_KEY, String(token));
      } else {
        globalThis.localStorage.removeItem(BOB_NODE_TOKEN_STORAGE_KEY);
      }
      persisted = true;
    }
  } catch {}
  if (globalThis.window && typeof globalThis.window === 'object') {
    if (!token) {
      delete globalThis.window.__BOB_NODE_TOKEN__;
    } else if (!persisted || globalThis.window.__BOB_NODE_TOKEN__ !== undefined) {
      globalThis.window.__BOB_NODE_TOKEN__ = String(token);
    }
  }
}

function setOnboardingPromptSuppressed(active) {
  if (globalThis.window && typeof globalThis.window === 'object') {
    globalThis.window.__BOB_ONBOARDING_ACTIVE__ = Boolean(active);
  }
}

function buildAuthHeaders(extra = {}) {
  const headers = {
    'X-Bob-Rpc': '1',
    ...extra,
  };
  const sessionToken = getBobNodeSessionToken();
  if (sessionToken) {
    headers.Authorization = `Bearer ${sessionToken}`;
  }
  return headers;
}

function createOnboardingHttpError(response, resBody, fallbackCode) {
  const code = String(
    (resBody && (resBody.error || resBody.code)) ||
      fallbackCode ||
      `HTTP_${response?.status || 500}`,
  );
  const detail =
    resBody && typeof resBody.message === 'string' && resBody.message.trim()
      ? resBody.message.trim()
      : '';
  const msg = detail && !detail.includes(code) ? `${code}: ${detail}` : detail || code;
  const err = new Error(msg);
  err.code = code;
  err.detail = detail;
  err.status = response?.status ?? 0;

  const rawRetry =
    resBody?.retryAfterSec ??
    resBody?.retry_after_sec ??
    (typeof response?.headers?.get === 'function' ? response.headers.get('Retry-After') : null);
  if (rawRetry !== null && rawRetry !== undefined && rawRetry !== '') {
    const num = Number(rawRetry);
    if (Number.isFinite(num)) {
      err.retryAfterSec = Math.max(0, Math.ceil(num));
    }
  }
  return err;
}

export async function fetchOnboardingStatus() {
  const fallback = { needsOnboarding: false, presets: [] };
  if (!isBobNodeRuntime() || typeof globalThis.fetch !== 'function') {
    return fallback;
  }
  try {
    const response = await globalThis.fetch('/api/node/v1/onboarding/status', {
      method: 'GET',
      credentials: 'same-origin',
      headers: buildAuthHeaders({ Accept: 'application/json' }),
    });
    if (!response || !response.ok) {
      return fallback;
    }
    const payload = await response.json().catch(() => null);
    const data =
      payload && typeof payload === 'object' && payload.result && typeof payload.result === 'object'
        ? payload.result
        : payload;
    if (!data || typeof data !== 'object') {
      return fallback;
    }
    const needsOnboarding = Boolean(data.needsOnboarding ?? data.needs_onboarding);
    setOnboardingPromptSuppressed(needsOnboarding);
    return {
      needsOnboarding,
      passwordConfigured: Boolean(data.passwordConfigured ?? data.password_configured),
      providerConfigured: Boolean(data.providerConfigured ?? data.provider_configured),
      pairingCodeRequired:
        (data.pairingCodeRequired ?? data.pairing_code_required) !== false,
      pairingExpiresAt: data.pairingExpiresAt ?? data.pairing_expires_at ?? null,
      lockedRetryAfterSec: Number(
        data.lockedRetryAfterSec ?? data.locked_retry_after_sec ?? 0,
      ),
      presets: Array.isArray(data.presets) ? data.presets : [],
    };
  } catch {
    return fallback;
  }
}

export async function claimOnboardingNode({ pairingCode, password, clientName } = {}) {
  if (!isBobNodeRuntime()) {
    throw new Error('NATIVE_HOST_REQUIRED');
  }
  const response = await globalThis.fetch('/api/node/v1/onboarding/claim', {
    method: 'POST',
    credentials: 'same-origin',
    headers: buildAuthHeaders({
      'Content-Type': 'application/json',
      Accept: 'application/json',
    }),
    body: JSON.stringify({
      pairingCode: String(pairingCode ?? '').trim(),
      password: String(password ?? ''),
      clientName: clientName || 'Bob Web Client',
    }),
  });
  const resBody = await response.json().catch(() => null);
  if (!response.ok || (resBody && resBody.ok === false)) {
    throw createOnboardingHttpError(response, resBody, 'CLAIM_FAILED');
  }
  const body =
    resBody && typeof resBody === 'object' && resBody.result && typeof resBody.result === 'object'
      ? { ...resBody.result, ok: resBody.ok ?? true }
      : resBody || { ok: true };
  const accessToken = body.accessToken || body.access_token || null;
  if (accessToken) {
    setBobNodeSessionToken(accessToken);
  }
  return body;
}

export async function configureOnboardingProvider({
  providerId,
  apiKey,
  baseUrl,
  clerkModel,
  makerModel,
  skipConnectivityTest = false,
} = {}) {
  if (!isBobNodeRuntime()) {
    throw new Error('NATIVE_HOST_REQUIRED');
  }
  const reqPayload = {
    providerId: String(providerId ?? ''),
    apiKey: String(apiKey ?? ''),
    skipConnectivityTest: Boolean(skipConnectivityTest),
  };
  if (baseUrl !== undefined && baseUrl !== null && String(baseUrl).trim() !== '') {
    reqPayload.baseUrl = String(baseUrl).trim();
  }
  if (clerkModel !== undefined && clerkModel !== null && String(clerkModel).trim() !== '') {
    reqPayload.clerkModel = String(clerkModel).trim();
  }
  if (makerModel !== undefined && makerModel !== null && String(makerModel).trim() !== '') {
    reqPayload.makerModel = String(makerModel).trim();
  }

  const response = await globalThis.fetch('/api/node/v1/onboarding/provider', {
    method: 'POST',
    credentials: 'same-origin',
    headers: buildAuthHeaders({
      'Content-Type': 'application/json',
      Accept: 'application/json',
    }),
    body: JSON.stringify(reqPayload),
  });
  const resBody = await response.json().catch(() => null);
  if (!response.ok || (resBody && resBody.ok === false)) {
    throw createOnboardingHttpError(response, resBody, 'PROVIDER_CONFIG_FAILED');
  }
  return resBody || { ok: true };
}

export async function completeOnboardingPersona({ assistantName = '' } = {}) {
  if (!isBobNodeRuntime()) {
    throw new Error('NATIVE_HOST_REQUIRED');
  }
  const response = await globalThis.fetch('/api/node/v1/onboarding/persona', {
    method: 'POST',
    credentials: 'same-origin',
    headers: buildAuthHeaders({
      'Content-Type': 'application/json',
      Accept: 'application/json',
    }),
    body: JSON.stringify({
      assistantName: String(assistantName ?? '').trim(),
    }),
  });
  const resBody = await response.json().catch(() => null);
  if (!response.ok || (resBody && resBody.ok === false)) {
    throw createOnboardingHttpError(response, resBody, 'PERSONA_SAVE_FAILED');
  }
  setOnboardingPromptSuppressed(false);
  return resBody || { ok: true };
}

export async function httpInvoke(command, args = {}) {
  if (!isBobNodeRuntime()) {
    throw new Error('NATIVE_HOST_REQUIRED');
  }
  // Never invoke raw browser window.prompt password dialogs; rely strictly on token headers or OnboardingWizard.
  const headers = buildAuthHeaders({
    'Content-Type': 'application/json',
    Accept: 'application/json',
  });
  const response = await globalThis.fetch(`/api/rpc/${encodeURIComponent(command)}`, {
    method: 'POST',
    credentials: 'same-origin',
    headers,
    body: JSON.stringify(args ?? {}),
  });
  const payload = await response.json().catch(() => null);
  if (!response.ok) {
    if (payload && payload.needsOnboarding) {
      setOnboardingPromptSuppressed(true);
    }
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
  const headers = buildAuthHeaders({
    'Content-Type': 'application/json',
    Accept: 'text/event-stream',
  });

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
