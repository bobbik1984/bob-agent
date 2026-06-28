<template>
  <div class="confirm-card">
    <div class="card-header">
      <div class="card-icon">
        <CheckSquare v-if="isTodo" :size="24" />
        <Calendar v-else :size="24" />
      </div>
      <div class="card-title-area">
        <div class="card-type">{{ isTodo ? $t('confirm_card.new_todo') : $t('confirm_card.new_event') }}</div>
        <div class="card-title">{{ event.title || $t('confirm_card.no_title') }}</div>
      </div>
    </div>

    <div class="card-body">
      <div class="info-row" v-if="!isTodo && event.start_time">
        <span class="info-label">{{ $t('confirm_card.time') }}</span>
        <span class="info-value">{{ formattedTime }}</span>
      </div>
      <div class="info-row" v-if="event.location">
        <span class="info-label">{{ $t('confirm_card.location') }}</span>
        <span class="info-value">{{ event.location }}</span>
      </div>
      <div class="info-row" v-if="event.description">
        <span class="info-label">{{ $t('confirm_card.note') }}</span>
        <span class="info-value">{{ event.description }}</span>
      </div>
      <div class="info-row" v-if="event.priority">
        <span class="info-label">{{ $t('confirm_card.priority') }}</span>
        <span class="info-value priority-badge" :class="event.priority">{{ priorityLabel }}</span>
      </div>
    </div>

    <div class="card-footer">
      <button class="btn btn-ghost" @click="$emit('cancel')">{{ $t('confirm_card.cancel') }}</button>
      <button class="btn btn-primary" @click="$emit('confirm', event)">{{ $t('confirm_card.confirm') }}</button>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { CheckSquare, Calendar } from 'lucide-vue-next';

const { t } = useI18n();

const props = defineProps({
  event: {
    type: Object,
    required: true
  }
});

defineEmits(['confirm', 'cancel']);

const isTodo = computed(() => props.event.type === 'todo');

const formattedTime = computed(() => {
  if (!props.event.start_time) return t('confirm_card.no_time');
  const start = new Date(props.event.start_time);
  let timeStr = start.toLocaleString('zh-CN', {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit'
  });

  if (props.event.end_time) {
    const end = new Date(props.event.end_time);
    if (start.toDateString() === end.toDateString()) {
      timeStr += ` - ${end.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`;
    } else {
      timeStr += ` ${t('confirm_card.to')} ${end.toLocaleString('zh-CN', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}`;
    }
  }
  return timeStr;
});

const priorityLabel = computed(() => {
  const map = { low: t('confirm_card.low'), medium: t('confirm_card.medium'), high: t('confirm_card.high') };
  return map[props.event.priority] || t('confirm_card.medium');
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
  color: var(--text-secondary);
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

.priority-badge.high { background: var(--color-error-bg); color: var(--color-error); }
.priority-badge.medium { background: var(--color-warning-bg); color: var(--color-warning); }
.priority-badge.low { background: var(--color-success-bg); color: var(--color-success); }

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
  background: transparent;
  color: var(--text-primary);
  border: 1px solid var(--border-default);
}

.btn-primary:hover {
  border-color: var(--text-secondary);
  background: var(--surface-hover);
}
</style>
