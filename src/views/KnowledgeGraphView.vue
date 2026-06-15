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
            <span class="chip-shape" :style="{ color: kgColors[t.type] || 'var(--text-muted)' }">
              {{ getTypeShapeIcon(t.type) }}
            </span>
            {{ t.type }} ({{ t.count }})
          </button>
        </div>
        <button class="kg-add-btn" @click="openFolderPicker" :title="$t('kg.add_folder') || '添加知识库'">
          <Plus :size="16" />
        </button>
      </div>
    </header>

    <!-- 主体：图谱画布 + Inspector -->
    <div class="kg-body">
      <!-- vis.js 画布 + 拖拽覆盖层 -->
      <div
        ref="networkContainer"
        class="kg-canvas"
        @dragover.prevent="onDragOver"
        @dragleave="onDragLeave"
        @drop.prevent="onDrop"
        :class="{ 'drag-over': isDragOver }"
      ></div>

      <!-- Inspector 面板 -->
      <div v-show="selectedNode" class="inspector-resizer" @mousedown="startResizeInspector"></div>
      <aside v-if="selectedNode" class="kg-inspector" :style="{ width: inspectorWidth + 'px' }">
        <div class="inspector-header">
          <span class="inspector-type-badge" :style="{ background: kgColors[selectedNode.type] || 'var(--text-muted)' }">
            {{ getTypeShapeIcon(selectedNode.type) }} {{ selectedNode.type }}
          </span>
          <div style="display:flex; gap: 4px;">
            <button class="btn-icon inspector-merge" :class="{ active: mergeMode }" @click="toggleMergeMode" title="关联/合并至...">
              <Link :size="14" />
            </button>
            <button class="btn-icon inspector-close" @click="selectedNode = null">
              <X :size="14" />
            </button>
          </div>
        </div>
        <h3 class="inspector-title">{{ selectedNode.label }}</h3>
        <p v-if="selectedNode.summary" class="inspector-summary">{{ selectedNode.summary }}</p>
        <p v-if="selectedNode.source" class="inspector-source">
          <FileText :size="12" />
          {{ selectedNode.source }}
        </p>

        <!-- 合并操作区 -->
        <div v-if="mergeMode" class="inspector-merge-panel">
          <div class="merge-prompt">
            <strong>关联/合并节点</strong>
            <p>将当前节点(被删除)合并至目标节点。</p>
            <p>请在下方选择，或在图谱上点击目标：</p>
          </div>
          <div class="merge-filters">
            <input type="text" v-model="mergeSearchTerm" placeholder="搜索目标节点..." class="merge-search-input" />
            <label class="merge-checkbox">
              <input type="checkbox" v-model="mergeFilterSameType" />
              仅同类型 ({{ selectedNode.type }})
            </label>
          </div>
          
          <div class="merge-list">
            <div 
              v-for="node in filteredMergeNodes" 
              :key="node.id"
              class="merge-list-item"
              :class="{ selected: mergeTargetId === node.id }"
              @click="mergeTargetId = node.id"
            >
              <span class="merge-item-icon" :style="{ color: kgColors[node.type] || 'var(--text-muted)' }">
                {{ getTypeShapeIcon(node.type) }}
              </span>
              <span class="merge-item-label">{{ node.label }}</span>
            </div>
            <div v-if="filteredMergeNodes.length === 0" class="merge-list-empty">无匹配节点</div>
          </div>
          <div class="merge-actions">
            <button class="btn-primary" :disabled="!mergeTargetId" @click="confirmMerge">确认合并</button>
            <button class="btn-secondary" @click="mergeMode = false">取消</button>
          </div>
        </div>

        <div class="inspector-section">
          <h4>关联节点</h4>
          <div class="relation-item" v-for="rel in selectedRelations" :key="rel.id" @click="focusNode(rel.id)">
            <div class="relation-icon" :style="{ color: kgColors[rel.type] || 'var(--text-muted)' }">
              {{ getTypeShapeIcon(rel.type) }}
            </div>
            <div class="relation-info">
              <span class="relation-label">{{ rel.label }}</span>
              <span class="relation-type">{{ rel.relation }}</span>
            </div>
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
import { Waypoints, Search, X, FileText, RefreshCw, Plus, Link } from 'lucide-vue-next';

