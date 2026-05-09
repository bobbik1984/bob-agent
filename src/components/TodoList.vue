<template>
  <div class="todo-list">
    <div v-if="todos.length === 0" class="empty-state">
      还没有待办事项，快去对我说添加任务吧！
    </div>
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
  const map = { low: '低', medium: '中', high: '高' };
  return map[priority] || '中';
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

.todo-priority.high { background: rgba(239, 68, 68, 0.1); color: #ef4444; }
.todo-priority.medium { background: rgba(245, 158, 11, 0.1); color: #f59e0b; }
.todo-priority.low { background: rgba(16, 185, 129, 0.1); color: #10b981; }
</style>
