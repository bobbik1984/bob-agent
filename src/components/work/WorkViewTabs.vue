<template>
  <nav class="mobile-tab-grid" :aria-label="t('work.mobile_view_tabs')">
    <button v-for="tab in WORK_VIEW_ITEMS" :key="tab.id" class="mobile-tab-item" :class="{ active: modelValue === tab.id }"
      type="button" :aria-current="modelValue === tab.id ? 'page' : undefined" @click="$emit('update:modelValue', tab.id)">
      <component :is="tab.icon" :size="19" :stroke-width="1.8" class="tab-icon" />
      <span>{{ t(tab.labelKey) }}</span>
    </button>
  </nav>
</template>

<script setup>
import { useI18n } from 'vue-i18n';
import { isWorkView, WORK_VIEW_ITEMS } from '../../work/work-view-navigation.js';

defineProps({ modelValue: { type: String, required: true, validator: isWorkView } });
defineEmits(['update:modelValue']);
const { t } = useI18n();
</script>

<style scoped>
@media (min-width: 721px) {
  .mobile-tab-grid { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
  .mobile-tab-item { display: flex; align-items: center; gap: 8px; min-height: 44px; padding: 8px 12px; border: 1px solid var(--border-color); border-radius: 10px; background: var(--bg-secondary); color: var(--text-secondary); font: inherit; cursor: pointer; }
  .mobile-tab-item.active { border-color: var(--accent-primary); color: var(--text-primary); background: var(--bg-tertiary); }
}
</style>
