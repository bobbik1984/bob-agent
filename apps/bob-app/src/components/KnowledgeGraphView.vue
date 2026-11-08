<script setup>
import { ref, computed } from 'vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  projects: {
    type: Array,
    default: () => [],
  },
  notes: {
    type: Array,
    default: () => [],
  },
  decisions: {
    type: Array,
    default: () => [],
  },
  memories: {
    type: Array,
    default: () => [],
  },
});

const emit = defineEmits(['select-entity']);

const { t } = useI18n();

const selectedCategory = ref('all'); // 'all' | 'project' | 'note' | 'decision' | 'memory'
const searchQuery = ref('');
const selectedNodeId = ref(null);
const hoveredNodeId = ref(null);

// Colors for node categories (CSS variables compatible)
const KIND_COLORS = {
  project: { bg: '#3b82f6', border: '#1d4ed8', text: '#ffffff' },
  note: { bg: '#8b5cf6', border: '#6d28d9', text: '#ffffff' },
  decision: { bg: '#f59e0b', border: '#d97706', text: '#ffffff' },
  memory: { bg: '#06b6d4', border: '#0891b2', text: '#ffffff' },
};

const rawNodes = computed(() => {
  const list = [];

  // 1. Projects
  for (const p of props.projects) {
    if (!p.id || p.id.startsWith('assistant:') || p.source_kind === 'legacy_import') continue;
    list.push({
      id: `p-${p.id}`,
      refId: p.id,
      title: p.title || p.id,
      kind: 'project',
      desc: p.description || '',
    });
  }

  // 2. Notes
  for (const n of props.notes) {
    list.push({
      id: `n-${n.id}`,
      refId: n.id,
      title: n.title || 'Untitled Note',
      kind: 'note',
      desc: n.body ? n.body.slice(0, 100) : '',
      projectId: n.project_id || null,
      tags: n.tags || [],
    });
  }

  // 3. Decisions
  for (const d of props.decisions) {
    list.push({
      id: `d-${d.id}`,
      refId: d.id,
      title: d.body ? d.body.slice(0, 50) : 'Decision',
      kind: 'decision',
      desc: d.status ? `Status: ${d.status}` : '',
      projectId: d.projectId || d.project_id || null,
    });
  }

  // 4. Memories
  for (const m of props.memories) {
    if (m.state === 'forgotten') continue;
    list.push({
      id: `m-${m.id}`,
      refId: m.id,
      title: m.summary || (m.body ? m.body.slice(0, 45) : 'Memory'),
      kind: 'memory',
      desc: m.body || '',
      projectId: m.projectId || m.project_id || null,
    });
  }

  return list;
});

const rawEdges = computed(() => {
  const edges = [];
  const nodes = rawNodes.value;
  const projectNodes = nodes.filter(n => n.kind === 'project');

  for (const node of nodes) {
    if (node.kind === 'project') continue;
    let matched = null;

    if (node.projectId) {
      matched = projectNodes.find(p => p.refId === node.projectId);
    }
    if (!matched) {
      const lower = node.title.toLowerCase();
      matched = projectNodes.find(p => lower.includes(p.title.toLowerCase()));
    }

    if (matched) {
      edges.push({
        id: `e-${matched.id}-${node.id}`,
        source: matched.id,
        target: node.id,
        relation: node.kind === 'decision' ? 'governs' : node.kind === 'note' ? 'contains' : 'informs',
      });
    }
  }

  return edges;
});

