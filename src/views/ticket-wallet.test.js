import { describe, it, expect } from 'vitest';

function getNodeCategory(node) {
  let meta = {};
  try {
    meta = typeof node.metadata === 'string' && node.metadata ? JSON.parse(node.metadata) : (node.metadata || {});
  } catch (e) {}

  const cat = (meta.category || '').toLowerCase();
  if (cat === 'flight' || cat === 'air') return 'flight';
  if (cat === 'train' || cat === 'rail') return 'train';
  if (cat === 'movie' || cat === 'film' || cat === 'cinema') return 'entertainment';
  if (cat === 'concert' || cat === 'music' || cat === 'show' || cat === 'exhibition' || cat === 'museum') return 'entertainment';

  const text = `${node.label || ''} ${node.summary || ''} ${meta.venue || ''}`;

  // Check train first to avoid G\d+ matching generic 2-char flight prefixes
  if (/(\b[GDCKZTX]\d{1,4}\b)|(\([GDCKZTX\d]+\))|高铁|动车|列车|火车站|车次/i.test(text)) {
    return 'train';
  }
  // Check movie / entertainment
  if (/电影|影城|影院|激光厅|IMAX|放映|音乐|演出|话剧|演唱会|展览|博览/i.test(text)) {
    return 'entertainment';
  }
  // Flight regex: standard 2-letter IATA carrier code followed by 3-4 digits
  if (/(\b[A-Z]{2}\s?\d{3,4}\b)|(\b[A-Z][0-9]\s?\d{3,4}\b)|(\b[0-9][A-Z]\s?\d{3,4}\b)|([A-Z]{3}\s*[-–➔✈]\s*[A-Z]{3})|航班|机票|登机牌/i.test(text)) {
    return 'flight';
  }
  return 'general';
}

function isNodeExpired(node, nowMs = Date.now()) {
  let meta = {};
  try {
    meta = typeof node.metadata === 'string' && node.metadata ? JSON.parse(node.metadata) : (node.metadata || {});
  } catch (e) {}
  if (!meta.start_time) return false;
  const dtStr = meta.start_time.replace(' ', 'T');
  const startTime = new Date(dtStr).getTime();
  if (isNaN(startTime)) return false;
  return nowMs > startTime + 24 * 3600 * 1000;
}

describe('Ticket Wallet Logic', () => {
  it('correctly categorizes explicit flight and inferred flight nodes', () => {
    const flight1 = {
      label: 'CA1376 SZX-PEK',
      metadata: JSON.stringify({ category: 'flight', start_time: '2026-08-22 14:55:00' })
    };
    expect(getNodeCategory(flight1)).toBe('flight');

    // Inferred from flight code and route
    const flight2 = {
      label: 'HU7336 CSX-PEK',
      metadata: '{}'
    };
    expect(getNodeCategory(flight2)).toBe('flight');
  });

  it('correctly categorizes high speed train and inferred train nodes', () => {
    const train1 = {
      label: '深圳北 ➔ 广州南 (G6510)',
      metadata: JSON.stringify({ category: 'train', start_time: '2026-07-15 09:38:00' })
    };
    expect(getNodeCategory(train1)).toBe('train');

    // Inferred from train code G6535
    const train2 = {
      label: '深圳北 ➔ 香港西九龙 (G6535)',
      metadata: '{}'
    };
    expect(getNodeCategory(train2)).toBe('train');
  });

  it('correctly categorizes movie tickets', () => {
    const movie1 = {
      label: '谍网追凶 (2张)',
      metadata: JSON.stringify({ category: 'movie', venue: '百老汇电影中心 3号激光厅' })
    };
    expect(getNodeCategory(movie1)).toBe('entertainment');

    const movie2 = {
      label: '奥本海默 IMAX',
      metadata: '{}'
    };
    expect(getNodeCategory(movie2)).toBe('entertainment');
  });

  it('identifies expired tickets accurately', () => {
    const pastTicket = {
      metadata: JSON.stringify({ start_time: '2025-01-01 10:00:00' })
    };
    const futureTicket = {
      metadata: JSON.stringify({ start_time: '2099-01-01 10:00:00' })
    };
    const now = new Date('2026-09-30T00:00:00Z').getTime();

    expect(isNodeExpired(pastTicket, now)).toBe(true);
    expect(isNodeExpired(futureTicket, now)).toBe(false);
  });
});
