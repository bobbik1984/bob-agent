<template>
  <div class="kg-view">
    <!-- 顶部工具栏 -->
    <header class="kg-toolbar">
      <div class="kg-toolbar-left">
        <Waypoints :size="18" />
        <h2>{{ $t('kg.title') || '知识图谱' }}</h2>
        <span v-if="stats" class="kg-stat-badge">{{ stats.node_count }} 节点 · {{ stats.edge_count }} 关系</span>
      </div>
      <div class="kg-toolbar-right">
        <div class="kg-search-box">
          <Search :size="14" />
          <input
            v-model="searchTerm"
            :placeholder="$t('kg.search_placeholder') || '搜索节点...'"
            @keyup.enter="doSearch"
          />
        </div>
        <div class="kg-type-filters">
          <button
            v-for="t in typeFilters"
            :key="t.type"
            class="kg-filter-chip"
            :class="{ active: activeTypes.has(t.type) }"
            @click="toggleType(t.type)"
          >
            <span class="chip-dot" :style="{ background: nodeColors[t.type] || 'var(--text-muted)' }"></span>
            {{ t.type }} ({{ t.count }})
          </button>
        </div>
      </div>
    </header>

    <!-- 主体：图谱画布 + Inspector -->
    <div class="kg-body">
      <!-- vis.js 画布 -->
      <div ref="networkContainer" class="kg-canvas"></div>

      <!-- Inspector 面板 -->
      <aside v-if="selectedNode" class="kg-inspector">
        <div class="inspector-header">
          <span class="inspector-type-badge" :style="{ background: nodeColors[selectedNode.type] || 'var(--text-muted)' }">
            {{ selectedNode.type }}
          </span>
          <button class="btn-icon inspector-close" @click="selectedNode = null">
            <X :size="14" />
          </button>
        </div>
        <h3 class="inspector-title">{{ selectedNode.label }}</h3>
        <p v-if="selectedNode.summary" class="inspector-summary">{{ selectedNode.summary }}</p>
        <p v-if="selectedNode.source" class="inspector-source">
          <FileText :size="12" />
          {{ selectedNode.source }}
        </p>

        <div class="inspector-section">
          <h4>关联节点</h4>
          <div
            v-for="rel in selectedRelations"
            :key="rel.id + rel.relation"
            class="relation-item"
            @click="focusNode(rel.id)"
          >
            <span class="relation-dot" :style="{ background: nodeColors[rel.type] || 'var(--text-muted)' }"></span>
            <span class="relation-label">{{ rel.label }}</span>
            <span class="relation-type">{{ rel.relation }}</span>
          </div>
          <div v-if="selectedRelations.length === 0" class="inspector-empty">
            暂无关联
          </div>
        </div>
      </aside>
    </div>

    <!-- 空状态 / 生成中 -->
    <div v-if="!loading && stats && stats.node_count === 0 && !backfilling" class="kg-empty">
      <Waypoints :size="48" style="opacity: 0.2;" />
      <p>{{ $t('kg.empty') }}</p>
      <p class="kg-empty-hint">{{ $t('kg.empty_hint') }}</p>
    </div>
    <div v-if="backfilling" class="kg-empty">
      <RefreshCw :size="32" class="spin" style="opacity: 0.4;" />
      <p>{{ $t('kg.generating') }}</p>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, onBeforeUnmount, watch, computed } from 'vue';
import { Network } from 'vis-network';
import { DataSet } from 'vis-data';
import { Waypoints, Search, X, FileText, RefreshCw } from 'lucide-vue-next';

// ── 状态 ────────────────────────────────────────────────
const networkContainer = ref(null);
const stats = ref(null);
const searchTerm = ref('');
const selectedNode = ref(null);
const selectedRelations = ref([]);
const loading = ref(true);
const activeTypes = ref(new Set());
const backfilling = ref(false);

let network = null;
let nodesDataSet = null;
let edgesDataSet = null;
let allGraphData = null;

