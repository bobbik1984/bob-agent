<template>
  <div class="sync-topology" role="img" :aria-label="ariaLabel">
    <svg class="sync-topology__canvas" viewBox="0 0 360 260" preserveAspectRatio="xMidYMid meet" aria-hidden="true">
      <defs>
        <marker id="sync-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
          <path d="M 0 0 L 8 4 L 0 8 z" fill="context-stroke" />
        </marker>
      </defs>

      <path class="sync-edge sync-edge--lan" :class="edgeClass(paths.lan_direct)"
        d="M 78 218 L 282 218" marker-end="url(#sync-arrow)" />

      <path class="sync-edge" :class="edgeClass(paths.mobile_to_relay)"
        d="M 67 191 L 163 64" marker-end="url(#sync-arrow)" />
      <path class="sync-edge" :class="edgeClass(paths.relay_to_mobile)"
        d="M 151 57 L 55 184" marker-end="url(#sync-arrow)" />

      <path class="sync-edge" :class="edgeClass(paths.relay_to_pc)"
        d="M 197 64 L 293 191" marker-end="url(#sync-arrow)" />
      <path class="sync-edge" :class="edgeClass(paths.pc_to_relay)"
        d="M 305 184 L 209 57" marker-end="url(#sync-arrow)" />

      <circle v-if="hasRunningPath" class="sync-packet" r="4">
        <animateMotion dur="1.4s" repeatCount="indefinite" path="M 67 191 L 163 64" />
      </circle>
    </svg>

    <div class="sync-node sync-node--relay" :class="nodeClass(nodes.relay)">
      <Cloud :size="30" :stroke-width="1.6" />
      <span>{{ labels.relay }}</span>
    </div>
    <div class="sync-node sync-node--mobile" :class="nodeClass(nodes.mobile)">
      <Smartphone :size="30" :stroke-width="1.6" />
      <span>{{ labels.mobile }}</span>
    </div>
    <div class="sync-node sync-node--pc" :class="nodeClass(nodes.pc)">
      <Monitor :size="30" :stroke-width="1.6" />
      <span>{{ labels.pc }}</span>
    </div>

    <button class="sync-edge-label sync-edge-label--lan" type="button" @click="$emit('select-path', 'lan_direct')">
      {{ labels.lan }}
    </button>
  </div>
</template>

<script setup>
import { computed } from 'vue';
import { Cloud, Monitor, Smartphone } from 'lucide-vue-next';

const props = defineProps({
  paths: { type: Object, required: true },
  nodes: { type: Object, default: () => ({ mobile: 'unknown', relay: 'unknown', pc: 'unknown' }) },
  labels: { type: Object, required: true },
  ariaLabel: { type: String, required: true },
});

defineEmits(['select-path']);

const allowedStates = new Set(['pending', 'running', 'success', 'failed', 'timeout', 'unknown', 'skipped']);
const normalized = (value) => allowedStates.has(value?.status ?? value) ? (value?.status ?? value) : 'unknown';
const edgeClass = (value) => `is-${normalized(value)}`;
const nodeClass = (value) => `is-${normalized(value)}`;
const hasRunningPath = computed(() => Object.values(props.paths).some((path) => normalized(path) === 'running'));
</script>

<style scoped>
.sync-topology {
  position: relative;
  width: min(100%, 440px);
  aspect-ratio: 360 / 260;
  margin-inline: auto;
}
.sync-topology__canvas { display: block; width: 100%; height: 100%; overflow: visible; }
.sync-edge { fill: none; stroke: var(--border-strong, #555); stroke-width: 2.5; vector-effect: non-scaling-stroke; transition: stroke .2s, opacity .2s; }
.sync-edge.is-pending, .sync-edge.is-unknown { stroke: var(--text-tertiary); opacity: .55; }
.sync-edge.is-running { stroke: var(--user-accent); stroke-dasharray: 7 5; animation: sync-march .8s linear infinite; }
.sync-edge.is-success { stroke: var(--color-success); }
.sync-edge.is-failed { stroke: var(--color-error); stroke-dasharray: 4 4; }
.sync-edge.is-timeout { stroke: var(--color-warning, #d69e2e); stroke-dasharray: 7 5; }
.sync-edge.is-skipped { opacity: .18; }
.sync-packet { fill: var(--user-accent); filter: drop-shadow(0 0 4px var(--user-accent)); }
@keyframes sync-march { to { stroke-dashoffset: -12; } }

.sync-node {
  position: absolute; transform: translate(-50%, -50%); display: flex; flex-direction: column;
  align-items: center; justify-content: center; gap: 4px; width: 72px; min-height: 66px;
  color: var(--text-tertiary); background: var(--bg-primary); border: 1px solid var(--border-subtle);
  border-radius: var(--radius-default); z-index: 2;
}
.sync-node span { font-size: 11px; font-weight: 600; }
.sync-node--relay { left: 50%; top: 16%; }
.sync-node--mobile { left: 14%; top: 84%; }
.sync-node--pc { left: 86%; top: 84%; }
.sync-node.is-running { color: var(--user-accent); border-color: var(--user-accent); }
.sync-node.is-success { color: var(--color-success); border-color: var(--color-success); }
.sync-node.is-failed { color: var(--color-error); border-color: var(--color-error); }
.sync-node.is-timeout { color: var(--color-warning, #d69e2e); border-color: currentColor; }
.sync-node.is-skipped { opacity: .35; }

.sync-edge-label {
  position: absolute; left: 50%; bottom: 1%; transform: translateX(-50%); z-index: 3;
  border: 0; padding: 2px 7px; border-radius: var(--radius-sm); color: var(--text-secondary);
  background: var(--bg-primary); font: inherit; font-size: 10px; cursor: pointer;
}
.sync-edge-label:focus-visible { outline: 2px solid var(--user-accent); outline-offset: 2px; }
@media (max-width: 360px) { .sync-node { width: 62px; min-height: 58px; } .sync-node span { font-size: 10px; } }
@media (prefers-reduced-motion: reduce) { .sync-edge.is-running { animation: none; } .sync-packet { display: none; } }
</style>
