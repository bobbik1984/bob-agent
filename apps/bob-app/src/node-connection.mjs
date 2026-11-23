// Device pairing uses independent credentials; owner passwords are never persisted.
export function isPrivateOrLoopbackHost(hostname) {
  const clean = String(hostname || '').trim().replace(/^\[|\]$/g, '').toLowerCase();
  if (!clean) return false;
  if (clean === 'localhost' || clean === '::1') return true;
  const m = clean.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
  if (!m) return false;
  const octets = m.slice(1).map(Number);
  if (octets.some(o => Number.isNaN(o) || o < 0 || o > 255)) return false;
  const [a, b] = octets;
  if (a === 127 || a === 10) return true;
  if (a === 192 && b === 168) return true;
  if (a === 172 && b >= 16 && b <= 31) return true;
  if (a === 100 && b >= 64 && b <= 127) return true;
  return false;
}

export function nodeAddress(raw) {
  const trimmed = String(raw || '').trim();
  if (!trimmed) throw Error('NODE_ADDRESS_INVALID');
  let input = trimmed;
  if (!trimmed.includes('://')) {
    const hostPart = trimmed.startsWith('[')
      ? trimmed.slice(0, trimmed.indexOf(']') + 1)
      : trimmed.split(/[/:]/)[0];
    input = `${isPrivateOrLoopbackHost(hostPart) ? 'http' : 'https'}://${trimmed}`;
  }
  const url = new URL(input);
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && isPrivateOrLoopbackHost(url.hostname))) {
    throw Error('NODE_HTTPS_REQUIRED');
  }
  if (url.username || url.password || url.search || url.hash || (url.pathname !== '/' && url.pathname !== '')) {
    throw Error('NODE_ADDRESS_INVALID');
  }
  return url.origin;
}
export function nodeClient(address, fetcher = globalThis.fetch) {
  const base = nodeAddress(address);
  return async (path, body, token) => {
    const response = await fetcher(`${base}/api/node/v1/${path}`, {
      method: body === undefined ? 'GET' : 'POST', redirect: 'error', cache: 'no-store',
      signal: AbortSignal.timeout(15000),
      headers: {'Content-Type':'application/json', ...(token ? {Authorization:`Bearer ${token}`} : {})},
      ...(body === undefined ? {} : {body:JSON.stringify(body)}),
    });
    if (!response.ok) throw Error(response.status === 429 ? 'NODE_RATE_LIMITED' : 'NODE_AUTH_FAILED');
    const result = await response.json();
    if (result.error) throw Error('NODE_AUTH_FAILED');
    return result;
  };
}
export function invitationUri(base, invitation) {
  const p = new URLSearchParams({u:nodeAddress(base), c:invitation.user_code, p:invitation.pairing_id, id:invitation.node_id, r:'node', x:String(invitation.expires_at)});
  return `bob://pair?${p}`;
}
