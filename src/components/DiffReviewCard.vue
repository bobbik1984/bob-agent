<template>
  <div class="diff-review-card" :class="cardStatusClass">
    <!-- 头部信息：文件名、改动行数统计、展开/收起 -->
    <div class="diff-card-header" @click="toggleExpand">
      <div class="header-left">
        <div class="file-icon-wrap">
          <FileCode :size="16" class="file-icon" />
        </div>
        <div class="header-meta">
          <div class="file-path" :title="filePath">{{ displayPath }}</div>
          <div v-if="changeSummary" class="change-summary">{{ changeSummary }}</div>
        </div>
      </div>
      <div class="header-right">
        <div class="diff-stats">
          <span class="stat-badge add" v-if="additions > 0">+{{ additions }}</span>
          <span class="stat-badge del" v-if="deletions > 0">-{{ deletions }}</span>
        </div>
        <button class="expand-toggle-btn" :aria-label="isExpanded ? $t('chat.diff_collapse') : $t('chat.diff_expand_all')">
          <ChevronUp v-if="isExpanded" :size="16" />
          <ChevronDown v-else :size="16" />
        </button>
      </div>
    </div>

    <!-- 差异代码查看区域 -->
    <div v-show="isExpanded" class="diff-body">
      <div class="diff-scroll-container">
        <div
          v-for="(line, idx) in visibleLines"
          :key="idx"
          class="diff-line"
          :class="getLineClass(line)"
        >
          <span class="line-prefix">{{ getLinePrefix(line) }}</span>
          <span class="line-text">{{ getLineText(line) }}</span>
        </div>
      </div>

      <!-- 超过折叠行数的展开全部提示 -->
      <div v-if="hasMoreLines && !showAllLines" class="diff-more-row">
        <button class="diff-more-btn" @click.stop="showAllLines = true">
          {{ $t('chat.diff_expand_more', { count: totalDiffLines - maxPreviewLines }) || `展开剩余 ${totalDiffLines - maxPreviewLines} 行` }}
        </button>
      </div>
    </div>

    <!-- 底部操作按钮栏 -->
    <div class="diff-card-footer">
      <template v-if="currentStatus === 'pending'">
        <button
          class="diff-action-btn btn-reject"
          :disabled="isProcessing"
          @click="$emit('reject', change)"
        >
          <X :size="16" />
          <span>{{ $t('chat.diff_reject') || '拒绝/丢弃' }}</span>
        </button>
        <button
          class="diff-action-btn btn-approve"
          :disabled="isProcessing"
          @click="$emit('approve', change)"
        >
          <Check :size="16" />
          <span>{{ isProcessing ? ($t('chat.diff_applying') || '正在应用...') : ($t('chat.diff_approve') || '批准修改') }}</span>
        </button>
      </template>

      <!-- 终态展示 -->
      <div v-else-if="currentStatus === 'applied'" class="status-badge applied">
        <Check :size="16" />
        <span>{{ $t('chat.diff_applied') || '已批准并应用到电脑' }}</span>
      </div>
      <div v-else-if="currentStatus === 'rejected'" class="status-badge rejected">
        <X :size="16" />
        <span>{{ $t('chat.diff_rejected') || '已拒绝修改 (改动已丢弃)' }}</span>
      </div>
      <div v-else-if="currentStatus === 'cancelled'" class="status-badge cancelled">
        <Square :size="14" />
        <span>{{ $t('chat.diff_cancelled') || '任务已由手机端中止' }}</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { FileCode, Check, X, ChevronDown, ChevronUp, Square } from 'lucide-vue-next';

const { t } = useI18n();

const props = defineProps({
  change: {
    type: Object,
    required: true,
  },
  isProcessing: {
    type: Boolean,
    default: false,
  },
});

defineEmits(['approve', 'reject']);

const isExpanded = ref(true);
const showAllLines = ref(false);
const maxPreviewLines = 12;

const filePath = computed(() => props.change?.file_path || props.change?.filePath || 'workspace_file');
const displayPath = computed(() => {
  const p = filePath.value;
  const parts = p.replace(/\\/g, '/').split('/');
  return parts.length > 2 ? '.../' + parts.slice(-2).join('/') : p;
});

const changeSummary = computed(() => props.change?.summary || '');
const additions = computed(() => props.change?.additions ?? 0);
const deletions = computed(() => props.change?.deletions ?? 0);
const currentStatus = computed(() => props.change?.status || 'pending');

const rawDiff = computed(() => props.change?.diff || '');
const allDiffLines = computed(() => {
  const d = rawDiff.value;
  if (!d) return [];
  return d.split('\n');
});

const totalDiffLines = computed(() => allDiffLines.value.length);
const hasMoreLines = computed(() => totalDiffLines.value > maxPreviewLines);

const visibleLines = computed(() => {
  if (showAllLines.value || !hasMoreLines.value) {
    return allDiffLines.value;
  }
  return allDiffLines.value.slice(0, maxPreviewLines);
});

