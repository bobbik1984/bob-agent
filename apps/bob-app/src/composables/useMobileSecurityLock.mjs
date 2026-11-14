import { computed, ref } from 'vue';

const DEFAULT_LOCK_STATE = Object.freeze({
  supported: false,
  enrolled: false,
  decided: true,
  enabled: false,
  unlocked: true,
});

function normalizeLockState(raw) {
  if (!raw || typeof raw !== 'object') return { ...DEFAULT_LOCK_STATE };
  const supported = Boolean(raw.supported);
  const enrolled = Boolean(raw.enrolled);
  const decided = supported ? Boolean(raw.decided) : true;
  const enabled = supported ? Boolean(raw.enabled) : false;
  const unlocked = !supported || !enabled || Boolean(raw.unlocked);
  return { supported, enrolled, decided, enabled, unlocked };
}

function normalizeErrorCode(err) {
  const code = String(err?.message || err || '');
  return /^[A-Z][A-Z0-9_]{2,90}$/.test(code) ? code : 'SECURITY_LOCK_AUTH_FAILED';
}

export function useMobileSecurityLock(bridge) {
  const state = ref({ ...DEFAULT_LOCK_STATE });
  const busy = ref(false);
  const errorCode = ref('');

  const needsDecision = computed(() => state.value.supported && !state.value.decided);
  const needsUnlock = computed(
    () => state.value.supported && state.value.decided && state.value.enabled && !state.value.unlocked,
  );
  const isBlocked = computed(() => needsDecision.value || needsUnlock.value);

  async function refresh() {
    if (!bridge?.securityLockStatus) {
      state.value = { ...DEFAULT_LOCK_STATE };
      return state.value;
    }
    try {
      const res = await bridge.securityLockStatus();
      state.value = normalizeLockState(res);
      errorCode.value = '';
    } catch {
      state.value = { ...DEFAULT_LOCK_STATE };
    }
    return state.value;
  }

  async function chooseFirstLaunch(enable, opts = {}) {
    if (busy.value) return state.value;
    if (!bridge?.securityLockSet) return refresh();
    busy.value = true;
    errorCode.value = '';
    try {
      const res = await bridge.securityLockSet({
        enabled: Boolean(enable),
        skipSetup: !enable,
        title: opts.title,
        subtitle: opts.subtitle,
      });
      state.value = normalizeLockState(res);
    } catch (err) {
      errorCode.value = normalizeErrorCode(err);
    } finally {
      busy.value = false;
    }
    return state.value;
  }

  async function unlock(opts = {}) {
    if (busy.value) return state.value;
    if (!bridge?.securityLockUnlock) return refresh();
    busy.value = true;
    errorCode.value = '';
    try {
      const res = await bridge.securityLockUnlock({
        title: opts.title,
        subtitle: opts.subtitle,
      });
      state.value = normalizeLockState(res);
    } catch (err) {
      errorCode.value = normalizeErrorCode(err);
    } finally {
      busy.value = false;
    }
    return state.value;
  }

  async function toggleInSettings(enable, opts = {}) {
    if (busy.value) return state.value;
    if (!bridge?.securityLockSet) return refresh();
    busy.value = true;
    errorCode.value = '';
    try {
      const res = await bridge.securityLockSet({
        enabled: Boolean(enable),
        skipSetup: false,
        title: opts.title,
        subtitle: opts.subtitle,
      });
      state.value = normalizeLockState(res);
    } catch (err) {
      errorCode.value = normalizeErrorCode(err);
    } finally {
      busy.value = false;
    }
    return state.value;
  }

  return {
    state,
    busy,
    errorCode,
    needsDecision,
    needsUnlock,
    isBlocked,
    refresh,
    chooseFirstLaunch,
    unlock,
    toggleInSettings,
  };
}
