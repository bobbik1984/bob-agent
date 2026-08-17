<template>
  <div class="bob-card-inline search-card" @click="openUrl" :title="url">
    <div class="bob-card-inline__icon search-card__icon">
      <ExternalLink :size="14" />
    </div>
    <div class="bob-card-inline__info">
      <div class="bob-card-inline__title">{{ title }}</div>
      <div v-if="snippet" class="search-card__snippet">{{ snippet }}</div>
    </div>
  </div>
</template>

<script setup>
import { ExternalLink } from 'lucide-vue-next';

const props = defineProps({
  title: { type: String, required: true },
  url: { type: String, required: true },
  snippet: { type: String, default: '' },
});

function openUrl() {
  if (window.appAPI?.openExternal) {
    window.appAPI.openExternal(props.url);
  } else {
    window.open(props.url, '_blank');
  }
}
</script>

<style scoped>
.search-card {
  max-width: 480px;
}

.search-card__icon {
  margin-top: 2px;
}

.search-card__snippet {
  font-size: 11px;
  color: var(--bg-root);
  opacity: 0.7;
  line-height: 1.3;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  margin-top: 2px;
}
</style>