const cardStatusClass = computed(() => {
  return `status-${currentStatus.value}`;
});

function toggleExpand() {
  isExpanded.value = !isExpanded.value;
}

function getLineClass(line) {
  if (!line) return '';
  if (line.startsWith('+++') || line.startsWith('---')) return 'diff-file-header';
  if (line.startsWith('@@')) return 'diff-hunk-header';
  if (line.startsWith('+')) return 'diff-add';
  if (line.startsWith('-')) return 'diff-del';
  return 'diff-context';
}

function getLinePrefix(line) {
  if (!line) return ' ';
  const first = line.charAt(0);
  if (first === '+' || first === '-' || first === ' ' || first === '@') {
    return first;
  }
  return ' ';
}

function getLineText(line) {
  if (!line) return '';
  if (line.startsWith('+') || line.startsWith('-') || line.startsWith(' ')) {
    return line.slice(1);
  }
  return line;
}
</script>

<style scoped>
.diff-review-card {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg, 12px);
  margin: var(--space-2, 8px) 0;
  overflow: hidden;
  box-shadow: var(--shadow-sm, 0 1px 3px rgba(0, 0, 0, 0.05));
  font-family: inherit;
}

.diff-card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  background: var(--bg-tertiary);
  cursor: pointer;
  user-select: none;
  border-bottom: 1px solid var(--border-color);
  gap: 8px;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  flex: 1;
}

.file-icon-wrap {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-sm, 6px);
  background: var(--bg-secondary);
  color: var(--accent-primary);
  flex-shrink: 0;
}

.header-meta {
  min-width: 0;
  flex: 1;
}

.file-path {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
}

.change-summary {
  font-size: 11px;
  color: var(--text-secondary);
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.diff-stats {
  display: flex;
  gap: 4px;
  font-size: 11px;
  font-weight: 600;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
}

.stat-badge {
  padding: 2px 6px;
  border-radius: 4px;
}

.stat-badge.add {
  background: rgba(34, 197, 94, 0.15);
  color: var(--color-success, #22c55e);
}

.stat-badge.del {
  background: rgba(239, 68, 68, 0.15);
  color: var(--color-error, #ef4444);
}

.expand-toggle-btn {
  background: transparent;
  border: none;
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 4px;
  cursor: pointer;
}

.diff-body {
  border-bottom: 1px solid var(--border-color);
}

.diff-scroll-container {
  max-height: 280px;
  overflow-y: auto;
  overflow-x: auto;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 12px;
  line-height: 1.5;
  background: var(--bg-root, #121212);
  scrollbar-gutter: stable;
}

.diff-line {
  display: flex;
  padding: 1px 8px;
  white-space: pre;
}

.line-prefix {
  width: 16px;
  user-select: none;
  flex-shrink: 0;
  font-weight: 700;
}

.line-text {
  flex: 1;
}

.diff-file-header {
  color: var(--text-secondary);
  background: rgba(255, 255, 255, 0.04);
  font-weight: 600;
}

.diff-hunk-header {
  color: var(--accent-primary);
  background: rgba(59, 130, 246, 0.08);
  font-style: italic;
}

.diff-add {
  background: rgba(34, 197, 94, 0.15);
  color: var(--color-success, #22c55e);
}

.diff-del {
  background: rgba(239, 68, 68, 0.15);
  color: var(--color-error, #ef4444);
}

.diff-context {
  color: var(--text-secondary);
}

.diff-more-row {
  padding: 6px 12px;
  text-align: center;
  background: var(--bg-tertiary);
  border-top: 1px dashed var(--border-color);
}

.diff-more-btn {
  background: transparent;
  border: none;
  color: var(--accent-primary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  padding: 4px 8px;
}

.diff-card-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding: 10px 14px;
  gap: 10px;
  background: var(--bg-secondary);
}

.diff-action-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  min-height: 44px;
  padding: 0 16px;
  font-size: 13px;
  font-weight: 600;
  border-radius: var(--radius-md, 8px);
  cursor: pointer;
  transition: opacity 0.15s, transform 0.1s;
  touch-action: manipulation;
  user-select: none;
}

.diff-action-btn:active:not(:disabled) {
  transform: scale(0.98);
}

.diff-action-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.btn-reject {
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  color: var(--text-secondary);
  flex: 1;
}

.btn-approve {
  background: var(--accent-primary);
  border: 1px solid transparent;
  color: #fff;
  flex: 2;
}

.status-badge {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
  padding: 8px 12px;
  border-radius: var(--radius-md, 8px);
  width: 100%;
  justify-content: center;
}

.status-badge.applied {
  background: rgba(34, 197, 94, 0.12);
  color: var(--color-success, #22c55e);
}

.status-badge.rejected {
  background: rgba(239, 68, 68, 0.12);
  color: var(--color-error, #ef4444);
}

.status-badge.cancelled {
  background: var(--bg-tertiary);
  color: var(--text-tertiary);
}
</style>