// ── 状态 ────────────────────────────────────────────────
const networkContainer = ref(null);
const stats = ref(null);
const searchTerm = ref('');
const selectedNode = ref(null);
const selectedRelations = ref([]);
const loading = ref(true);
const activeTypes = ref(new Set());
const backfilling = ref(false);
const isDragOver = ref(false);

const mergeMode = ref(false);
const mergeTargetId = ref('');
const mergeSearchTerm = ref('');
const mergeFilterSameType = ref(true);

const inspectorWidth = ref(280);
const isResizingInspector = ref(false);

let network = null;
let nodesDataSet = null;
let edgesDataSet = null;
let allGraphData = null;

const allNodesList = computed(() => {
  if (!allGraphData) return [];
  return allGraphData.nodes
    .filter(n => n.id !== selectedNode.value?.id)
    .sort((a,b) => a.label.localeCompare(b.label));
});

const filteredMergeNodes = computed(() => {
  let list = allNodesList.value;
  if (mergeFilterSameType.value && selectedNode.value) {
    list = list.filter(n => n.type === selectedNode.value.type);
  }
  if (mergeSearchTerm.value) {
    const term = mergeSearchTerm.value.toLowerCase();
    list = list.filter(n => n.label.toLowerCase().includes(term));
  }
  return list;
});

function toggleMergeMode() {
  mergeMode.value = !mergeMode.value;
  mergeTargetId.value = '';
  mergeSearchTerm.value = '';
  mergeFilterSameType.value = true;
}

// ── Inspector Resizing ──
function startResizeInspector(e) {
  isResizingInspector.value = true;
  document.addEventListener('mousemove', handleResizeInspector);
  document.addEventListener('mouseup', stopResizeInspector);
  document.body.style.cursor = 'col-resize';
  document.body.style.userSelect = 'none';
}

function handleResizeInspector(e) {
  if (!isResizingInspector.value) return;
  // Panel is on the right, so width is (screen width - mouse X)
  let newWidth = window.innerWidth - e.clientX;
  if (newWidth < 280) newWidth = 280;
  if (newWidth > 800) newWidth = 800;
  inspectorWidth.value = newWidth;
}

function stopResizeInspector() {
  isResizingInspector.value = false;
  document.removeEventListener('mousemove', handleResizeInspector);
  document.removeEventListener('mouseup', stopResizeInspector);
  document.body.style.cursor = '';
  document.body.style.userSelect = '';
}

async function confirmMerge() {
  if (!mergeTargetId.value || !selectedNode.value) return;
  const targetNode = allNodesList.value.find(n => n.id === mergeTargetId.value);
  if (!targetNode) return;
  
  const yes = window.confirm(`确定要将【${selectedNode.value.label}】合并至【${targetNode.label}】吗？\n\n合并后，当前节点将被删除，其所有关联关系将转移到目标节点上。`);
  if (!yes) return;
  
  try {
    const res = await window.electronAPI.invoke('kg_merge_nodes', {
      payload: {
        primary_id: targetNode.id,
        alias_id: selectedNode.value.id
      }
    });
    if (!res.ok) throw new Error(res.error || 'Unknown error');
    
    mergeMode.value = false;
    selectedNode.value = null;
    await loadGraph();
  } catch (e) {
    alert("合并失败: " + e);
  }
}

// ── 颜色与形状定义 ─────────────────────────────────────
const kgColors = ref({});

function updateKgColors() {
  const s = getComputedStyle(document.documentElement);
  const get = (v) => s.getPropertyValue(v).trim();
  kgColors.value = {
    tag: get('--kg-node-tag') || '#0891b2',
    project: get('--kg-node-project') || '#16a34a',
    file: get('--kg-node-file') || '#64748b',
    concept: get('--kg-node-concept') || '#4f46e5',
    person: get('--kg-node-person') || '#d97706',
    topic: get('--kg-node-topic') || '#9333ea',
    edge: get('--kg-edge') || 'rgba(100, 116, 139, 0.25)',
    edgeHl: get('--kg-edge-highlight') || get('--accent-primary') || '#6366f1',
    font: get('--kg-font') || '#64748b',
  };
}

const typeFilters = computed(() => {
  if (!stats.value?.type_distribution) return [];
  return stats.value.type_distribution;
});