// ── 节点类型颜色映射 ────────────────────────────────────
const nodeColors = {
  concept: '#6366f1',    // indigo
  project: '#22c55e',    // green
  person: '#f59e0b',     // amber
  topic: '#a855f7',      // purple
  file: '#64748b',       // slate
  tag: '#06b6d4',        // cyan
  device: '#ef4444',     // red
  skill: '#ec4899',      // pink
  infrastructure: '#78716c', // stone
};

const typeFilters = computed(() => {
  if (!stats.value?.type_distribution) return [];
  return stats.value.type_distribution;
});

// ── vis.js 网络配置 ──────────────────────────────────────
const networkOptions = {
  nodes: {
    shape: 'dot',
    size: 16,
    font: {
      size: 12,
      color: 'var(--text-secondary)',
      face: 'Inter, system-ui, sans-serif',
    },
    borderWidth: 2,
    shadow: { enabled: true, size: 4, x: 0, y: 2, color: 'rgba(0,0,0,0.15)' },
  },
  edges: {
    width: 1.5,
    color: { color: 'var(--border-subtle)', highlight: 'var(--accent-primary)', hover: 'var(--accent-primary)' },
    font: { size: 9, color: 'var(--text-muted)', strokeWidth: 0, align: 'middle' },
    arrows: { to: { enabled: true, scaleFactor: 0.5 } },
    smooth: { type: 'continuous' },
  },
  physics: {
    solver: 'forceAtlas2Based',
    forceAtlas2Based: { gravitationalConstant: -40, centralGravity: 0.005, springLength: 120, springConstant: 0.05 },
    stabilization: { iterations: 150 },
  },
  interaction: {
    hover: true,
    tooltipDelay: 200,
    zoomView: true,
    dragView: true,
  },
};

// ── 初始化 ──────────────────────────────────────────────
onMounted(async () => {
  await loadGraph();
  loading.value = false;
});

onBeforeUnmount(() => {
  if (network) {
    network.destroy();
    network = null;
  }
});

async function loadGraph() {
  try {
    // 加载统计和完整图谱
    const [graphData, statsData] = await Promise.all([
      window.electronAPI.kgGetFullGraph(),
      window.electronAPI.kgStats(),
    ]);
    stats.value = statsData;
    allGraphData = graphData;

    if (graphData.nodes?.length > 0) {
      renderNetwork(graphData);
    } else {
      // 图谱为空，自动从现有 wiki_fts 生成
      await doBackfill();
    }
  } catch (e) {
    console.error('KG load failed:', e);
  }
}

function renderNetwork(data) {
  if (!networkContainer.value) return;

  nodesDataSet = new DataSet(data.nodes.map(n => ({
    id: n.id,
    label: n.label,
    color: {
      background: nodeColors[n.type] || '#64748b',
      border: nodeColors[n.type] || '#64748b',
      highlight: { background: nodeColors[n.type] || '#64748b', border: '#fff' },
    },
    title: `${n.label} (${n.type})${n.summary ? '\n' + n.summary : ''}`,
    _raw: n,
  })));

  edgesDataSet = new DataSet(data.edges.map((e, i) => ({
    id: `e${i}`,
    from: e.source,
    to: e.target,
    label: e.relation,
    _raw: e,
  })));

  network = new Network(networkContainer.value, { nodes: nodesDataSet, edges: edgesDataSet }, networkOptions);

  // 点击节点 → 显示 Inspector
  network.on('selectNode', (params) => {
    if (params.nodes.length > 0) {
      const nodeId = params.nodes[0];
      const node = nodesDataSet.get(nodeId);
      if (node?._raw) {
        selectedNode.value = node._raw;
        loadRelations(nodeId);
      }
    }
  });

  network.on('deselectNode', () => {
    selectedNode.value = null;
    selectedRelations.value = [];
  });
}

