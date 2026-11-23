// Client-owned navigation and unsaved capture draft, never an execution store.
const KEY = 'bob.client-view.v1';
export function loadClientState(storage = globalThis.localStorage) {
  try { const v = JSON.parse(storage?.getItem(KEY) || '{}'); return v && typeof v === 'object' && !Array.isArray(v) ? v : {}; }
  catch { return {}; }
}
export function saveClientState(value, storage = globalThis.localStorage) {
  try { storage?.setItem(KEY, JSON.stringify(value)); return true; } catch { return false; }
}
export function chooseStartupConversation(state, catalog) {
  return catalog?.conversations?.some(c => c.id === state.selectedConversation && !c.archived_at)
    ? state.selectedConversation : null;
}