const typeShapes = {
  concept: { vis: 'dot', icon: '●' },
  project: { vis: 'star', icon: '★' },
  file: { vis: 'square', icon: '■' },
  tag: { vis: 'diamond', icon: '◆' },
  person: { vis: 'triangleDown', icon: '▼' },
  topic: { vis: 'hexagon', icon: '⬢' },
};

function getTypeShapeIcon(type) {
  return typeShapes[type]?.icon || '●';
}

function getTypeVisShape(type) {
  return typeShapes[type]?.vis || 'dot';
}

function buildNetworkOptions() {
  const colors = kgColors.value;
  return {
    nodes: {
      size: 14,
      font: { size: 11, color: colors.font, face: 'Inter, system-ui, sans-serif' },
      borderWidth: 1.5,
      shadow: false,
    },
    edges: {
      width: 0.6,
      color: { color: colors.edge, highlight: colors.edgeHl, hover: colors.edgeHl, opacity: 1.0 },
      font: { size: 0, color: 'transparent' },
      arrows: { to: { enabled: true, scaleFactor: 0.35 } },
      smooth: { enabled: true, type: 'continuous', roundness: 0.5 },
      hoverWidth: 0.3,
    },
    physics: {
      solver: 'barnesHut',
      barnesHut: { 
        gravitationalConstant: -12000, // 更强的节点斥力 (默认是 -2000)
        centralGravity: 0.05,          // 较弱的中心引力
        springLength: 250,             // 更长的边距 (默认 100)
        springConstant: 0.04,
        damping: 0.2 
      },
      stabilization: { enabled: true, iterations: 80, fit: true },
      maxVelocity: 50,
      minVelocity: 0.75,
      timestep: 0.5,
    },
    interaction: {
      hover: true,
      tooltipDelay: 300,
      zoomView: true,
      dragView: true,
      zoomSpeed: 1.0, // 加大缩放速度，去除迟滞感
      multiselect: false,
    },
  };
}

let tauriDragUnlistens = [];

// ── 初始化 ──────────────────────────────────────────────
onMounted(async () => {
  updateKgColors();
  await loadGraph();
  loading.value = false;

  // 注册 Tauri 原生拖拽监听 (因为全局已接管 native OS drop)
  if (window.electronAPI.onDragEnter) {
    window.electronAPI.onDragEnter(async () => {
      isDragOver.value = true;
    }).then(u => tauriDragUnlistens.push(u));

    window.electronAPI.onDragLeave(async () => {
      isDragOver.value = false;
    }).then(u => tauriDragUnlistens.push(u));

    window.electronAPI.onDragDrop(async (e) => {
      isDragOver.value = false;
      if (e.payload && e.payload.paths && e.payload.paths.length > 0) {
        const path = e.payload.paths[0];
        let yes = false;
        try {
          const { ask } = await import('@tauri-apps/plugin-dialog');
          
          // 先预估成本
          const estimate = await window.electronAPI.estimateKB(path);
          let msg = `是否要从该路径提取知识点并加入图谱？\n\n${path}\n\n`;
          
          if (estimate && !estimate.error) {
            msg += `【扫描结果】\n`;
            msg += `- 支持的文件: ${estimate.convertable_files} 个\n`;
            msg += `- Token 规模: 约 ${estimate.estimated_tokens}\n`;
            msg += `- 预计耗时: 约 ${estimate.estimated_minutes} 分钟\n`;
            msg += `- 预估成本: 约 ¥${(estimate.estimated_cost_cheap_rmb || 0).toFixed(4)} (基础模型)\n`;
          } else if (estimate && estimate.error) {
            msg += `【预估失败】\n${estimate.error}\n`;
          }

          yes = await ask(msg, {
            title: '提取前确认 (包含成本预估)',
            type: 'info'
          });
        } catch (err) {
          yes = window.confirm(`是否要从该路径提取知识点并加入图谱？\n\n${path}`);
        }
        
        if (yes) {
          await buildKBAndRefresh(path);
        }
      }
    }).then(u => tauriDragUnlistens.push(u));
  }
});

