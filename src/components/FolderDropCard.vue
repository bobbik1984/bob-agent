<template>
  <div class="bob-card-block folder-drop-card">
    <div class="bob-card-block__header">
      <div class="bob-card-block__icon bob-card-block__icon--accent">
        <FolderDown :size="24" />
      </div>
      <div class="bob-card-block__title-area">
        <div class="bob-card-block__type">{{ $t('folder_card.folder_detected') }}</div>
        <div class="bob-card-block__title" :title="folderName">{{ folderName }}</div>
      </div>
    </div>

    <div class="bob-card-block__body">
      <div class="bob-card-block__info-row">
        <span class="bob-card-block__info-label">{{ $t('folder_card.label_path') }}</span>
        <span class="bob-card-block__info-value path-value" :title="folderPath">{{ folderPath }}</span>
      </div>
      
      <div class="stats-grid">
        <div class="stat-item">
          <div class="stat-value">{{ scanResult?.fileCount || 0 }}</div>
          <div class="stat-label">{{ $t('folder_card.files') ? $t('folder_card.files') : '文件数' }}</div>
        </div>
        <div class="stat-item">
          <div class="stat-value">{{ scanResult?.dirCount || 0 }}</div>
          <div class="stat-label">{{ $t('folder_card.subdirs') ? $t('folder_card.subdirs') : '子目录' }}</div>
        </div>
        <div class="stat-item">
          <div class="stat-value">{{ formatSize(scanResult?.totalSize || 0) }}</div>
          <div class="stat-label">总大小</div>
        </div>
        <div class="stat-item">
          <div class="stat-value">~${{ estimateCost(scanResult?.totalSize || 0) }}</div>
          <div class="stat-label">预估费用</div>
        </div>
      </div>

      <div class="bob-card-block__info-row types-row" v-if="topCategories.length > 0">
        <span class="bob-card-block__info-label">{{ $t('folder_card.mainly_contains') }}</span>
        <div class="tags-list">
          <span v-for="cat in topCategories" :key="cat.name" class="type-tag">
            {{ cat.name }} ({{ cat.count }})
          </span>
        </div>
      </div>
    </div>

    <div class="bob-card-block__footer">
      <button class="btn btn-ghost" @click="$emit('cancel')">{{ $t('folder_card.cancel') }}</button>
      <button class="btn btn-primary" @click="$emit('confirm')">
        <FolderPlus :size="16" />
        {{ $t('folder_card.save_to_kb') }}
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

function formatSize(bytes) {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function estimateCost(bytes) {
  // 经验系数：PPT、PDF 等文档经过提取纯文本后，实际文字的字节数通常只有原文件总大小的 1% ~ 5%。
  // 我们这里采用一个保守的经验基数：2% (0.02)。
  const estimatedTextBytes = bytes * 0.02;

  // 1 token ~ 4 bytes (UTF-8 中文), 大约 $1.00 / 1M tokens (参考通用廉价模型)
  const tokens = estimatedTextBytes / 4;
  const cost = (tokens / 1_000_000) * 1.0;
  
  return cost < 0.01 ? '<0.01' : cost.toFixed(2);
}
</script>

<style scoped>
.path-value {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  direction: rtl;
  text-align: left;
}

.stats-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  background-color: var(--bg-secondary);
  border-radius: 8px;
  padding: 12px;
  margin: 12px 0;
  border: 1px solid var(--border-color, var(--border-subtle));
}

.stat-item {
  text-align: center;
  padding: 4px 0;
}

.stat-item:nth-child(odd) {
  border-right: 1px solid var(--border-color, var(--border-subtle));
}

.stat-value {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  line-height: 1.2;
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
  border: 1px solid var(--border-color, var(--border-subtle));
  color: var(--text-secondary);
  padding: 2px 8px;
  border-radius: 12px;
  font-size: 11px;
}
</style>
