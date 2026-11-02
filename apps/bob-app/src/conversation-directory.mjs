export const nodeLabel = node => node.label || node.name || node.baseUrl || node.id;
export function workspaceName(path) {
  const parts = String(path || '').split(/[\\/]+/).filter(Boolean);
  return parts.at(-1) || path;
}

const GENERIC_WORKSPACES = new Set([
  'dsh_projects',
  'projects',
  'workspace',
  'workspaces',
  'work',
  'code',
  'repos',
  'default',
  'home',
  'bobbik',
  'ubuntu',
  'root',
  'tmp',
]);

const STOP_TOKENS = new Set([
  '项目',
  '方案',
  '报告',
  '优化',
  '效果',
  '寻找',
  '记录',
  '检查',
  '测试',
  '问题',
  '分析',
  '设计',
  '实现',
  '功能',
  'the',
  'and',
  'for',
  'with',
  'from',
  'check',
  'test',
]);

export function inferSmartTopic(row) {
  const title = String(row.title || '').trim();
  const ws = workspaceName(row.workspace || '');
  const wsLower = String(ws || '').toLocaleLowerCase();
  if (/^session-[0-9a-f-]{8,}/i.test(title)) {
    return { key: 'smart:unnamed-uuid', name: '未命名节点会话 (UUID)' };
  }
  if (ws && !GENERIC_WORKSPACES.has(wsLower)) {
    const normalized = wsLower.replace(/[_-]+/g, ' ').trim();
    return { key: `smart:ws:${normalized}`, name: ws };
  }
  const lower = title.toLocaleLowerCase();
  if (/(sea2|海路纪元|地球渲染|海洋|航海)/i.test(title)) {
    return { key: 'smart:topic:sea2-hailu', name: '海路纪元 · Sea2 地球渲染' };
  }
  if (/(winter[\s_-]*shelter)/i.test(lower)) {
    return { key: 'smart:topic:winter-shelter', name: 'winter-shelter' };
  }
  if (/(blender|render|渲染|建模|3d)/i.test(title)) {
    return { key: 'smart:topic:3d-render', name: '3D 渲染与美术管线' };
  }
  if (/(dos|exe|逆向|还原源代码|反编译)/i.test(title)) {
    return { key: 'smart:topic:reverse-eng', name: '逆向工程与源码还原' };
  }
  const tokens =
    title
      .match(/[A-Za-z0-9_]{3,}|[\u4e00-\u9fa5]{2,4}/g)
      ?.map(t => t.toLocaleLowerCase())
      .filter(t => !STOP_TOKENS.has(t)) || [];
  if (tokens.length > 0) {
    return { key: `smart:kw:${tokens[0]}`, name: `专题 · ${tokens[0]}` };
  }
  return { key: 'smart:misc', name: '其他独立会话' };
}

// Display names never become identity keys: equal basenames can belong to different nodes/paths.
export function directoryGroups(rows, mode, pinnedSet = null) {
  const filtered = rows.filter(row => !row.legacy);
  const pins =
    pinnedSet instanceof Set
      ? pinnedSet
      : Array.isArray(pinnedSet)
        ? new Set(pinnedSet)
        : null;
  const ordered = pins && pins.size > 0
    ? [
        ...filtered.filter(r => pins.has(r.uid || r.id)),
        ...filtered.filter(r => !pins.has(r.uid || r.id)),
      ]
    : filtered;

  if (mode === 'smart') {
    const groups = new Map();
    for (const row of ordered) {
      const linked = row.project_owner && row.project_id;
      if (linked) {
        const key = JSON.stringify(['project', row.project_owner, row.project_id]);
        if (!groups.has(key)) {
          groups.set(key, {
            key,
            name: row.project,
            fullName: row.project,
            items: [],
          });
        }
        groups.get(key).items.push(row);
        continue;
      }
      const topic = inferSmartTopic(row);
      if (!groups.has(topic.key)) {
        groups.set(topic.key, {
          key: topic.key,
          name: topic.name,
          fullName: topic.name,
          items: [],
        });
      }
      groups.get(topic.key).items.push(row);
    }
    return [...groups.values()];
  }

  const groups = new Map();
  for (const row of ordered) {
    const device = row.remote ? nodeLabel(row.node) : row.device;
    const owner = row.remote ? ['remote', row.node.id] : ['local', row.device];
    const project = row.project || row.workspace || '';
    const linked = row.project_owner && row.project_id;
    const key = JSON.stringify(
      mode === 'device'
        ? owner
        : linked
          ? ['project', row.project_owner, row.project_id]
          : [...owner, row.project_id || row.workspace || ''],
    );
    if (!groups.has(key))
      groups.set(key, {
        key,
        name:
          mode === 'device'
            ? device
            : linked
              ? row.project
              : project
                ? `${device} / ${row.project || workspaceName(row.workspace)}`
                : '',
        fullName: mode === 'device' ? device : project ? `${device} / ${project}` : '',
        items: [],
      });
    groups.get(key).items.push(row);
  }
  return [...groups.values()];
}
