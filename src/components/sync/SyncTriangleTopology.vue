<template>
  <div ref="topologyRef" class="sync-topology" role="img" :aria-label="ariaLabel">
    <svg class="sync-topology__canvas" viewBox="0 0 360 260" preserveAspectRatio="none" aria-hidden="true">
      <path class="sync-edge" :class="edgeClass(leftLinkState)" :d="edgePaths.left.d" />
      <path class="sync-edge" :class="edgeClass(rightLinkState)" :d="edgePaths.right.d" />
      <path class="sync-edge sync-edge--lan" :class="edgeClass(paths.lan_direct)" :d="edgePaths.lan.d" />
      <g :class="pulseClass(leftLinkState, leftReverse)"><circle v-for="(point,index) in edgePaths.left.points" :key="index" :cx="point.x" :cy="point.y" r="2.4" /></g>
      <g :class="pulseClass(rightLinkState, rightReverse)"><circle v-for="(point,index) in edgePaths.right.points" :key="index" :cx="point.x" :cy="point.y" r="2.4" /></g>
      <g :class="pulseClass(paths.lan_direct, false)"><circle v-for="(point,index) in edgePaths.lan.points" :key="index" :cx="point.x" :cy="point.y" r="2.4" /></g>
    </svg>

    <div ref="relayRef" class="sync-node sync-node--relay" :class="nodeClass(nodes.relay)">
      <Cloud :size="25" :stroke-width="1.6" />
    </div>
    <div ref="mobileRef" class="sync-node sync-node--mobile" :class="nodeClass(nodes.mobile)">
      <Smartphone :size="25" :stroke-width="1.6" />
    </div>
    <div ref="pcRef" class="sync-node sync-node--pc" :class="nodeClass(nodes.pc)">
      <Monitor :size="25" :stroke-width="1.6" />
    </div>

  </div>
</template>

<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { Cloud, Monitor, Smartphone } from 'lucide-vue-next';

const props = defineProps({
  paths: { type: Object, required: true },
  nodes: { type: Object, default: () => ({ mobile: 'unknown', relay: 'unknown', pc: 'unknown' }) },
  labels: { type: Object, required: true },
  ariaLabel: { type: String, required: true },
});

const topologyRef = ref(null);
const relayRef = ref(null);
const mobileRef = ref(null);
const pcRef = ref(null);
const emptyEdge = () => ({ d: '', points: [] });
const edgePaths = reactive({ left: emptyEdge(), right: emptyEdge(), lan: emptyEdge() });
let resizeObserver;

const allowedStates = new Set(['pending', 'running', 'success', 'failed', 'timeout', 'unknown', 'skipped']);
const normalized = (value) => allowedStates.has(value?.status ?? value) ? (value?.status ?? value) : 'unknown';
const edgeClass = (value) => `is-${normalized(value)}`;
const nodeClass = (value) => `is-${normalized(value)}`;

const physicalLinkState = (states) => {
  const values = states.map(normalized);
  if (values.includes('running')) return 'running';
  if (values.includes('failed')) return 'failed';
  if (values.includes('timeout')) return 'timeout';
  if (values.includes('success')) return 'success';
  if (values.every((value) => value === 'skipped')) return 'skipped';
  if (values.includes('pending')) return 'pending';
  return 'unknown';
};

const leftLinkState = computed(() => physicalLinkState([
  props.paths.mobile_to_relay,
  props.paths.relay_to_mobile,
]));
const rightLinkState = computed(() => physicalLinkState([
  props.paths.relay_to_pc,
  props.paths.pc_to_relay,
]));
const leftReverse = computed(() => normalized(props.paths.relay_to_mobile) === 'running' && normalized(props.paths.mobile_to_relay) !== 'running');
const rightReverse = computed(() => normalized(props.paths.pc_to_relay) === 'running' && normalized(props.paths.relay_to_pc) !== 'running');
const pulseClass = (state, reverse) => ({ 'sync-pulses': true, 'is-active': normalized(state) === 'running', 'is-reverse': reverse });

const nodeGeometry = (element, topologyRect) => {
  const rect = element.getBoundingClientRect();
  const scaleX = 360 / topologyRect.width;
  const scaleY = 260 / topologyRect.height;
  return {
    x: (rect.left - topologyRect.left + rect.width / 2) * scaleX,
    y: (rect.top - topologyRect.top + rect.height / 2) * scaleY,
    // Links visually connect to the icon, not to the wider icon-plus-label box.
    // A fixed icon radius keeps at least four dots visible on narrow Android layouts.
    halfWidth: 15,
    halfHeight: 15,
  };
};

