import { describe, expect, it } from 'vitest';
import { createLayoutState, detectTerminalKind, detectViewportShape, resolveLayoutMode } from './layout-mode.js';

describe('shared layout mode', () => {
  it('keeps a landscape Android device in native mobile mode', () => {
    expect(createLayoutState({ width: 900, height: 420, userAgent: 'Mozilla/5.0 Android' })).toEqual({
      terminalKind: 'native-mobile',
      viewportShape: 'landscape',
      layoutMode: 'mobile-native',
      compactNavigation: true,
    });
  });

  it('uses compact desktop layout for a portrait desktop window', () => {
    expect(createLayoutState({ width: 700, height: 1000, userAgent: 'Windows NT 10.0' }).layoutMode).toBe('desktop-compact');
  });

  it('uses wide desktop layout for a landscape desktop window', () => {
    expect(createLayoutState({ width: 1200, height: 800, userAgent: 'Windows NT 10.0' }).layoutMode).toBe('desktop-wide');
  });

  it('treats a square viewport as landscape consistently', () => {
    expect(detectViewportShape(800, 800)).toBe('landscape');
    expect(resolveLayoutMode(detectTerminalKind('Windows'), 'landscape')).toBe('desktop-wide');
  });
});
