<template>
  <div class="confirm-card">
    <div class="card-header">
      <div class="card-icon">{{ isTodo ? '☑️' : '📅' }}</div>
      <div class="card-title-area">
        <div class="card-type">{{ isTodo ? '新增待办' : '新增日程' }}</div>
        <div class="card-title">{{ event.title || '无标题' }}</div>
      </div>
    </div>

    <div class="card-body">
      <div class="info-row" v-if="!isTodo && event.start_time">
        <span class="info-label">时间：</span>
        <span class="info-value">{{ formattedTime }}</span>
      </div>
      <div class="info-row" v-if="event.location">
        <span class="info-label">地点：</span>
        <span class="info-value">{{ event.location }}</span>
      </div>
      <div class="info-row" v-if="event.description">
        <span class="info-label">备注：</span>
        <span class="info-value">{{ event.description }}</span>
      </div>
      <div class="info-row" v-if="event.priority">
        <span class="info-label">优先级：</span>
        <span class="info-value priority-badge" :class="event.priority">{{ priorityLabel }}</span>
      </div>
    </div>

    <div class="card-footer">
      <button class="btn btn-ghost" @click="$emit('cancel')">取消</button>
      <button class="btn btn-primary" @click="$emit('confirm', event)">确认保存</button>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue';

const props = defineProps({
  event: {
    type: Object,
    required: true
  }
});

defineEmits(['confirm', 'cancel']);

const isTodo = computed(() => props.event.type === 'todo');

const formattedTime = computed(() => {
  if (!props.event.start_time) return '未指定时间';
  const start = new Date(props.event.start_time);
  let timeStr = start.toLocaleString('zh-CN', {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit'
  });

  if (props.event.end_time) {
    const end = new Date(props.event.end_time);
    if (start.toDateString() === end.toDateString()) {
      timeStr += ` - ${end.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`;
    } else {
      timeStr += ` 至 ${end.toLocaleString('zh-CN', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}`;
    }
  }
  return timeStr;
});

const priorityLabel = computed(() => {
  const map = { low: '低', medium: '中', high: '高' };
  return map[props.event.priority] || '中';
});
</script>

<style scoped>
.confirm-card {
  background: var(--surface-card);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  padding: var(--space-4);
  max-width: 400px;
  width: 100%;
  margin: var(--space-2) 0;
  box-shadow: var(--shadow-sm);
}

.card-header {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-bottom: var(--space-3);
  padding-bottom: var(--space-3);
  border-bottom: 1px solid var(--border-subtle);
}

.card-icon {
  font-size: 1.5rem;
}

.card-title-area {
  display: flex;
  flex-direction: column;
}

.card-type {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.card-title {
  font-size: var(--text-base);
  font-weight: 600;
  color: var(--text-primary);
}

.card-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin-bottom: var(--space-4);
}

.info-row {
  display: flex;
  font-size: var(--text-sm);
}

.info-label {
  color: var(--text-tertiary);
  width: 60px;
  flex-shrink: 0;
}

.info-value {
  color: var(--text-secondary);
  flex: 1;
}

.priority-badge {
  display: inline-block;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 0.85em;
  font-weight: 500;
}

.priority-badge.high { background: rgba(239, 68, 68, 0.1); color: #ef4444; }
.priority-badge.medium { background: rgba(245, 158, 11, 0.1); color: #f59e0b; }
.priority-badge.low { background: rgba(16, 185, 129, 0.1); color: #10b981; }

.card-footer {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
}

.btn {
  padding: var(--space-2) var(--space-4);
  font-size: var(--text-sm);
  border-radius: var(--radius-md);
  cursor: pointer;
  border: 1px solid transparent;
}

.btn-ghost {
  background: transparent;
  color: var(--text-secondary);
}

.btn-ghost:hover {
  background: var(--surface-hover);
}

.btn-primary {
  background: var(--accent-primary);
  color: white;
}

.btn-primary:hover {
  background: var(--accent-hover);
}
</style>
