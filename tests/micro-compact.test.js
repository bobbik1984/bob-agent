import { describe, it, expect } from 'vitest';
import { microCompact } from '../electron/services/micro-compact.js';

describe('Micro-compact', () => {
  it('should not truncate short messages', () => {
    const messages = [
      { role: 'user', content: 'hello' },
      { role: 'assistant', content: 'short reply' }
    ];
    const result = microCompact(messages, { threshold: 8000 });
    expect(result).toEqual(messages);
  });

  it('should truncate long assistant messages to head + marker + tail', () => {
    const headStr = 'A'.repeat(2000);
    const midStr = 'B'.repeat(5000);
    const tailStr = 'C'.repeat(2000);
    const content = headStr + midStr + tailStr; // total length 9000

    const messages = [
      { role: 'assistant', content }
    ];

    const result = microCompact(messages, { threshold: 8000, keepHead: 2000, keepTail: 2000 });

    expect(result.length).toBe(1);
    expect(result[0].role).toBe('assistant');
    expect(result[0].content).toContain(headStr);
    expect(result[0].content).toContain(tailStr);
    expect(result[0].content).toContain('[... micro-compact: truncated 5000 chars ...]');
    expect(result[0].content.length).toBe(2000 + 2000 + `\n\n[... micro-compact: truncated 5000 chars ...]\n\n`.length);
  });

  it('should never truncate user and system messages', () => {
    const longContent = 'A'.repeat(10000);
    const messages = [
      { role: 'system', content: longContent },
      { role: 'user', content: longContent }
    ];
    const result = microCompact(messages, { threshold: 8000 });
    expect(result[0].content.length).toBe(10000);
    expect(result[1].content.length).toBe(10000);
  });

  it('should apply custom threshold parameters', () => {
    const content = 'A'.repeat(100);
    const messages = [
      { role: 'assistant', content }
    ];

    // threshold 50, keep head 10, keep tail 10
    const result = microCompact(messages, { threshold: 50, keepHead: 10, keepTail: 10 });

    expect(result[0].content).toContain('AAAAAAAAAA\n\n[... micro-compact: truncated 80 chars ...]\n\nAAAAAAAAAA');
  });

  it('should not crash on empty content messages', () => {
    const messages = [
      { role: 'assistant', content: null },
      { role: 'assistant', content: '' },
      { role: 'assistant' } // undefined content
    ];
    const result = microCompact(messages);
    expect(result).toEqual(messages);
  });

  it('should not modify the original array (immutability)', () => {
    const content = 'A'.repeat(10000);
    const messages = [
      { role: 'assistant', content }
    ];

    const originalMessages = JSON.parse(JSON.stringify(messages));

    microCompact(messages, { threshold: 8000 });

    expect(messages).toEqual(originalMessages);
  });
});