// Layout computation: radial / cluster placement in 800x500 viewport
const layoutGraph = computed(() => {
  const width = 800;
  const height = 500;
  const centerX = width / 2;
  const centerY = height / 2;

  const allNodes = rawNodes.value;
  const edges = rawEdges.value;

  const projectNodes = allNodes.filter(n => n.kind === 'project');
  const otherNodes = allNodes.filter(n => n.kind !== 'project');

  const positions = new Map();

  // Place projects in inner ring
  const numProjects = Math.max(projectNodes.length, 1);
  const projRadius = Math.min(150, 40 + numProjects * 18);

  projectNodes.forEach((p, i) => {
    const angle = (2 * Math.PI * i) / numProjects - Math.PI / 2;
    positions.set(p.id, {
      x: centerX + projRadius * Math.cos(angle),
      y: centerY + projRadius * Math.sin(angle),
      radius: 20,
    });
  });

  // Group child nodes by project
  const childrenByProj = new Map();
  const orphanNodes = [];

  for (const node of otherNodes) {
    const edge = edges.find(e => e.target === node.id);
    if (edge && positions.has(edge.source)) {
      if (!childrenByProj.has(edge.source)) {
        childrenByProj.set(edge.source, []);
      }
      childrenByProj.get(edge.source).push(node);
    } else {
      orphanNodes.push(node);
    }
  }

  // Place children in an outer arc around their parent project
  for (const [projId, children] of childrenByProj.entries()) {
    const parentPos = positions.get(projId);
    const parentAngle = Math.atan2(parentPos.y - centerY, parentPos.x - centerX);
    const branchDist = 80;
    const spreadArc = Math.min(Math.PI * 0.8, children.length * 0.35);

    children.forEach((c, idx) => {
      const offset = children.length > 1
        ? (idx / (children.length - 1) - 0.5) * spreadArc
        : 0;
      const angle = parentAngle + offset;
      positions.set(c.id, {
        x: Math.max(30, Math.min(width - 30, parentPos.x + branchDist * Math.cos(angle))),
        y: Math.max(30, Math.min(height - 30, parentPos.y + branchDist * Math.sin(angle))),
        radius: 12,
      });
    });
  }

  // Place orphans in outer perimeter ring
  const numOrphans = Math.max(orphanNodes.length, 1);
  const orphanRadius = 210;
  orphanNodes.forEach((o, i) => {
    const angle = (2 * Math.PI * i) / numOrphans;
    positions.set(o.id, {
      x: Math.max(30, Math.min(width - 30, centerX + orphanRadius * Math.cos(angle))),
      y: Math.max(30, Math.min(height - 30, centerY + orphanRadius * Math.sin(angle))),
      radius: 12,
    });
  });

  // Map nodes with positions
  const placedNodes = allNodes.map(n => {
    const pos = positions.get(n.id) || { x: centerX, y: centerY, radius: 14 };
    return {
      ...n,
      x: pos.x,
      y: pos.y,
      radius: pos.radius,
    };
  });

  // Map edges with coordinate lines
  const placedEdges = edges.map(e => {
    const sourcePos = positions.get(e.source);
    const targetPos = positions.get(e.target);
    return {
      ...e,
      x1: sourcePos ? sourcePos.x : centerX,
      y1: sourcePos ? sourcePos.y : centerY,
      x2: targetPos ? targetPos.x : centerX,
      y2: targetPos ? targetPos.y : centerY,
    };
  });

  return { nodes: placedNodes, edges: placedEdges };
});

const filteredNodes = computed(() => {
  let list = layoutGraph.value.nodes;
  if (selectedCategory.value !== 'all') {
    list = list.filter(n => n.kind === selectedCategory.value);
  }
  if (searchQuery.value.trim()) {
    const q = searchQuery.value.trim().toLowerCase();
    list = list.filter(n => n.title.toLowerCase().includes(q) || (n.desc && n.desc.toLowerCase().includes(q)));
  }
  return list;
});

const visibleNodeIds = computed(() => new Set(filteredNodes.value.map(n => n.id)));

const filteredEdges = computed(() => {
  return layoutGraph.value.edges.filter(
    e => visibleNodeIds.value.has(e.source) && visibleNodeIds.value.has(e.target)
  );
});

const counts = computed(() => ({
  project: rawNodes.value.filter(n => n.kind === 'project').length,
  note: rawNodes.value.filter(n => n.kind === 'note').length,
  decision: rawNodes.value.filter(n => n.kind === 'decision').length,
  memory: rawNodes.value.filter(n => n.kind === 'memory').length,
  total: rawNodes.value.length,
  edges: rawEdges.value.length,
}));

const selectedNode = computed(() => {
  if (!selectedNodeId.value) return null;
  return rawNodes.value.find(n => n.id === selectedNodeId.value) || null;
});

