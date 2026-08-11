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
