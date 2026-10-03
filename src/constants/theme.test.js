import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { ACCENT_COLORS, BOB_BRAND_BLUE, normalizeAccentColor } from './theme.js';

describe('default Bob brand color', () => {
  it('uses the vector logo fill as the light-theme accent source', () => {
    const logo = readFileSync(new URL('../../public/bob_logo.svg', import.meta.url), 'utf8');
    const css = readFileSync(new URL('../index.css', import.meta.url), 'utf8');
    const logoFill = logo.match(/\.st0\{fill:(#[\da-fA-F]{6})/i)?.[1];
    expect(logoFill?.toUpperCase()).toBe(BOB_BRAND_BLUE);
    expect(css).toContain(`--bob-brand-blue: ${BOB_BRAND_BLUE};`);
    expect(css).toContain('--user-accent:       var(--bob-brand-blue);');
    expect(ACCENT_COLORS[0].value).toBe(BOB_BRAND_BLUE);
  });

  it('ignores invalid stored colors while retaining deliberate theme choices', () => {
    expect(normalizeAccentColor('#2776bb')).toBe(BOB_BRAND_BLUE);
    expect(normalizeAccentColor('#57b5c3')).toBe('#57B5C3');
    expect(normalizeAccentColor('blue')).toBeNull();
    expect(normalizeAccentColor(null)).toBeNull();
  });
});
