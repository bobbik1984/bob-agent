export function ticketMetadata(node) {
  const raw = node?.metadata;
  if (raw && typeof raw === 'object' && !Array.isArray(raw)) return raw;
  if (typeof raw !== 'string' || !raw.trim()) return {};
  try {
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

export function ticketCategory(node) {
  const meta = ticketMetadata(node);
  const declared = String(meta.category || '').toLowerCase();
  if (['flight', 'train', 'movie', 'exhibition', 'concert', 'membership'].includes(declared)) return declared;
  const clues = `${node?.label || ''} ${node?.summary || ''}`.toLowerCase();
  if (/flight|airline|boarding|航班|机票|登机/.test(clues)) return 'flight';
  if (/train|rail|高铁|火车|动车/.test(clues)) return 'train';
  if (/movie|cinema|电影|影院/.test(clues)) return 'movie';
  if (/exhibition|museum|展览|博物馆|美术馆/.test(clues)) return 'exhibition';
  if (/concert|演唱会|音乐会/.test(clues)) return 'concert';
  return 'general';
}

export function ticketBarcode(node) {
  const meta = ticketMetadata(node);
  for (const key of ['barcode_data', 'qr_code', 'qr_data']) {
    if (typeof meta[key] === 'string' && meta[key].trim()) return meta[key];
  }
  return '';
}

export function ticketDate(node) {
  const meta = ticketMetadata(node);
  const value = meta.end_time || meta.start_time || meta.date;
  if (!value) return null;
  const parsed = Date.parse(String(value).replace(' ', 'T'));
  return Number.isFinite(parsed) ? parsed : null;
}

export function ticketGroup(node, now = Date.now()) {
  const meta = ticketMetadata(node);
  const end = ticketDate(node);
  if (end === null) return 'undated';
  const hasEnd = Boolean(meta.end_time);
  return now > end + (hasEnd ? 0 : 24 * 60 * 60 * 1000) ? 'expired' : 'upcoming';
}

export function groupWalletTickets(nodes, now = Date.now()) {
  const groups = { upcoming: [], undated: [], expired: [] };
  for (const node of nodes) groups[ticketGroup(node, now)].push(node);
  groups.upcoming.sort((a, b) => ticketDate(a) - ticketDate(b));
  groups.expired.sort((a, b) => ticketDate(b) - ticketDate(a));
  return groups;
}
