import { deviceKey } from './device-paths.mjs';
export const nodeLabel = node => node.label || node.name || node.baseUrl || node.id;
export function workspaceName(path) {
  const parts = String(path || '').split(/[\\/]+/).filter(Boolean);
  return parts.at(-1) || path;
}

const GENERIC_WORKSPACES = new Set([
  'dsh_projects',
  'bob-projects',
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
  if (/(sea2|海路纪元|地球渲染|海洋|航海)/i.test(title)) {
    return { key: 'smart:topic:sea2-hailu', name: '海路纪元 · Sea2 地球渲染' };
  }
  if (/(winter[\s_-]*shelter)/i.test(title) || /(winter[\s_-]*shelter)/i.test(wsLower)) {
    return { key: 'smart:topic:winter-shelter', name: 'winter-shelter' };
  }
  if (/(庭院|花园守线|garden)/i.test(title) || /(garden)/i.test(wsLower)) {
    return { key: 'smart:topic:courtyard-garden', name: '庭院布局与塔防网页' };
  }
  if (/(空间站|月表|molence|gpcr|blender|render|渲染|建模|3d)/i.test(title)) {
    return { key: 'smart:topic:3d-render', name: '3D 渲染与美术管线' };
  }
  if (/(dos|exe|逆向|还原源代码|反编译)/i.test(title)) {
    return { key: 'smart:topic:reverse-eng', name: '逆向工程与源码还原' };
  }
  if (/(four-harness|four_harness)/i.test(title) || /(four_harness)/i.test(wsLower)) {
    return { key: 'smart:topic:four-harness', name: '多工具联合验收 (Four-Harness)' };
  }
  if (ws && !GENERIC_WORKSPACES.has(wsLower) && !/^proj_[0-9a-f]{12,}/i.test(wsLower) && !/^exec_[a-z0-9_]+$/i.test(wsLower)) {
    const normalized = wsLower.replace(/[_-]+/g, ' ').trim();
    return { key: `smart:ws:${normalized}`, name: ws };
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

export function rowNodeKey(row) {
  return row.remote ? `remote:${deviceKey(row.node || {}) || ''}` : `local:${row.device || 'local'}`;
}

export function rowNodeLabel(row) {
  return row.remote ? nodeLabel(row.node) : String(row.device || 'Local');
}

export function rowHarnessKey(row) {
  if (!row.remote) return 'local';
  const raw = row.harness || (row.source_kind === 'claude_native' ? 'claude' : 'dsh');
  return String(raw).toLocaleLowerCase();
}

export function rowHarnessLabel(row) {
  if (!row.remote) return 'Bob';
  if (row.toolLabel || row.tool_label) return String(row.toolLabel || row.tool_label);
  const h = rowHarnessKey(row);
  if (h === 'claude') return 'Claude';
  if (h === 'agy') return 'AGY';
  if (h === 'pi') return 'Pi';
  if (h === 'codex') return 'Codex';
  return 'DSH';
}

export function rowTopicInfo(row) {
  if (row.project_owner && row.project_id) {
    return {
      key: `project:${row.project_id}`,
      name: row.project || row.project_id,
      kind: 'project',
    };
  }
  const smart = inferSmartTopic(row);
  return {
    key: smart.key,
    name: smart.name,
    kind: 'smart',
  };
}

export function extractFilterOptions(rows) {
  const nonLegacy = (rows || []).filter(r => !r.legacy);
  const nodeMap = new Map();
  const harnessMap = new Map();
  const topicMap = new Map();

  for (const row of nonLegacy) {
    const nKey = rowNodeKey(row);
    const nLabel = rowNodeLabel(row);
    if (!nodeMap.has(nKey)) nodeMap.set(nKey, { key: nKey, label: nLabel, count: 0 });
    nodeMap.get(nKey).count += 1;

    const hKey = rowHarnessKey(row);
    const hLabel = rowHarnessLabel(row);
    if (!harnessMap.has(hKey)) harnessMap.set(hKey, { key: hKey, label: hLabel, count: 0 });
    harnessMap.get(hKey).count += 1;

    const tInfo = rowTopicInfo(row);
    if (!topicMap.has(tInfo.key)) {
      topicMap.set(tInfo.key, {
        key: tInfo.key,
        label: tInfo.name,
        kind: tInfo.kind,
        count: 0,
      });
    }
    topicMap.get(tInfo.key).count += 1;
  }

  return {
    nodes: [...nodeMap.values()],
    harnesses: [...harnessMap.values()],
    topics: [...topicMap.values()].sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === 'project' ? -1 : 1;
      return b.count - a.count || a.label.localeCompare(b.label);
    }),
  };
}

const rowStamp = s => {
  const raw = s.updated_at;
  const n = Number(raw);
  return Number.isFinite(n) ? (n < 1e12 ? n * 1000 : n) : Date.parse(raw) || 0;
};

export function filterAndSortRows(rows, pinnedSet = null, options = null) {
  const secondIsOptions =
    pinnedSet &&
    !(pinnedSet instanceof Set) &&
    !Array.isArray(pinnedSet) &&
    typeof pinnedSet === 'object' &&
    options === null;
  const pins = secondIsOptions
    ? new Set()
    : pinnedSet instanceof Set
      ? pinnedSet
      : Array.isArray(pinnedSet)
        ? new Set(pinnedSet)
        : new Set();
  const opts = (secondIsOptions ? pinnedSet : options) || {};
  const nodeSet =
    opts.selectedNodes instanceof Set
      ? opts.selectedNodes
      : Array.isArray(opts.selectedNodes) && opts.selectedNodes.length > 0
        ? new Set(opts.selectedNodes)
        : null;
  const harnessSet =
    opts.selectedHarnesses instanceof Set
      ? opts.selectedHarnesses
      : Array.isArray(opts.selectedHarnesses) && opts.selectedHarnesses.length > 0
        ? new Set(opts.selectedHarnesses)
        : null;
  const topicFilter = String(opts.selectedTopic || 'all');
  const sortBy = String(opts.sortBy || 'time_desc');

  const filtered = (rows || []).filter(row => {
    if (row.legacy) return false;
    if (nodeSet && !nodeSet.has(rowNodeKey(row)) && !nodeSet.has(rowNodeLabel(row))) {
      return false;
    }
    if (harnessSet && !harnessSet.has(rowHarnessKey(row)) && !harnessSet.has(rowHarnessLabel(row))) {
      return false;
    }
    if (topicFilter === 'pinned') {
      return pins.has(row.uid || row.id);
    }
    if (topicFilter === 'unlinked') {
      return !(row.project_owner && row.project_id);
    }
    if (topicFilter && topicFilter !== 'all') {
      return rowTopicInfo(row).key === topicFilter;
    }
    return true;
  });

  return [...filtered].sort((a, b) => {
    const aPin = pins.has(a.uid || a.id) ? 1 : 0;
    const bPin = pins.has(b.uid || b.id) ? 1 : 0;
    if (aPin !== bPin) return bPin - aPin;

    if (sortBy === 'time_asc') {
      return rowStamp(a) - rowStamp(b) || String(a.uid || a.id).localeCompare(String(b.uid || b.id));
    }
    if (sortBy === 'title_asc') {
      return (
        String(a.title || '').localeCompare(String(b.title || '')) ||
        rowStamp(b) - rowStamp(a)
      );
    }
    if (sortBy === 'node_asc') {
      return (
        rowNodeLabel(a).localeCompare(rowNodeLabel(b)) ||
        rowHarnessLabel(a).localeCompare(rowHarnessLabel(b)) ||
        rowStamp(b) - rowStamp(a)
      );
    }
    if (sortBy === 'topic_asc') {
      return (
        rowTopicInfo(a).name.localeCompare(rowTopicInfo(b).name) ||
        rowStamp(b) - rowStamp(a)
      );
    }
    return rowStamp(b) - rowStamp(a) || String(a.uid || a.id).localeCompare(String(b.uid || b.id));
  });
}

// Display names never become identity keys: equal basenames can belong to different nodes/paths.
export function directoryGroups(rows, mode, pinnedSet = null, options = null) {
  const ordered = filterAndSortRows(rows, pinnedSet, options);

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
    const owner = row.remote ? ['remote', deviceKey(row.node)] : ['local', row.device];
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
