// Group only by explicitly stored identity. Labels and addresses are never identity.
export const deviceKey = node => node.deviceId || node.id;
export function deviceGroups(nodes = []) {
  const groups = new Map();
  for (const node of nodes) {
    const id = deviceKey(node);
    if (!groups.has(id)) groups.set(id, { id, label: '', paths: [] });
    groups.get(id).paths.push(node);
  }
  for (const group of groups.values()) {
    const canonical = group.paths.find(n => n.id === group.id) || group.paths[0];
    group.label = canonical.label || canonical.name || canonical.baseUrl || canonical.id;
  }
  return [...groups.values()];
}

// Keep saved device filters usable when a legacy path is assigned to a device.
export function deviceFilterKeys(keys, nodes) {
  return [...new Set(keys.map(key => {
    const node = nodes.find(n => `remote:${n.id}` === key);
    return node ? `remote:${deviceKey(node)}` : key;
  }))];
}
