// M0 Online Work Service client helper (`bob-core-client/1.0`, `/api/client/v1`).
// Pure protocol helper with injected fetch & crypto; never auto-connects without explicit user action.

export const CLIENT_PROTOCOL = 'bob-core-client/1.0';

function toBase64Url(bytes) {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 1) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/g, '');
}

export async function generatePkcePair(cryptoObj = globalThis.crypto) {
  const raw = new Uint8Array(32);
  cryptoObj.getRandomValues(raw);
  const verifier = toBase64Url(raw);
  const encoded = new TextEncoder().encode(verifier);
  const digest = await cryptoObj.subtle.digest('SHA-256', encoded);
  const challenge = toBase64Url(new Uint8Array(digest));
  return { verifier, challenge, challenge_method: 'S256' };
}

export function normalizeOnlineOrigin(rawOrigin) {
  const trimmed = String(rawOrigin || '').trim().replace(/\/+$/g, '');
  if (!trimmed) throw new Error('ONLINE_ORIGIN_REQUIRED');
  const parsed = new URL(trimmed);
  if (parsed.protocol !== 'https:' && parsed.hostname !== '127.0.0.1' && parsed.hostname !== 'localhost') {
    throw new Error('ONLINE_HTTPS_REQUIRED');
  }
  return parsed.origin;
}

export function createOnlineWorkClient({ fetchImpl = globalThis.fetch, cryptoObj = globalThis.crypto } = {}) {
  async function requestJson(origin, path, { method = 'GET', token = null, body = undefined } = {}) {
    const base = normalizeOnlineOrigin(origin);
    const headers = { Accept: 'application/json' };
    if (token) headers.Authorization = `Bearer ${token}`;
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    const response = await fetchImpl(`${base}${path}`, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok && response.status !== 202) {
      const err = new Error(data?.code || `HTTP_${response.status}`);
      err.status = response.status;
      err.payload = data;
      throw err;
    }
    return { status: response.status, data };
  }

  return {
    async beginPairing({ origin, clientInstanceId, clientName }) {
      const pkce = await generatePkcePair(cryptoObj);
      const { data } = await requestJson(origin, '/api/client/v1/pairings', {
        method: 'POST',
        body: {
          client_instance_id: clientInstanceId,
          client_name: clientName || 'Bob Unified Client',
          protocol_version: CLIENT_PROTOCOL,
          challenge_method: 'S256',
          challenge: pkce.challenge,
        },
      });
      return {
        ...data,
        verifier: pkce.verifier,
        client_instance_id: clientInstanceId,
        origin: normalizeOnlineOrigin(origin),
      };
    },

    async exchangePairing({ origin, pairingId, clientInstanceId, verifier }) {
      const { status, data } = await requestJson(
        origin,
        `/api/client/v1/pairings/${encodeURIComponent(pairingId)}/exchange`,
        {
          method: 'POST',
          body: {
            client_instance_id: clientInstanceId,
            verifier,
          },
        },
      );
      return { pending: status === 202 || data?.status === 'pending', ...data };
    },

    async fetchCapabilities({ origin, token }) {
      const { data } = await requestJson(origin, '/api/client/v1/capabilities', { token });
      return data;
    },

    async fetchProjects({ origin, token }) {
      const { data } = await requestJson(origin, '/api/client/v1/projects', { token });
      return data;
    },

    async fetchProjectTasks({ origin, token, projectId }) {
      const { data } = await requestJson(
        origin,
        `/api/client/v1/projects/${encodeURIComponent(projectId)}/tasks`,
        { token },
      );
      return data;
    },

    async fetchProjectMessages({ origin, token, projectId, after = 0 }) {
      const { data } = await requestJson(
        origin,
        `/api/client/v1/projects/${encodeURIComponent(projectId)}/messages?after=${Number(after) || 0}`,
        { token },
      );
      return data;
    },

    async fetchTaskSnapshot({ origin, token, taskId }) {
      const { data } = await requestJson(
        origin,
        `/api/client/v1/tasks/${encodeURIComponent(taskId)}/snapshot`,
        { token },
      );
      return data;
    },

    async revokeSelf({ origin, token }) {
      const { data } = await requestJson(origin, '/api/client/v1/credentials/self/revoke', {
        method: 'POST',
        token,
        body: {},
      });
      return data;
    },
  };
}