function loadRelations(nodeId) {
  if (!allGraphData) return;
  const rels = [];
  for (const e of allGraphData.edges) {
    const neighborId = e.source === nodeId ? e.target : e.target === nodeId ? e.source : null;
    if (!neighborId) continue;
    const neighborNode = allGraphData.nodes.find(n => n.id === neighborId);
    if (neighborNode) {
      rels.push({
        id: neighborNode.id,
        label: neighborNode.label,
        type: neighborNode.type,
        relation: e.relation,
      });
    }
  }
  selectedRelations.value = rels;
}

// ── 交互 ────────────────────────────────────────────────
function doSearch() {
  if (!searchTerm.value.trim() || !network || !nodesDataSet) return;
  const term = searchTerm.value.toLowerCase();
  const matchIds = nodesDataSet.getIds({
    filter: (item) => item.label?.toLowerCase().includes(term),
  });
  if (matchIds.length > 0) {
    network.selectNodes(matchIds);
    network.focus(matchIds[0], { scale: 1.2, animation: true });
    // 显示第一个匹配节点的 Inspector
    const node = nodesDataSet.get(matchIds[0]);
    if (node?._raw) {
      selectedNode.value = node._raw;
      loadRelations(matchIds[0]);
    }
  }
}

function focusNode(nodeId) {
  if (!network) return;
  network.selectNodes([nodeId]);
  network.focus(nodeId, { scale: 1.2, animation: true });
  const node = nodesDataSet.get(nodeId);
  if (node?._raw) {
    selectedNode.value = node._raw;
    loadRelations(nodeId);
  }
}

function toggleType(type) {
  const s = new Set(activeTypes.value);
  if (s.has(type)) s.delete(type); else s.add(type);
  activeTypes.value = s;
  applyTypeFilter();
}

function applyTypeFilter() {
  if (!nodesDataSet || !allGraphData) return;
  if (activeTypes.value.size === 0) {
    // 显示全部
    nodesDataSet.clear();
    nodesDataSet.add(allGraphData.nodes.map(n => ({
      id: n.id, label: n.label,
      color: { background: nodeColors[n.type] || '#64748b', border: nodeColors[n.type] || '#64748b',
               highlight: { background: nodeColors[n.type] || '#64748b', border: '#fff' } },
      title: `${n.label} (${n.type})`, _raw: n,
    })));
  } else {
    const filtered = allGraphData.nodes.filter(n => activeTypes.value.has(n.type));
    const filteredIds = new Set(filtered.map(n => n.id));
    nodesDataSet.clear();
    nodesDataSet.add(filtered.map(n => ({
      id: n.id, label: n.label,
      color: { background: nodeColors[n.type] || '#64748b', border: nodeColors[n.type] || '#64748b',
               highlight: { background: nodeColors[n.type] || '#64748b', border: '#fff' } },
      title: `${n.label} (${n.type})`, _raw: n,
    })));
    // 只保留两端都在过滤结果中的边
    edgesDataSet.clear();
    edgesDataSet.add(allGraphData.edges
      .filter(e => filteredIds.has(e.source) && filteredIds.has(e.target))
      .map((e, i) => ({ id: `e${i}`, from: e.source, to: e.target, label: e.relation, _raw: e }))
    );
  }
}

async function doBackfill() {
  backfilling.value = true;
  try {
    const result = await window.electronAPI.kgBackfill();
    console.log('KG backfill result:', result);
    // 直接获取并渲染，不调 loadGraph (避免递归)
    const [graphData, statsData] = await Promise.all([
      window.electronAPI.kgGetFullGraph(),
      window.electronAPI.kgStats(),
    ]);
    stats.value = statsData;
    allGraphData = graphData;
    if (graphData.nodes?.length > 0) {
      renderNetwork(graphData);
    }
  } catch (e) {
    console.error('KG backfill failed:', e);
  } finally {
    backfilling.value = false;
  }
}
</script>

<style scoped>
.kg-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-primary);
}

/* ── 工具栏 ──────────────────────────────────── */
.kg-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--border-subtle);
  gap: var(--space-3);
  flex-wrap: wrap;
}

