import { describe, expect, it } from 'vitest';
import {
  SURFACE_PHASE,
  createSurfaceState,
  isSurfaceInteractive,
  isSurfaceMounted,
  reduceSurfaceState,
} from './surface-state.js';

describe('Today Layer surface state', () => {
  it('opens, becomes ready, and closes deterministically', () => {
    let state = createSurfaceState();
    state = reduceSurfaceState(state, { type: 'open', entrySource: 'quick_note', returnContext: { draft: 'idea' } });
    expect(state.phase).toBe(SURFACE_PHASE.OPENING);
    expect(isSurfaceMounted(state)).toBe(true);
    expect(isSurfaceInteractive(state)).toBe(false);
    state = reduceSurfaceState(state, { type: 'ready' });
    expect(isSurfaceInteractive(state)).toBe(true);
    state = reduceSurfaceState(state, { type: 'close' });
    state = reduceSurfaceState(state, { type: 'closed' });
    expect(state).toEqual(createSurfaceState());
  });

  it('coalesces repeated open requests and preserves the first handoff', () => {
    const opening = reduceSurfaceState(createSurfaceState(), { type: 'open', entrySource: 'quick_note', returnContext: { draft: 'keep' } });
    const repeated = reduceSurfaceState(opening, { type: 'open', entrySource: 'chat_header' });
    expect(repeated).toBe(opening);
    expect(repeated.returnContext.draft).toBe('keep');
  });

  it('can recover from a failed opening without leaving a second layer mounted', () => {
    let state = reduceSurfaceState(createSurfaceState(), { type: 'open', entrySource: 'empty_chat' });
    state = reduceSurfaceState(state, { type: 'fail' });
    expect(state.phase).toBe(SURFACE_PHASE.FAILED);
    state = reduceSurfaceState(state, { type: 'close' });
    state = reduceSurfaceState(state, { type: 'closed' });
    expect(isSurfaceMounted(state)).toBe(false);
  });
});
