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

export function ticketKind(node) {
  const cat = ticketCategory(node);
  if (cat === 'flight') return 'flight';
  if (cat === 'train') return 'rail';
  if (cat === 'movie') return 'film';
  if (cat === 'exhibition') return 'museum';
  if (cat === 'concert') return 'film';
  return 'flight';
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

export function ticketCardTitle(node) {
  const meta = ticketMetadata(node);
  const cat = ticketCategory(node);
  const label = node?.label || '';
  if (cat === 'flight' || cat === 'train') {
    const origin = meta.flight_info?.origin || (meta.venue && meta.venue.includes('-') ? meta.venue.split('-')[0].trim() : '');
    const dest = meta.flight_info?.destination || (meta.venue && meta.venue.includes('-') ? meta.venue.split('-')[1].trim() : '');
    if (origin && dest) return `${origin} → ${dest}`;
    if (label.includes('➔')) {
      const parts = label.split('➔');
      return `${parts[0].trim()} → ${parts[1].trim()}`;
    }
    if (label.includes('->')) {
      const parts = label.split('->');
      return `${parts[0].trim()} → ${parts[1].trim()}`;
    }
    if (label.includes('→')) {
      const parts = label.split('→');
      return `${parts[0].trim()} → ${parts[1].trim()}`;
    }
  }
  return label;
}

export function ticketCardSubtitle(node) {
  const meta = ticketMetadata(node);
  const cat = ticketCategory(node);
  const label = node?.label || '';
  const parts = [];

  if (cat === 'flight') {
    if (meta.flight_info?.flight_number) {
      parts.push(meta.flight_info.flight_number);
    } else {
      const match = label.match(/\b([A-Z0-9]{2}\s?\d{3,4})\b/i);
      if (match) parts.push(match[1]);
    }
    if (meta.flight_info?.origin_terminal) {
      parts.push(meta.flight_info.origin_terminal);
    }
    if (meta.flight_info?.seat || meta.seat_info) {
      parts.push(meta.flight_info?.seat || meta.seat_info);
    }
  } else if (cat === 'train') {
    const trainMatch = label.match(/\(([GDCKZTX\d]+)\)/i) || label.match(/\b([GDCKZTX]\d{1,4})\b/i);
    if (trainMatch) {
      parts.push(trainMatch[1]);
    } else if (meta.flight_info?.flight_number) {
      parts.push(meta.flight_info.flight_number);
    }
    if (meta.seat_info || meta.flight_info?.seat) {
      parts.push(meta.seat_info || meta.flight_info?.seat);
    }
  } else {
    if (meta.venue) parts.push(meta.venue);
    if (meta.seat_info) parts.push(meta.seat_info);
  }

  if (parts.length > 0) return parts.join(' · ');
  return meta.venue || meta.summary || node?.summary || '';
}

export function ticketFormattedDate(node) {
  const meta = ticketMetadata(node);
  const raw = meta.start_time || meta.date;
  if (!raw) return '';
  const str = String(raw).split(' ')[0];
  const parts = str.split('-');
  if (parts.length === 3) {
    const month = parseInt(parts[1], 10);
    const day = parseInt(parts[2], 10);
    if (month && day) return `${month}月${day}日`;
  }
  return str;
}

export function ticketFormattedTime(node) {
  const meta = ticketMetadata(node);
  const raw = meta.start_time || meta.date || '';
  if (raw.includes(' ')) {
    const timePart = raw.split(' ')[1];
    if (timePart && timePart !== '00:00:00') {
      return timePart.substring(0, 5);
    }
  }
  if (meta.time) return String(meta.time);
  return '';
}
