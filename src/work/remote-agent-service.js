// One application service per bridge, not per component. Credentials stay in Rust.
import { reactive } from 'vue';
import { createRemoteAgentController } from './remote-agent-state.js';

export function createRemoteAgentService(api) {
  const state = reactive({ connection: null, catalog: null, history: null, live: null,
    sessionId: '', busy: false, error: '', unlocked: false, profiles: [], selectedProfileId: '',
    reconnecting: false, retries: 0, storageBusy: false, migrationRequired: false });
  let retryTimer = null, generation = 0, connecting = false, paused = false, disposed = false, healthySince = 0;
  const transient = new Set(['REMOTE_NETWORK', 'REMOTE_TIMEOUT', 'REMOTE_DISCONNECTED']);
  const safe = e => /^REMOTE_[A-Z_]+$/.test(String(e)) ? String(e) : 'REMOTE_UNKNOWN';
  const profile = () => state.profiles.find(p => p.id === state.selectedProfileId);
  const stopRetry = () => { clearTimeout(retryTimer); retryTimer = null; state.reconnecting = false; };
  function scheduleRetry() {
    if (disposed || paused || connecting || state.actionBusy || retryTimer || !profile()?.autoReconnect || !transient.has(state.error) || state.retries >= 5) return;
    state.reconnecting = true;
    retryTimer = setTimeout(() => {
      retryTimer = null; state.retries++;
      void connectProfile(state.selectedProfileId, true);
    }, Math.min(16000, 1000 * 2 ** state.retries));
  }
  const controller = createRemoteAgentController(api, patch => {
    Object.assign(state, patch);
    // A socket that opens and immediately fails must not reset the retry budget.
    if (state.error || !state.live?.ready) healthySince = 0;
    else if (!healthySince) healthySince = Date.now();
    else if (Date.now() - healthySince >= 30000) state.retries = 0;
    scheduleRetry();
  });
  async function load() {
    try { Object.assign(state, await api.remoteAgentProfiles()); return true; }
    catch (e) { state.error = safe(e); return false; }
  }
  async function connectProfile(id, retry = false) {
    if (connecting || disposed) return false;
    stopRetry(); paused = false;
    if (!retry) state.retries = 0;
    const current = ++generation; connecting = true;
    state.selectedProfileId = id;
    try {
      const ok = await controller.connect(null, () => api.remoteAgentReconnect(id));
      if (current !== generation) return false;
      await load();
      if (ok) {
        const sessionId = profile()?.sessionId;
        if (sessionId && state.catalog?.sessions.some(s => s.sessionId === sessionId)) await controller.select(sessionId);
      }
      return ok;
    } finally { connecting = false; if (current === generation) scheduleRetry(); }
  }
  async function select(id) {
    const current = generation, selectedProfileId = state.selectedProfileId;
    await controller.select(id);
    if (current === generation && state.history?.sessionId === id && selectedProfileId) {
      try { await api.remoteAgentProfileUpdate(selectedProfileId, { sessionId: id }); await load(); }
      catch (e) { state.error = safe(e); }
    }
  }
  async function resume() {
    if (disposed || paused || connecting) return;
    await load();
    if (!state.unlocked) return;
    if (['REMOTE_AUTH', 'REMOTE_PROTOCOL'].includes(state.error)) return;
    if (state.connection && !state.error && state.live?.ready !== false) { await controller.refresh(); scheduleRetry(); return; }
    const target = profile() || state.profiles.find(p => p.autoReconnect);
    // Authentication/protocol failures require an explicit user action, not a retry loop.
    if (target?.autoReconnect && !['REMOTE_AUTH', 'REMOTE_PROTOCOL'].includes(state.error)) await connectProfile(target.id);
  }
  async function disconnect() {
    generation++; paused = true; stopRetry();
    let persistenceError = '';
    if (state.selectedProfileId) {
      try { await api.remoteAgentProfileUpdate(state.selectedProfileId, { autoReconnect: false }); await load(); }
      catch (e) { persistenceError = safe(e); }
    }
    await controller.disconnect();
    if (persistenceError) state.error = persistenceError;
  }
  const service = {
    state, load, resume, select, connectProfile, refresh: () => controller.refresh(), disconnect,
    async unlock(legacyPassword) {
      if (state.storageBusy) return false;
      state.storageBusy = true;
      try {
        const result = await api.remoteAgentUnlock(legacyPassword);
        if (result?.unlocked !== true || !Array.isArray(result.profiles)) throw 'REMOTE_SYSTEM_AUTH_FAILED';
        Object.assign(state, result); state.migrationRequired = false; state.error = ''; paused = false; return true;
      }
      catch (e) { state.error = safe(e); if (state.error === 'REMOTE_VAULT_MIGRATION_REQUIRED') state.migrationRequired = true; return false; }
      finally { state.storageBusy = false; }
    },
    async save(input, name) {
      if (connecting) return false;
      stopRetry(); const current = ++generation; connecting = true; paused = false; state.retries = 0;
      try {
        const ok = await controller.connect(null, () => api.remoteAgentSave(input, name));
        if (current !== generation) return false;
        if (ok) state.selectedProfileId = state.connection.profileId;
        await load(); return ok;
      } finally { connecting = false; }
    },
    async automatic(id, value) {
      try { await api.remoteAgentProfileUpdate(id, { autoReconnect: value }); await load(); if (!value) stopRetry(); }
      catch (e) { state.error = safe(e); }
    },
    async forget(id) {
      generation++; stopRetry(); paused = true;
      try {
        await api.remoteAgentForget(id);
        if (id === state.selectedProfileId) { await controller.disconnect(); state.selectedProfileId = ''; }
        await load();
      } catch (e) { state.error = safe(e); }
    },
    async lock() {
      generation++; paused = true; stopRetry(); await controller.disconnect();
      try { await api.remoteAgentLock(); state.unlocked = false; state.profiles = []; state.selectedProfileId = ''; }
      catch (e) { state.error = safe(e); }
    },
    needsReview() { return !!profile()?.uncertainSessions?.includes(state.sessionId); },
    async reviewed() {
      try { await api.remoteAgentProfileUpdate(state.selectedProfileId, { reviewedSession: state.sessionId }); await load(); }
      catch (e) { state.error = safe(e); }
    },
    async send(text, id) {
      if (service.needsReview()) return false;
      const ok = await controller.send(text, id); await load(); return ok;
    },
    async answer(id, answer) {
      if (service.needsReview()) return false;
      const ok = await controller.answer(id, answer); await load(); return ok;
    },
    cancel: () => controller.cancel(),
    async dispose() { disposed = true; generation++; stopRetry(); await controller.dispose(); },
  };
  return service;
}

const services = new WeakMap();
export function getRemoteAgentService(api) {
  if (!services.has(api)) services.set(api, createRemoteAgentService(api));
  return services.get(api);
}
