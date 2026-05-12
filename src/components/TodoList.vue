<template>
  <div class="todo-list">
    <div v-if="todos.length === 0" class="empty-state">{{ $t('todo.empty') }}</div>
    <div
      v-for="todo in sortedTodos"
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
        <span class="todo-title">{{ todo.title }}</span>
        <span class="todo-priority" :class="todo.priority || 'medium'">
          {{ getPriorityLabel(todo.priority) }}
        </span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

const { t } = useI18n();

const props = defineProps({
  todos: {
    type: Array,
    default: () => []
  }
});

const emit = defineEmits(['update-status']);

const sortedTodos = computed(() => {
  return [...props.todos].sort((a, b) => {
    if (a.status === 'done' && b.status !== 'done') return 1;
    if (a.status !== 'done' && b.status === 'done') return -1;
    return 0;
  });
});

async function toggleStatus(todo) {
  const newStatus = todo.status === 'done' ? 'pending' : 'done';
  try {
    await window.electronAPI.updateEventStatus(todo.id, newStatus);
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
