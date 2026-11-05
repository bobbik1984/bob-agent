import { reactive } from 'vue';
// Runtime observations, not saved configuration. No tokens or session content.
export const pathHealth = reactive({});
let sequence = 0;
export async function checkPath(node, bridge) {
  const url = node.baseUrl || node.url || '';
  const request = ++sequence;
  pathHealth[node.id] = { state: 'checking', url, request };
  try {
    const rows = await bridge.dshList(node.id);
    if (!Array.isArray(rows)) throw Error('DSH_INVALID_LIST');
    if (pathHealth[node.id]?.request === request) pathHealth[node.id] = { state: 'readable', url, checkedAt: Date.now(), count: rows.length, request };
    return rows;
  } catch (error) {
    if (pathHealth[node.id]?.request === request) pathHealth[node.id] = { state: 'failed', url, checkedAt: Date.now(), request };
    throw error;
  }
}
export function pathState(node, now = Date.now()) {
  const observation = pathHealth[node.id];
  return observation?.url === (node.baseUrl || node.url || '') &&
    (!observation.checkedAt || now - observation.checkedAt < 60_000)
    ? observation.state : 'unverified';
}
