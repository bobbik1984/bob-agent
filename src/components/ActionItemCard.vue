<template>
  <div class="action-item-card">
    <div class="ai-card-header">
      <div class="ai-card-icon">
        <CheckSquare v-if="item.type === 'todo'" :size="18" />
        <Calendar v-else :size="18" />
      </div>
      <div class="ai-card-title-area">
        <div class="ai-card-type">{{ item.type === 'todo' ? '检测到待办' : '检测到日程' }}</div>
        <div class="ai-card-title">{{ item.title }}</div>
      </div>
    </div>

    <div class="ai-card-body" v-if="item.date">
      <div class="ai-info-row">
        <span class="ai-info-label">日期</span>
        <span class="ai-info-value">{{ item.date }}</span>
      </div>
    </div>

    <div class="ai-card-footer">
      <button class="ai-btn ai-btn-ghost" @click="$emit('dismiss')">忽略</button>
      <button class="ai-btn ai-btn-primary" @click="$emit('save', item)">保存</button>
    </div>
  </div>
</template>

<script setup>
import { CheckSquare, Calendar } from 'lucide-vue-next';

defineProps({
  item: {
    type: Object,
    required: true
  }
});

defineEmits(['save', 'dismiss']);
</script>

<style scoped>
.action-item-card {
  background: var(--surface-card);
  border: 1px solid var(--border-subtle);
  border-left: 3px solid var(--accent-primary);
  border-radius: var(--radius-lg);
  padding: var(--space-3) var(--space-4);
  max-width: 380px;
  width: 100%;
  margin: var(--space-2) 0;
  box-shadow: var(--shadow-sm);
}

.ai-card-header {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-bottom: var(--space-2);
}

.ai-card-icon {
  color: var(--accent-primary);
  flex-shrink: 0;
}

.ai-card-title-area {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.ai-card-type {
  font-size: 10px;
  color: var(--accent-primary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  font-weight: 600;
}

.ai-card-title {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ai-card-body {
  padding: var(--space-1) 0;
  margin-bottom: var(--space-2);
}

.ai-info-row {
  display: flex;
  font-size: var(--text-xs);
  gap: var(--space-2);
}

.ai-info-label {
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.ai-info-value {
  color: var(--text-secondary);
}

.ai-card-footer {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  padding-top: var(--space-2);
  border-top: 1px solid var(--border-subtle);
}

.ai-btn {
  padding: 4px 12px;
  font-size: var(--text-xs);
  border-radius: var(--radius-md);
  cursor: pointer;
  border: 1px solid transparent;
  transition: all 0.15s;
}

.ai-btn-ghost {
  background: transparent;
  color: var(--text-tertiary);
}

.ai-btn-ghost:hover {
  background: var(--surface-hover);
  color: var(--text-secondary);
}

.ai-btn-primary {
  background: transparent;
  color: var(--text-primary);
  border: 1px solid var(--border-default);
}

.ai-btn-primary:hover {
  border-color: var(--accent-primary);
  background: color-mix(in srgb, var(--accent-primary) 6%, transparent);
}
</style>
