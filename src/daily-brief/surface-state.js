export const SURFACE_PHASE = Object.freeze({
  CLOSED: 'closed',
  OPENING: 'opening',
  VISIBLE: 'visible',
  CLOSING: 'closing',
  FAILED: 'failed',
});

export function createSurfaceState() {
  return {
    phase: SURFACE_PHASE.CLOSED,
    entrySource: null,
    returnContext: null,
    requestId: 0,
  };
}

export function reduceSurfaceState(state, event) {
  switch (event.type) {
    case 'open':
      if (![SURFACE_PHASE.CLOSED, SURFACE_PHASE.FAILED].includes(state.phase)) return state;
      return {
        phase: SURFACE_PHASE.OPENING,
        entrySource: event.entrySource,
        returnContext: event.returnContext ?? null,
        requestId: state.requestId + 1,
      };
    case 'ready':
      if (state.phase !== SURFACE_PHASE.OPENING) return state;
      return { ...state, phase: SURFACE_PHASE.VISIBLE };
    case 'fail':
      if (state.phase !== SURFACE_PHASE.OPENING) return state;
      return { ...state, phase: SURFACE_PHASE.FAILED };
    case 'close':
      if (![SURFACE_PHASE.OPENING, SURFACE_PHASE.VISIBLE, SURFACE_PHASE.FAILED].includes(state.phase)) return state;
      return { ...state, phase: SURFACE_PHASE.CLOSING };
    case 'closed':
      if (state.phase !== SURFACE_PHASE.CLOSING) return state;
      return createSurfaceState();
    default:
      return state;
  }
}

export function isSurfaceMounted(state) {
  return ![SURFACE_PHASE.CLOSED].includes(state.phase);
}

export function isSurfaceInteractive(state) {
  return state.phase === SURFACE_PHASE.VISIBLE;
}
