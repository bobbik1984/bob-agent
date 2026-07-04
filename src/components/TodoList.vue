<template>
  <div class="todo-list">
    <div class="todo-header" v-if="todos.some(t => t.status === 'done')">
      <label class="toggle-completed">
        <input type="checkbox" v-model="showCompleted" />
        <span class="toggle-text">{{ $t('todo.show_completed') }}</span>
      </label>
    </div>
    <div v-if="visibleTodos.length === 0" class="empty-state">{{ $t('todo.empty') }}</div>
    <div
      v-for="todo in visibleTodos"
      :key="todo.id"
      class="todo-item"
      :class="{ 'is-done': todo.status === 'done' }"
    >
      <input
        type="checkbox"
        class="todo-checkbox"
        :checked="todo.status === 'done'"
        @change="toggleStatus(todo)"
      />
      <div class="todo-content">
        <div class="todo-main">
          <span class="todo-title">{{ todo.title }}</span>
          <span v-if="todo.status === 'done' && todo.completed_at" class="todo-time">
            ({{ $t('todo.completed_at', { time: formatTime(todo.completed_at) }) }})
          </span>
        </div>
        <span class="todo-priority" :class="todo.priority || 'medium'">
          {{ getPriorityLabel(todo.priority) }}
        </span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';

const { t } = useI18n();

const props = defineProps({
  todos: {
    type: Array,
    default: () => []
  }
});

const emit = defineEmits(['update-status']);

const showCompleted = ref(false);

const visibleTodos = computed(() => {
  return props.todos
    .filter(todo => showCompleted.value || todo.status !== 'done')
    .sort((a, b) => {
      if (a.status === 'done' && b.status !== 'done') return 1;
      if (a.status !== 'done' && b.status === 'done') return -1;
      return 0;
    });
});

function formatTime(timestamp) {
  if (!timestamp) return '';
  const d = new Date(timestamp * 1000);
  return `${d.getMonth() + 1}-${d.getDate()} ${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
}

async function toggleStatus(todo) {
  const newStatus = todo.status === 'done' ? 'pending' : 'done';
  try {
    await window.appAPI.updateEventStatus(todo.id, newStatus);
    emit('update-status', { id: todo.id, status: newStatus });
  } catch (err) {
    console.error('更新待办状态失败', err);
  }
}

function getPriorityLabel(priority) {
  const map = { low: t('confirm_card.low'), medium: t('confirm_card.medium'), high: t('confirm_card.high') };
  return map[priority] || t('confirm_card.medium');
}
</script>

<style scoped>
.todo-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  background: var(--surface-card);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  padding: var(--space-4);
}

@media (max-width: 768px) {
  .todo-list {
    background: transparent !important;
    border: none !important;
    padding: 0 !important;
    border-radius: 0 !important;
  }
}

.empty-state {
  color: var(--text-tertiary);
  text-align: center;
  padding: var(--space-4);
  font-size: var(--text-sm);
}

.todo-item {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  background: var(--bg-primary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  transition: all 0.2s;
}

.todo-item:hover {
  border-color: var(--accent-primary);
}

.todo-item.is-done {
  opacity: 0.6;
}

.todo-item.is-done .todo-title {
  text-decoration: line-through;
  color: var(--text-tertiary);
}

.todo-checkbox {
  width: 18px;
  height: 18px;
  cursor: pointer;
  accent-color: var(--accent-primary);
}

.todo-content {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.todo-main {
  display: flex;
  align-items: center;
  gap: 8px;
}

.todo-time {
  font-size: 11px;
  color: var(--text-tertiary);
}

.todo-header {
  display: flex;
  justify-content: flex-end;
  padding-bottom: 8px;
}

.toggle-completed {
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
}

.toggle-text:hover {
  color: var(--text-primary);
}

.todo-title {
  font-size: var(--text-sm);
  color: var(--text-primary);
  transition: color 0.2s;
}

.todo-priority {
  font-size: 11px;
  padding: 2px 6px;
  border-radius: 4px;
}

.todo-priority.high { background: var(--color-error-bg); color: var(--color-error); }
.todo-priority.medium { background: var(--color-warning-bg); color: var(--color-warning); }
.todo-priority.low { background: var(--color-success-bg); color: var(--color-success); }
</style>
