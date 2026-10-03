import { describe, expect, it } from 'vitest';
import {
  groupWalletTickets,
  ticketBarcode,
  ticketCategory,
  ticketGroup,
  ticketKind,
  ticketCardTitle,
  ticketCardSubtitle,
  ticketFormattedDate,
  ticketFormattedDateWithWeekday,
  ticketFormattedTime
} from './wallet.js';

describe('ticket wallet projection', () => {
  const now = Date.parse('2026-10-02T12:00:00');
  it('keeps existing metadata visible without rewriting it', () => {
    const ticket = { label: '旧航班', metadata: JSON.stringify({ category: 'flight', barcode_data: 'private-qr' }) };
    expect(ticketCategory(ticket)).toBe('flight');
    expect(ticketKind(ticket)).toBe('flight');
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
    expect(ticketKind(unknown)).toBe('film');
    expect(ticketGroup(unknown, now)).toBe('undated');
  });

  it('formats titles, subtitles, dates and times for cards', () => {
    const flight = {
      label: '上海 - 北京',
      metadata: {
        category: 'flight',
        flight_info: { origin: '上海', destination: '北京', flight_number: 'MU5112', origin_terminal: '虹桥 T2' },
        start_time: '2026-10-08 09:20:00'
      }
    };
    expect(ticketKind(flight)).toBe('flight');
    expect(ticketCardTitle(flight)).toBe('上海 → 北京');
    expect(ticketCardSubtitle(flight)).toBe('MU5112 · 虹桥 T2');
    expect(ticketFormattedDate(flight)).toBe('10月8日');
    expect(ticketFormattedDateWithWeekday(flight)).toBe('10月8日 周四');
    expect(ticketFormattedTime(flight)).toBe('09:20');

    const rail = {
      label: '上海虹桥 → 杭州东',
      metadata: {
        category: 'train',
        flight_info: { flight_number: 'G7321', seat: '05车 12A' },
        start_time: '2026-10-05 14:36:00'
      }
    };
    expect(ticketKind(rail)).toBe('rail');
    expect(ticketCardTitle(rail)).toBe('上海虹桥 → 杭州东');
    expect(ticketCardSubtitle(rail)).toBe('G7321 · 05车 12A');
    expect(ticketFormattedDate(rail)).toBe('10月5日');
    expect(ticketFormattedTime(rail)).toBe('14:36');

    const film = {
      label: '午夜图书馆',
      metadata: {
        category: 'movie',
        venue: '万达影城',
        seat_info: '3厅 8排6座',
        start_time: '2026-10-12 19:30:00'
      }
    };
    expect(ticketKind(film)).toBe('film');
    expect(ticketCardTitle(film)).toBe('午夜图书馆');
    expect(ticketCardSubtitle(film)).toBe('万达影城 · 3厅 8排6座');
    expect(ticketFormattedDate(film)).toBe('10月12日');
    expect(ticketFormattedTime(film)).toBe('19:30');
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
