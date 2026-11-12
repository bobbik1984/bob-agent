// Only projects explicitly created by this client get a temporary empty entry.
export const RECENT_PROJECT_TTL = 30 * 60 * 1000;
const created = new Map();
export function markRecentProject(id, now = Date.now()) {
  if (id) created.set(id, now);
}
export function isRecentProject(id, now = Date.now()) {
  const at = created.get(id);
  if (at === undefined) return false;
  if (now < at || now - at >= RECENT_PROJECT_TTL) {
    created.delete(id);
    return false;
  }
  return true;
}