function handleNodeClick(node) {
  selectedNodeId.value = node.id;
  emit('select-entity', node);
}
</script>

<template>
  <div class="knowledge-graph-container hub-card">
    <!-- Header / Stats Bar -->
    <div class="kg-header-bar">
      <div class="kg-title-group">
        <h3>{{ t('knowledgeGraphTitle') }}</h3>
        <span class="hub-count-pill">{{ t('knowledgeGraphNodeCount', { count: counts.total }) }}</span>
        <span class="hub-count-pill">{{ t('knowledgeGraphEdgeCount', { count: counts.edges }) }}</span>
      </div>

      <div class="kg-filter-bar">
        <button
          type="button"
          class="filter-pill tiny"
          :class="{ active: selectedCategory === 'all' }"
          @click="selectedCategory = 'all'"
        >
          {{ t('knowledgeGraphFilterAll') }} ({{ counts.total }})
        </button>
        <button
          type="button"
          class="filter-pill tiny"
          :class="{ active: selectedCategory === 'project' }"
          @click="selectedCategory = 'project'"
        >
          {{ t('knowledgeGraphFilterProjects') }} ({{ counts.project }})
        </button>
        <button
          type="button"
          class="filter-pill tiny"
          :class="{ active: selectedCategory === 'note' }"
          @click="selectedCategory = 'note'"
        >
          {{ t('knowledgeGraphFilterNotes') }} ({{ counts.note }})
        </button>
        <button
          type="button"
          class="filter-pill tiny"
          :class="{ active: selectedCategory === 'decision' }"
          @click="selectedCategory = 'decision'"
        >
          {{ t('knowledgeGraphFilterDecisions') }} ({{ counts.decision }})
        </button>
        <button
          type="button"
          class="filter-pill tiny"
          :class="{ active: selectedCategory === 'memory' }"
          @click="selectedCategory = 'memory'"
        >
          {{ t('knowledgeGraphFilterMemories') }} ({{ counts.memory }})
        </button>
      </div>
    </div>

    <!-- Empty State -->
    <div v-if="!counts.total" class="kg-empty-box">
      <p>{{ t('knowledgeGraphEmpty') }}</p>
    </div>

    <!-- Interactive SVG Graph Viewport -->
    <div v-else class="kg-canvas-wrapper">
      <svg
        viewBox="0 0 800 500"
        class="kg-svg-canvas"
        preserveAspectRatio="xMidYMid meet"
      >
        <defs>
          <filter id="node-glow" x="-20%" y="-20%" width="140%" height="140%">
            <feGaussianBlur stdDeviation="3" result="blur" />
            <feComposite in="SourceGraphic" in2="blur" operator="over" />
          </filter>
        </defs>

        <!-- Edges -->
        <g class="kg-edges-group">
          <line
            v-for="edge in filteredEdges"
            :key="edge.id"
            :x1="edge.x1"
            :y1="edge.y1"
            :x2="edge.x2"
            :y2="edge.y2"
            class="kg-edge-line"
            :class="{
              highlight: hoveredNodeId === edge.source || hoveredNodeId === edge.target || selectedNodeId === edge.source || selectedNodeId === edge.target
            }"
          />
        </g>

        <!-- Nodes -->
        <g class="kg-nodes-group">
          <g
            v-for="node in filteredNodes"
            :key="node.id"
            class="kg-node-group"
            :class="{
              active: selectedNodeId === node.id,
              hovered: hoveredNodeId === node.id,
            }"
            :transform="`translate(${node.x}, ${node.y})`"
            @mouseenter="hoveredNodeId = node.id"
            @mouseleave="hoveredNodeId = null"
            @click="handleNodeClick(node)"
          >
            <!-- Outer halo on hover/select -->
            <circle
              v-if="selectedNodeId === node.id || hoveredNodeId === node.id"
              :r="node.radius + 6"
              class="kg-node-halo"
              :style="{ stroke: KIND_COLORS[node.kind]?.bg || 'var(--accent-primary)' }"
            />

            <!-- Main Node Circle -->
            <circle
              :r="node.radius"
              class="kg-node-circle"
              :style="{
                fill: KIND_COLORS[node.kind]?.bg || 'var(--accent-primary)',
                stroke: KIND_COLORS[node.kind]?.border || 'var(--accent-primary)',
              }"
            />

            <!-- Node Label Text -->
            <text
              :y="node.radius + 12"
              class="kg-node-label"
              text-anchor="middle"
            >
              {{ node.title.length > 14 ? node.title.slice(0, 12) + '...' : node.title }}
            </text>
          </g>
        </g>
      </svg>

      <!-- Selected Node Inspection Card -->
      <div v-if="selectedNode" class="kg-inspect-card">
        <div class="kg-inspect-header">
          <div style="display: flex; align-items: center; gap: 0.4rem;">
            <span
              class="kg-kind-badge"
              :style="{ background: KIND_COLORS[selectedNode.kind]?.bg, color: '#fff' }"
            >
              {{ selectedNode.kind }}
            </span>
            <strong>{{ selectedNode.title }}</strong>
          </div>
          <button
            type="button"
            class="btn ghost tiny"
            @click="selectedNodeId = null"
          >
            ×
          </button>
        </div>
        <p v-if="selectedNode.desc" class="kg-inspect-desc">{{ selectedNode.desc }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.knowledge-graph-container {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 0.85rem;
}

.kg-header-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.kg-title-group {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.kg-title-group h3 {
  margin: 0;
  font-size: var(--text-base);
  font-weight: 600;
  color: var(--text-primary);
}

.kg-filter-bar {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  flex-wrap: wrap;
}

.kg-empty-box {
  padding: 3rem 1.5rem;
  text-align: center;
  color: var(--text-secondary);
  font-size: var(--text-sm);
}

.kg-canvas-wrapper {
  position: relative;
  width: 100%;
  border-radius: var(--radius-card);
  border: 1px solid var(--line);
  background: var(--bg-secondary);
  overflow: hidden;
}

.kg-svg-canvas {
  width: 100%;
  height: 480px;
  display: block;
}

.kg-edge-line {
  stroke: var(--line);
  stroke-width: 1.2;
  stroke-dasharray: 3 3;
  transition: stroke 0.2s ease, stroke-width 0.2s ease;
}

.kg-edge-line.highlight {
  stroke: var(--accent-primary);
  stroke-width: 2.2;
  stroke-dasharray: none;
}

.kg-node-group {
  cursor: pointer;
  transition: transform 0.15s ease;
}

.kg-node-halo {
  fill: none;
  stroke-width: 2;
  opacity: 0.6;
  animation: pulse-halo 1.8s infinite ease-in-out;
}

@keyframes pulse-halo {
  0%, 100% { transform: scale(1); opacity: 0.6; }
  50% { transform: scale(1.1); opacity: 0.25; }
}

.kg-node-circle {
  stroke-width: 2;
  transition: transform 0.15s ease, filter 0.15s ease;
}

.kg-node-group:hover .kg-node-circle {
  filter: drop-shadow(0 2px 6px rgba(0, 0, 0, 0.25));
}

.kg-node-label {
  font-size: 11px;
  font-weight: 500;
  fill: var(--text-primary);
  pointer-events: none;
  text-shadow: 0 1px 2px var(--bg-primary);
}

.kg-inspect-card {
  position: absolute;
  bottom: 0.75rem;
  left: 0.75rem;
  right: 0.75rem;
  max-width: 480px;
  background: var(--bg-primary);
  border: 1px solid var(--line);
  border-radius: var(--radius-card);
  box-shadow: var(--shadow);
  padding: 0.65rem 0.85rem;
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  backdrop-filter: blur(8px);
}

.kg-inspect-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  font-size: var(--text-sm);
}

.kg-kind-badge {
  font-size: var(--text-2xs);
  padding: 0.1rem 0.35rem;
  border-radius: var(--radius-xs);
  text-transform: capitalize;
  font-weight: 600;
}

.kg-inspect-desc {
  font-size: var(--text-xs);
  color: var(--text-secondary);
  line-height: 1.35;
  margin: 0;
}
</style>
