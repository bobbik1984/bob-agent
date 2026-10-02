import { describe, expect, it } from 'vitest';
import { groupWalletTickets, ticketBarcode, ticketCategory, ticketGroup } from './wallet.js';

describe('ticket wallet projection', () => {
  const now = Date.parse('2026-10-02T12:00:00');
  it('keeps existing metadata visible without rewriting it', () => {
    const ticket = { label: '旧航班', metadata: JSON.stringify({ category: 'flight', barcode_data: 'private-qr' }) };
    expect(ticketCategory(ticket)).toBe('flight');
    expect(ticketBarcode(ticket)).toBe('private-qr');
    expect(ticket.metadata).toContain('private-qr');
  });

  it('separates upcoming, expired and unknown dates without inventing status', () => {
    const future = { id: 'future', metadata: { start_time: '2026-10-04 09:00:00' } };
    const past = { id: 'past', metadata: { start_time: '2026-09-01 09:00:00' } };
    const unknown = { id: 'unknown', label: '电影票' };
    const groups = groupWalletTickets([past, unknown, future], now);
    expect(groups.upcoming.map(t => t.id)).toEqual(['future']);
    expect(groups.expired.map(t => t.id)).toEqual(['past']);
    expect(groups.undated.map(t => t.id)).toEqual(['unknown']);
    expect(ticketCategory(unknown)).toBe('movie');
    expect(ticketGroup(unknown, now)).toBe('undated');
  });

  it('keeps a ten-ticket mixed wallet in time order across types', () => {
    const categories = ['flight', 'flight', 'flight', 'flight', 'flight', 'movie', 'movie', 'train', 'train', 'exhibition'];
    const tickets = categories.map((category, index) => ({
      id: `${index}`,
      metadata: { category, start_time: `2026-10-${String(index + 3).padStart(2, '0')} 09:00:00` },
    }));
    const groups = groupWalletTickets(tickets.reverse(), now);
    expect(groups.upcoming.map(ticket => ticket.id)).toEqual(categories.map((_, index) => `${index}`));
    expect(groups.upcoming.map(ticketCategory)).toEqual(categories);
  });
});
