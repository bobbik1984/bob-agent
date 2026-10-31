export const nodeLabel = node => node.label || node.name || node.baseUrl || node.id;
export function workspaceName(path) {
  const parts = String(path || '').split(/[\\/]+/).filter(Boolean);
  return parts.at(-1) || path;
}
// Display names never become identity keys: equal basenames can belong to different nodes/paths.
export function directoryGroups(rows, mode) {
  const groups = new Map();
  for (const row of rows.filter(row => !row.legacy)) {
    const device = row.remote ? nodeLabel(row.node) : row.device;
    const owner = row.remote ? ['remote', row.node.id] : ['local', row.device];
    const project = row.project || row.workspace || '';
    const key = JSON.stringify(mode === 'device' ? owner : [...owner, row.project_id || row.workspace || '']);
    if (!groups.has(key)) groups.set(key, {
      key,
      name: mode === 'device' ? device : project ? `${device} / ${row.project || workspaceName(row.workspace)}` : '',
      fullName: mode === 'device' ? device : project ? `${device} / ${project}` : '',
      items: [],
    });
    groups.get(key).items.push(row);
  }
  return [...groups.values()];
}
