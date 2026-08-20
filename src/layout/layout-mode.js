const MOBILE_USER_AGENT = /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i;

export function detectTerminalKind(userAgent = '') {
  return MOBILE_USER_AGENT.test(userAgent) ? 'native-mobile' : 'desktop';
}

export function detectViewportShape(width, height) {
  return Number(height) > Number(width) ? 'portrait' : 'landscape';
}

export function resolveLayoutMode(terminalKind, viewportShape) {
  if (terminalKind === 'native-mobile') return 'mobile-native';
  return viewportShape === 'portrait' ? 'desktop-compact' : 'desktop-wide';
}

export function createLayoutState({ width, height, userAgent = '' }) {
  const terminalKind = detectTerminalKind(userAgent);
  const viewportShape = detectViewportShape(width, height);
  const layoutMode = resolveLayoutMode(terminalKind, viewportShape);
  return {
    terminalKind,
    viewportShape,
    layoutMode,
    compactNavigation: layoutMode !== 'desktop-wide',
  };
}
