import { computed, reactive, ref } from 'vue';
import {
  SURFACE_PHASE,
  createSurfaceState,
  isSurfaceMounted,
  reduceSurfaceState,
} from '@/daily-brief/surface-state.js';

function currentDateContext() {
  const now = new Date();
  const localDate = [
    now.getFullYear(),
    String(now.getMonth() + 1).padStart(2, '0'),
    String(now.getDate()).padStart(2, '0'),
  ].join('-');
  return { localDate, utcOffsetMinutes: -now.getTimezoneOffset() };
}

export function useDailyBrief() {
  const surface = reactive(createSurfaceState());
  const snapshot = ref(null);
  const loading = ref(false);
  const refreshing = ref(false);
  const errorCode = ref('');
  let readyResolve = null;
  let openPromise = null;
  let loadPromise = null;

  function transition(event) {
    Object.assign(surface, reduceSurfaceState({ ...surface }, event));
  }

  async function load(forceRefresh = false) {
    if (loadPromise) return loadPromise;
    const method = forceRefresh ? 'dailyBriefRefresh' : 'dailyBriefGet';
    if (!window.appAPI?.[method]) {
      errorCode.value = 'ERR-BRIEF-UNAVAILABLE';
      return null;
    }
    loadPromise = (async () => {
      if (forceRefresh) refreshing.value = true;
      else loading.value = true;
      errorCode.value = '';
      try {
        snapshot.value = await window.appAPI[method](currentDateContext());
        return snapshot.value;
      } catch (error) {
        errorCode.value = String(error?.message || error || 'ERR-BRIEF-UNKNOWN').split(':')[0];
        return null;
      } finally {
        loading.value = false;
        refreshing.value = false;
        loadPromise = null;
      }
    })();
    return loadPromise;
  }

  function openTodayLayer(entrySource = 'chat_header', returnContext = null) {
    if (surface.phase !== SURFACE_PHASE.CLOSED) return openPromise || Promise.resolve(false);
    const focusTarget = typeof document !== 'undefined' ? document.activeElement : null;
    transition({
      type: 'open',
      entrySource,
      returnContext: { ...(returnContext || {}), focusTarget },
    });
    openPromise = new Promise((resolve) => { readyResolve = resolve; });
    void load(false);
    return openPromise;
  }

  function notifyReady() {
    if (surface.phase !== SURFACE_PHASE.OPENING) return;
    transition({ type: 'ready' });
    readyResolve?.(true);
    readyResolve = null;
  }

  function closeTodayLayer() {
    if (!isSurfaceMounted(surface) || surface.phase === SURFACE_PHASE.CLOSING) return;
    transition({ type: 'close' });
    readyResolve?.(false);
    readyResolve = null;
    const focusTarget = surface.returnContext?.focusTarget;
    window.setTimeout(() => {
      transition({ type: 'closed' });
      openPromise = null;
      focusTarget?.focus?.({ preventScroll: true });
    }, 160);
  }

  async function refresh() {
    return load(true);
  }

  async function ensureLoaded() {
    if (snapshot.value) return snapshot.value;
    return load(false);
  }

  async function markSeen() {
    const current = snapshot.value;
    if (!current?.snapshotId || !window.appAPI?.dailyBriefMarkSeen) return false;
    try {
      await window.appAPI.dailyBriefMarkSeen(current.snapshotId, current.revision);
      current.changedSinceLastSeen = [];
      return true;
    } catch (error) {
      errorCode.value = String(error?.message || error || 'ERR-BRIEF-MARK').split(':')[0];
      return false;
    }
  }

  return {
    surface,
    snapshot,
    loading,
    refreshing,
    errorCode,
    isMounted: computed(() => isSurfaceMounted(surface)),
    isVisible: computed(() => surface.phase === SURFACE_PHASE.OPENING || surface.phase === SURFACE_PHASE.VISIBLE),
    openTodayLayer,
    closeTodayLayer,
    notifyReady,
    refresh,
    ensureLoaded,
    markSeen,
  };
}