.kg-toolbar-left {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.kg-toolbar-left h2 {
  font-size: var(--text-base);
  font-weight: 600;
  color: var(--text-primary);
  margin: 0;
}

.kg-stat-badge {
  font-size: var(--text-xs);
  color: var(--text-muted);
  background: var(--bg-tertiary);
  padding: 2px 8px;
  border-radius: 99px;
}

.kg-toolbar-right {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.kg-search-box {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  background: var(--bg-secondary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  padding: var(--space-1) var(--space-3);
}

.kg-search-box input {
  border: none;
  background: transparent;
  color: var(--text-primary);
  font-size: var(--text-sm);
  outline: none;
  width: 160px;
}

.kg-type-filters {
  display: flex;
  gap: var(--space-1);
  flex-wrap: wrap;
}

.kg-filter-chip {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 2px 10px;
  border-radius: 99px;
  border: 1px solid var(--border-subtle);
  background: transparent;
  color: var(--text-secondary);
  font-size: var(--text-xs);
  cursor: pointer;
  transition: all var(--duration-fast);
}

.kg-filter-chip.active {
  background: var(--bg-tertiary);
  border-color: var(--accent-primary);
  color: var(--text-primary);
}

.chip-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
}

/* ── 主体 ──────────────────────────────────── */
.kg-body {
  flex: 1;
  display: flex;
  overflow: hidden;
  position: relative;
}

.kg-canvas {
  flex: 1;
  min-height: 400px;
}

/* ── Inspector ──────────────────────────────── */
.kg-inspector {
  width: 280px;
  border-left: 1px solid var(--border-subtle);
  padding: var(--space-4);
  overflow-y: auto;
  background: var(--bg-secondary);
  animation: slideInRight var(--duration-normal) ease;
}

@keyframes slideInRight {
  from { transform: translateX(20px); opacity: 0; }
  to { transform: translateX(0); opacity: 1; }
}

.inspector-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: var(--space-2);
}

.inspector-type-badge {
  font-size: var(--text-xs);
  color: #fff;
  padding: 2px 10px;
  border-radius: 99px;
  font-weight: 500;
}

.inspector-close {
  color: var(--text-muted);
}

.inspector-title {
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 var(--space-2);
}

.inspector-summary {
  font-size: var(--text-sm);
  color: var(--text-secondary);
  line-height: 1.6;
  margin-bottom: var(--space-3);
}

.inspector-source {
  font-size: var(--text-xs);
  color: var(--text-muted);
  display: flex;
  align-items: center;
  gap: 4px;
  margin-bottom: var(--space-4);
  word-break: break-all;
}

.inspector-section h4 {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--text-primary);
  margin: 0 0 var(--space-2);
}

.relation-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background var(--duration-fast);
}

.relation-item:hover {
  background: var(--bg-tertiary);
}

.relation-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

.relation-label {
  font-size: var(--text-sm);
  color: var(--text-primary);
  flex: 1;
}

.relation-type {
  font-size: var(--text-xs);
  color: var(--text-muted);
  background: var(--bg-tertiary);
  padding: 1px 6px;
  border-radius: 4px;
}

.inspector-empty {
  font-size: var(--text-sm);
  color: var(--text-muted);
  text-align: center;
  padding: var(--space-4);
}

/* ── 空状态 ──────────────────────────────────── */
.kg-empty {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  text-align: center;
  color: var(--text-muted);
}

.kg-empty p {
  margin: var(--space-2) 0;
  font-size: var(--text-base);
}

.kg-empty-hint {
  font-size: var(--text-sm) !important;
  opacity: 0.6;
}

.kg-backfill-btn {
  margin-top: var(--space-4);
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);
  border: 1px solid var(--accent-primary);
  background: transparent;
  color: var(--accent-primary);
  font-size: var(--text-sm);
  cursor: pointer;
  transition: all var(--duration-fast);
}

.kg-backfill-btn:hover:not(:disabled) {
  background: var(--accent-primary);
  color: #fff;
}

.kg-backfill-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
</style>
