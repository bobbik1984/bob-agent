// Documents run in an opaque sandbox origin; never inherit Bob credentials.
export const artifactHtmlSandbox = 'allow-scripts';
export function artifactHtmlDocument(source, resources = []) {
  const policy = "default-src 'none'; script-src 'unsafe-inline' data:; style-src 'unsafe-inline' data:; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";
  const assets = new Map(resources.filter(r =>
    ['image/png','image/jpeg','image/webp','text/css','text/javascript'].includes(r.mime) &&
    /^[A-Za-z0-9+/]*={0,2}$/.test(r.data)
  ).map(r => [r.url, r]));
  // Do not parse untrusted HTML in Bob's document: even an inert DOM parser
  // may initiate resource fetches. Only substitute exact server-authorized URLs.
  source = String(source).replace(/(\b(?:src|href)\s*=\s*)(["'])([^"']*)\2/gi, (all, prefix, quote, url) => {
    const resource = assets.get(url.replace(/&amp;/g, '&'));
    return resource ? `${prefix}${quote}data:${resource.mime};base64,${resource.data}${quote}` : all;
  });
  return '<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="' + policy + '"><meta name="referrer" content="no-referrer">' + String(source);
}