const clippedEdge = (from, to) => {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const length = Math.hypot(dx, dy) || 1;
  const ux = dx / length;
  const uy = dy / length;
  const boundaryDistance = (node) => Math.min(
    Math.abs(ux) > 0.0001 ? node.halfWidth / Math.abs(ux) : Infinity,
    Math.abs(uy) > 0.0001 ? node.halfHeight / Math.abs(uy) : Infinity,
  );
  const gap = 3.5;
  const startDistance = boundaryDistance(from) + gap;
  const endDistance = boundaryDistance(to) + gap;
  const start = { x: from.x + ux * startDistance, y: from.y + uy * startDistance };
  const end = { x: to.x - ux * endDistance, y: to.y - uy * endDistance };
  const points = [0.2,0.4,0.6,0.8].map((ratio) => ({ x: start.x + (end.x-start.x)*ratio, y: start.y + (end.y-start.y)*ratio }));
  return { d: `M ${start.x.toFixed(1)} ${start.y.toFixed(1)} L ${end.x.toFixed(1)} ${end.y.toFixed(1)}`, points };
};

const updateGeometry = () => {
  const topology = topologyRef.value;
  if (!topology || !relayRef.value || !mobileRef.value || !pcRef.value || !topology.clientWidth) return;
  const topologyRect = topology.getBoundingClientRect();
  const relay = nodeGeometry(relayRef.value, topologyRect);
  const mobile = nodeGeometry(mobileRef.value, topologyRect);
  const pc = nodeGeometry(pcRef.value, topologyRect);
  edgePaths.left = clippedEdge(mobile, relay);
  edgePaths.right = clippedEdge(relay, pc);
  edgePaths.lan = clippedEdge(mobile, pc);
};

onMounted(async () => {
  await nextTick();
  updateGeometry();
  resizeObserver = new ResizeObserver(updateGeometry);
  resizeObserver.observe(topologyRef.value);
});

onBeforeUnmount(() => resizeObserver?.disconnect());
</script>

<style scoped>
.sync-topology {
  position: relative;
  width: min(100%, 320px);
  aspect-ratio: 360 / 260;
  margin-inline: auto;
}

.sync-topology__canvas {
  display: block;
  width: 100%;
  height: 100%;
  overflow: visible;
}

.sync-edge {
  fill: none;
  stroke: var(--border-strong, #777);
  stroke-width: 2.2;
  stroke-linecap: round;
  stroke-dasharray: .01 5;
  vector-effect: non-scaling-stroke;
  transition: stroke .2s, opacity .2s;
}

.sync-edge.is-pending,
.sync-edge.is-unknown { stroke: var(--text-tertiary); opacity: .52; }
.sync-edge.is-running { stroke: var(--user-accent); animation: sync-march .8s linear infinite; }
.sync-edge.is-success { stroke: var(--user-accent); }
.sync-edge.is-failed { stroke: var(--text-tertiary); opacity: .72; }
.sync-edge.is-timeout { stroke: var(--text-tertiary); opacity: .72; }
.sync-edge.is-skipped { opacity: .18; }

@keyframes sync-march { to { stroke-dashoffset: -10; } }

.sync-node {
  position: absolute;
  transform: translate(-50%, -50%);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 3px;
  width: 60px;
  height: 48px;
  color: var(--text-tertiary);
  background: transparent;
  border: 0;
  z-index: 2;
  transition: color .2s, opacity .2s;
}

.sync-node--relay { left: 50%; top: 29.3%; }
.sync-node--mobile { left: 36.5%; top: 66.5%; }
.sync-node--pc { left: 63.5%; top: 66.5%; }
.sync-node.is-running,
.sync-node.is-success { color: var(--user-accent); }
.sync-node.is-failed,
.sync-node.is-timeout { color: var(--text-primary); }
.sync-node.is-skipped { opacity: .35; }

.sync-pulses { opacity: 0; pointer-events: none; }
.sync-pulses circle { fill: var(--user-accent); opacity: .18; transform-box: fill-box; transform-origin: center; }
.sync-pulses.is-active { opacity: 1; }
.sync-pulses.is-active circle { animation: sync-dot-pulse 1.15s ease-in-out infinite; }
.sync-pulses.is-active circle:nth-child(2) { animation-delay: .18s; }
.sync-pulses.is-active circle:nth-child(3) { animation-delay: .36s; }
.sync-pulses.is-active circle:nth-child(4) { animation-delay: .54s; }
.sync-pulses.is-active.is-reverse circle:nth-child(1) { animation-delay: .54s; }
.sync-pulses.is-active.is-reverse circle:nth-child(2) { animation-delay: .36s; }
.sync-pulses.is-active.is-reverse circle:nth-child(3) { animation-delay: .18s; }
.sync-pulses.is-active.is-reverse circle:nth-child(4) { animation-delay: 0s; }
@keyframes sync-dot-pulse { 0%,58%,100% { opacity:.18;transform:scale(.8) } 25% { opacity:1;transform:scale(1.45) } }

@media (max-width: 360px) {
  .sync-node { width: 56px; height: 46px; }
  .sync-node--relay { top: 31%; }
  .sync-node--mobile { left: 36.5%; top: 64%; }
  .sync-node--pc { left: 63.5%; top: 64%; }
}

@media (prefers-reduced-motion: reduce) {
  .sync-edge.is-running { animation: none; }
  .sync-pulses.is-active circle { animation: none; opacity: .8; }
}
</style>