onBeforeUnmount(() => {
  if (network) {
    network.destroy();
    network = null;
  }
  tauriDragUnlistens.forEach(u => typeof u === 'function' && u());
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

  nodesDataSet = new DataSet(data.nodes.map(n => {
    const color = kgColors.value[n.type] || '#64748b';
    return {
      id: n.id,
      label: n.label,
      shape: getTypeVisShape(n.type),
      color: {
        background: color,
        border: color,
        highlight: { background: color, border: '#fff' },
      },
      title: `${n.label} (${n.type})${n.summary ? '\n' + n.summary : ''}`,
      opacity: 1.0,
      hidden: false,
      _raw: n,
    };
  }));

  edgesDataSet = new DataSet(data.edges.map((e, i) => ({
    id: `e${i}`,
    from: e.source,
    to: e.target,
    label: e.relation,
    hidden: false,
    _raw: e,
  })));

  network = new Network(networkContainer.value, { nodes: nodesDataSet, edges: edgesDataSet }, buildNetworkOptions());

  // 点击节点 → 显示 Inspector，并高亮邻居
  network.on('selectNode', (params) => {
    if (params.nodes.length > 0) {
      const nodeId = params.nodes[0];
      
      if (mergeMode.value) {
        if (nodeId !== selectedNode.value?.id) {
          mergeTargetId.value = nodeId;
          // 恢复之前选中的主节点视觉效果
          network.setSelection({ nodes: [selectedNode.value.id, nodeId] });
        }
        return;
      }

      const node = nodesDataSet.get(nodeId);
      if (node?._raw) {
        selectedNode.value = node._raw;
        loadRelations(nodeId);
        focusNeighbors(nodeId);
      }
    }
  });

  network.on('deselectNode', () => {
    if (mergeMode.value) {
      // 如果在合并模式下取消选择，保留主节点的选择状态
      if (selectedNode.value) {
        network.setSelection({ nodes: [selectedNode.value.id] });
      }
      return;
    }
    
    selectedNode.value = null;
    selectedRelations.value = [];
    resetFocus();
  });

  // CAD-like 缩放：保持连线视觉粗细一致
  network.on('zoom', () => {
    updateEdgeWidths();
  });

  // 初始时调整一次
  setTimeout(() => updateEdgeWidths(), 500);
}

function updateEdgeWidths() {
  if (!network || !edgesDataSet) return;
  const scale = network.getScale();
  // 当画布缩小 (scale < 1) 时，增加基础线宽；当画布放大 (scale > 1) 时，减小基础线宽
  const normalWidth = Math.max(0.6 / scale, 0.1);
  const highlightWidth = Math.max(1.5 / scale, 0.2);
  const dimWidth = Math.max(0.3 / scale, 0.05);

  // 更新全局配置
  network.setOptions({
    edges: {
      width: normalWidth,
      hoverWidth: normalWidth * 0.5,
    }
  });

  // 如果有选中的节点，或者之前有独立设置过线宽，需要同步更新
  const edgeUpdates = [];
  edgesDataSet.forEach(edge => {
    if (selectedNode.value) {
      edgeUpdates.push({
        id: edge.id,
        width: edge._isRelevant ? highlightWidth : dimWidth
      });
    } else {
      // 没选中节点时，如果曾经被高亮/淡化过，重置回基础缩放宽度
      if (edge.width !== undefined && Math.abs(edge.width - normalWidth) > 0.001) {
         edgeUpdates.push({
           id: edge.id,
           width: normalWidth
         });
      }
    }
  });
  if (edgeUpdates.length > 0) {
    edgesDataSet.update(edgeUpdates);
  }
}

function focusNeighbors(nodeId) {
  if (!network) return;
  const neighborIds = new Set(network.getConnectedNodes(nodeId));
  neighborIds.add(nodeId);

  const nodeUpdates = [];
  nodesDataSet.forEach(node => {
    const isRelevant = neighborIds.has(node.id);
    nodeUpdates.push({ id: node.id, opacity: isRelevant ? 1.0 : 0.2 });
  });
  nodesDataSet.update(nodeUpdates);

  const scale = network.getScale();
  const highlightWidth = Math.max(1.5 / scale, 0.2);
  const dimWidth = Math.max(0.3 / scale, 0.05);

  const edgeUpdates = [];
  const edgeHl = kgColors.value.edgeHl;
  const edgeBase = kgColors.value.edge;
  edgesDataSet.forEach(edge => {
    const isRelevant = edge.from === nodeId || edge.to === nodeId;
    edgeUpdates.push({
      id: edge.id,
      _isRelevant: isRelevant,
      color: { color: isRelevant ? edgeHl : edgeBase, opacity: isRelevant ? 1.0 : 0.08 },
      width: isRelevant ? highlightWidth : dimWidth,
    });
  });
  edgesDataSet.update(edgeUpdates);
}

function resetFocus() {
  const nodeUpdates = [];
  nodesDataSet.forEach(node => {
    nodeUpdates.push({ id: node.id, opacity: 1.0 });
  });
  nodesDataSet.update(nodeUpdates);

  const scale = network.getScale();
  const normalWidth = Math.max(0.6 / scale, 0.1);

  const edgeUpdates = [];
  const edgeBase = kgColors.value.edge;
  edgesDataSet.forEach(edge => {
    edgeUpdates.push({
      id: edge.id,
      _isRelevant: false,
      color: { color: edgeBase, opacity: 1.0 },
      width: normalWidth,
    });
  });
  edgesDataSet.update(edgeUpdates);
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
  if (!nodesDataSet || !edgesDataSet) return;

  const showAll = activeTypes.value.size === 0;
  
  const nodeUpdates = [];
  nodesDataSet.forEach(node => {
    const shouldHide = !showAll && !activeTypes.value.has(node._raw.type);
    if (node.hidden !== shouldHide) {
      nodeUpdates.push({ id: node.id, hidden: shouldHide });
    }
  });
  if (nodeUpdates.length > 0) nodesDataSet.update(nodeUpdates);

  const edgeUpdates = [];
  edgesDataSet.forEach(edge => {
    const srcHidden = nodesDataSet.get(edge.from)?.hidden;
    const tgtHidden = nodesDataSet.get(edge.to)?.hidden;
    const shouldHide = srcHidden || tgtHidden;
    if (edge.hidden !== shouldHide) {
      edgeUpdates.push({ id: edge.id, hidden: shouldHide });
    }
  });
  if (edgeUpdates.length > 0) edgesDataSet.update(edgeUpdates);
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

// ── 拖拽添加 + 文件夹选择 ────────────────────────────────
function onDragOver(e) {
  isDragOver.value = true;
}

function onDragLeave() {
  isDragOver.value = false;
}

async function confirmExtract(path) {
  try {
    const est = await window.electronAPI.invoke('system_estimate_kb', { folderPath: path });
    
    let msg = `是否要从该路径提取知识点并加入图谱？\n\n路径: ${path}`;
    if (est) {
      msg += `\n\n【成本估算】`;
      msg += `\n有效文件: ${est.convertable_files} 个`;
      msg += `\n有效容量: ${(est.convertable_bytes / 1024).toFixed(2)} KB`;
      msg += `\n预估消耗: ${est.estimated_tokens} Tokens`;
      msg += `\n预估费用: 约 ￥${est.estimated_cost_core_rmb.toFixed(4)}`;
    }

    const { ask } = await import('@tauri-apps/plugin-dialog');
    const yes = await ask(msg, {
      title: '提取前确认及成本预估',
      type: 'info'
    });
    
    if (yes) {
      await buildKBAndRefresh(path);
    }
  } catch (err) {
    console.error(err);
    const yes = window.confirm(`是否要从该路径提取知识点并加入图谱？\n\n${path}`);
    if (yes) {
      await buildKBAndRefresh(path);
    }
  }
}

async function onDrop(e) {
  isDragOver.value = false;
  const files = e.dataTransfer?.files;
  if (!files || files.length === 0) return;
  // 获取第一个拖入的路径
  const path = files[0].path || files[0].name;
  if (path) {
    await confirmExtract(path);
  }
}

async function openFolderPicker() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const selected = await open({ directory: true, multiple: false, title: '选择知识库文件夹' });
    if (selected) {
      await confirmExtract(selected);
    }
  } catch (e) {
    // 降级: 使用 invoke 直接调用
    console.error('Folder picker failed:', e);
  }
}

