// Device pairing uses independent credentials; owner passwords are never persisted.
export function nodeAddress(raw) {
  const url = new URL(raw.includes('://') ? raw : `https://${raw}`);
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && ['localhost','127.0.0.1','[::1]'].includes(url.hostname))) throw Error('NODE_HTTPS_REQUIRED');
  if (url.username || url.password || url.search || url.hash || (url.pathname !== '/' && url.pathname !== '')) throw Error('NODE_ADDRESS_INVALID');
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
