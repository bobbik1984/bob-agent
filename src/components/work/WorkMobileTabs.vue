<template>
  <nav class="work-mobile-tabs" :aria-label="t('work.mobile_view_tabs')">
    <button v-for="tab in tabs" :key="tab.id" class="work-mobile-tab" :class="{ active: modelValue === tab.id }"
      type="button" :aria-current="modelValue === tab.id ? 'page' : undefined" @click="$emit('update:modelValue', tab.id)">
      <component :is="tab.icon" :size="19" :stroke-width="1.8" />
      <span>{{ t(tab.labelKey) }}</span>
    </button>
  </nav>
</template>

<script setup>
import { Activity, ClipboardList, Crosshair, LayoutGrid, Scale } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';

defineProps({ modelValue: { type: String, required: true } });
defineEmits(['update:modelValue']);
const { t } = useI18n();
const tabs = [
  { id: 'overview', labelKey: 'work.mobile_overview', icon: LayoutGrid },
  { id: 'goals', labelKey: 'work.goals', icon: Crosshair },
  { id: 'tasks', labelKey: 'work.tasks', icon: ClipboardList },
  { id: 'decisions', labelKey: 'work.decisions', icon: Scale },
  { id: 'activity', labelKey: 'work.mobile_activity', icon: Activity },
];
</script>

<style scoped>
.work-mobile-tabs { position: sticky; top: 0; z-index: 12; display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); min-height: 64px; border-bottom: 1px solid var(--border-subtle); background: color-mix(in srgb, var(--bg-primary) 94%, transparent); backdrop-filter: blur(12px); }
.work-mobile-tab { position: relative; display: grid; place-items: center; align-content: center; gap: 4px; min-width: 0; min-height: 64px; border: 0; padding: 7px 2px 8px; color: var(--text-muted); background: transparent; font: inherit; font-size: 10px; cursor: pointer; }
.work-mobile-tab::after { content: ''; position: absolute; right: 26%; bottom: 0; left: 26%; height: 2px; border-radius: 999px; background: transparent; }
.work-mobile-tab.active { color: var(--user-accent, var(--accent-primary)); }
.work-mobile-tab.active::after { background: currentColor; }
.work-mobile-tab:focus-visible { outline: 2px solid var(--user-accent, var(--accent-primary)); outline-offset: -3px; }
</style>