async function buildKBAndRefresh(folderPath) {
  backfilling.value = true;
  try {
    // 调用现有的 KB 构建管线
    await window.electronAPI.invoke('system_build_kb', { folderPath, plan: '' });
    // KB 构建完成后回填图谱并刷新
    await window.electronAPI.kgBackfill();
    const [graphData, statsData] = await Promise.all([
      window.electronAPI.kgGetFullGraph(),
      window.electronAPI.kgStats(),
    ]);
    stats.value = statsData;
    allGraphData = graphData;
    if (graphData.nodes?.length > 0) {
      if (network) network.destroy();
      renderNetwork(graphData);
    }
  } catch (e) {
    console.error('KB build from KG view failed:', e);
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

.chip-shape {
  font-size: 10px;
  line-height: 1;
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
  transition: outline var(--duration-fast);
}

.kg-canvas.drag-over {
  outline: 2px dashed var(--accent-primary);
  outline-offset: -12px;
  background: color-mix(in srgb, var(--bg-primary) 80%, transparent);
  backdrop-filter: blur(4px);
}

.kg-add-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-subtle);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all var(--duration-fast);
  flex-shrink: 0;
}

.kg-add-btn:hover {
  background: var(--accent-primary);
  color: #fff;
  border-color: var(--accent-primary);
}

