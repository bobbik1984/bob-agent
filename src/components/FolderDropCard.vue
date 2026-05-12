<template>
  <div class="confirm-card folder-drop-card">
    <div class="card-header">
      <div class="card-icon folder-icon">
        <FolderDown :size="24" />
      </div>
      <div class="card-title-area">
        <div class="card-type">发现文件夹</div>
        <div class="card-title">{{ folderName }}</div>
      </div>
    </div>

    <div class="card-body">
      <div class="info-row">
        <span class="info-label">路径</span>
        <span class="info-value path-value" :title="folderPath">{{ folderPath }}</span>
      </div>
      
      <div class="stats-grid">
        <div class="stat-item">
          <div class="stat-value">{{ scanResult?.fileCount || 0 }}</div>
          <div class="stat-label">个文件</div>
        </div>
        <div class="stat-item">
          <div class="stat-value">{{ scanResult?.dirCount || 0 }}</div>
          <div class="stat-label">个子目录</div>
        </div>
      </div>

      <div class="info-row types-row" v-if="topCategories.length > 0">
        <span class="info-label">主要包含</span>
        <div class="tags-list">
          <span v-for="cat in topCategories" :key="cat.name" class="type-tag">
            {{ cat.name }} ({{ cat.count }})
          </span>
        </div>
      </div>
    </div>

    <div class="card-footer">
      <button class="btn btn-ghost" @click="$emit('cancel')">取消</button>
      <button class="btn btn-primary" @click="$emit('confirm')">
        <FolderPlus :size="16" />
        收藏到知识库
      </button>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue';
import { FolderDown, FolderPlus } from 'lucide-vue-next';

const props = defineProps({
  folderPath: {
    type: String,
    required: true
  },
  folderName: {
    type: String,
    required: true
  },
  scanResult: {
    type: Object,
    required: true
  }
});

defineEmits(['confirm', 'cancel']);

const topCategories = computed(() => {
  if (!props.scanResult || !props.scanResult.stats) return [];
  const entries = Object.entries(props.scanResult.stats);
  entries.sort((a, b) => b[1] - a[1]);
  return entries.slice(0, 3).map(([name, count]) => ({ name, count }));
});
</script>

<style scoped>
.confirm-card {
  background-color: var(--bg-primary);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  overflow: hidden;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.2);
  width: 100%;
  max-width: 400px;
  margin: 10px 0;
}

.card-header {
  display: flex;
  align-items: center;
  padding: 16px;
  border-bottom: 1px solid var(--border-color);
  background-color: var(--bg-secondary);
}

.card-icon.folder-icon {
  background-color: rgba(var(--accent-primary-rgb), 0.15);
  color: var(--accent-primary);
  width: 48px;
  height: 48px;
  border-radius: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-right: 16px;
}

.card-title-area {
  flex: 1;
  min-width: 0;
}

.card-type {
  font-size: 12px;
  color: var(--text-secondary);
  margin-bottom: 4px;
}

.card-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.card-body {
  padding: 16px;
}

.info-row {
  display: flex;
  margin-bottom: 12px;
  font-size: 13px;
}

.info-row:last-child {
  margin-bottom: 0;
}

.info-label {
  color: var(--text-secondary);
  width: 64px;
  flex-shrink: 0;
}

.info-value {
  color: var(--text-primary);
  flex: 1;
}

.path-value {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  direction: rtl;
  text-align: left;
}

.stats-grid {
  display: flex;
  background-color: var(--bg-secondary);
  border-radius: 8px;
  padding: 12px;
  margin: 16px 0;
  border: 1px solid var(--border-color);
}

.stat-item {
  flex: 1;
  text-align: center;
}

.stat-item:first-child {
  border-right: 1px solid var(--border-color);
}

.stat-value {
  font-size: 20px;
  font-weight: 600;
  color: var(--text-primary);
  line-height: 1;
  margin-bottom: 4px;
}

.stat-label {
  font-size: 11px;
  color: var(--text-secondary);
}

.tags-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  flex: 1;
}

.type-tag {
  background-color: var(--bg-secondary);
  border: 1px solid var(--border-color);
  color: var(--text-secondary);
  padding: 2px 8px;
  border-radius: 12px;
  font-size: 11px;
}

.card-footer {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  padding: 16px;
  border-top: 1px solid var(--border-color);
  background-color: var(--bg-secondary);
}

.btn {
  padding: 8px 16px;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  transition: all 0.2s;
  border: none;
}

.btn-ghost {
  background: transparent;
  color: var(--text-secondary);
}

.btn-ghost:hover {
  background-color: var(--bg-hover);
  color: var(--text-primary);
}

.btn-primary {
  background-color: var(--accent-primary);
  color: white;
}

.btn-primary:hover {
  background-color: var(--accent-hover);
}
</style>