/* ── Inspector ──────────────────────────────── */
.kg-inspector {
  border-left: 1px solid var(--border-subtle);
  padding: var(--space-4);
  overflow-y: auto;
  background: var(--bg-secondary);
  animation: slideInRight var(--duration-normal) ease;
  flex-shrink: 0;
}

.inspector-resizer {
  width: 6px;
  background: transparent;
  cursor: col-resize;
  flex-shrink: 0;
  z-index: 10;
  margin-right: -3px;
  margin-left: -3px;
  position: relative;
}

.inspector-resizer::after {
  content: '';
  position: absolute;
  top: 0;
  bottom: 0;
  right: 3px;
  width: 1px;
  background: transparent;
  transition: background var(--duration-fast);
}

.inspector-resizer:hover::after, .inspector-resizer:active::after {
  background: var(--accent-primary);
  width: 2px;
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

.inspector-merge.active {
  color: var(--user-accent);
  background: var(--bg-tertiary);
}

.inspector-merge-panel {
  background: var(--bg-tertiary);
  border-radius: var(--radius-md);
  padding: var(--space-3);
  margin-bottom: var(--space-4);
  border: 1px solid var(--border-subtle);
  animation: slideDown var(--duration-fast) ease;
}

.merge-prompt strong {
  display: block;
  font-size: var(--text-sm);
  color: var(--text-primary);
  margin-bottom: var(--space-1);
}

.merge-prompt p {
  font-size: var(--text-xs);
  color: var(--text-secondary);
  margin: 0 0 var(--space-1) 0;
  line-height: 1.4;
}

.merge-search-input {
  width: 100%;
  padding: 6px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-subtle);
  background: var(--bg-primary);
  color: var(--text-primary);
  font-size: var(--text-sm);
  outline: none;
  margin-bottom: var(--space-2);
}

.merge-checkbox {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  color: var(--text-secondary);
  margin-bottom: var(--space-3);
  cursor: pointer;
}

.merge-list {
  max-height: 150px;
  overflow-y: auto;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
  background: var(--bg-primary);
  margin-bottom: var(--space-3);
}

.merge-list-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  font-size: var(--text-sm);
  color: var(--text-primary);
  cursor: pointer;
  border-bottom: 1px solid var(--border-subtle);
}

.merge-list-item:last-child {
  border-bottom: none;
}

.merge-list-item:hover {
  background: var(--bg-tertiary);
}

.merge-list-item.selected {
  background: var(--user-accent);
  color: white;
}

.merge-item-icon {
  font-size: 10px;
}

.merge-list-empty {
  padding: 10px;
  text-align: center;
  font-size: var(--text-xs);
  color: var(--text-muted);
}

.merge-actions {
  display: flex;
  gap: var(--space-2);
}

.merge-actions button {
  flex: 1;
  padding: 4px 8px;
  border-radius: var(--radius-sm);
  font-size: var(--text-xs);
  cursor: pointer;
  border: none;
  font-weight: 500;
}

.merge-actions .btn-primary {
  background: var(--user-accent);
  color: white;
}

.merge-actions .btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.merge-actions .btn-secondary {
  background: var(--bg-primary);
  color: var(--text-secondary);
  border: 1px solid var(--border-subtle);
}

@keyframes slideDown {
  from { opacity: 0; transform: translateY(-10px); }
  to { opacity: 1; transform: translateY(0); }
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

.relation-icon {
  font-size: 10px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
}

.relation-info {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
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
