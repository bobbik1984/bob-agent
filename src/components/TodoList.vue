<template>
  <div class="todo-list">
    <div class="todo-header">
      <button class="add-todo-btn" @click="isAdding = true" :title="$t('todo.add') || '添加待办'">
        <Plus :size="16" />
      </button>
      <label class="toggle-completed" v-if="todos.some(t => t.status === 'done')">
        <input type="checkbox" v-model="showCompleted" />
        <span class="toggle-text">{{ $t('todo.show_completed') }}</span>
      </label>
    </div>

    <div v-if="isAdding" class="add-todo-form">
      <input v-model="newTitle" class="add-todo-input" :placeholder="$t('todo.title_placeholder') || '待办事项...'" @keyup.enter="saveNewTodo" autofocus />
      <textarea v-model="newDesc" class="add-todo-textarea" :placeholder="$t('todo.desc_placeholder') || '详情描述 (可选)...'" rows="2"></textarea>
      <div class="add-todo-actions">
        <button class="add-todo-cancel" @click="cancelAdd">{{ $t('common.cancel') || '取消' }}</button>
        <button class="add-todo-save" @click="saveNewTodo" :disabled="!newTitle.trim()">{{ $t('common.save') || '保存' }}</button>
      </div>
    </div>

    <div v-if="visibleTodos.length === 0 && !isAdding" class="empty-state">{{ $t('todo.empty') }}</div>
    <div
      v-for="todo in visibleTodos"
      :key="todo.id"
      class="todo-item-wrapper"
    >
      <div 
        class="todo-item"
        :class="{ 'is-done': todo.status === 'done', 'is-expanded': expandedIds.has(todo.id) }"
        @click="toggleExpand(todo.id)"
      >
        <input
          type="checkbox"
          class="todo-checkbox"
          :checked="todo.status === 'done'"
          @change.stop="toggleStatus(todo)"
          @click.stop
        />
      <div class="todo-content">
        <div class="todo-main">
          <span class="todo-title">{{ todo.title }}</span>
          <span v-if="todo.status === 'done' && todo.completed_at" class="todo-time">
            ({{ $t('todo.completed_at', { time: formatTime(todo.completed_at) }) }})
          </span>
        </div>
        <div class="todo-actions">
          <span class="todo-priority" :class="todo.priority || 'medium'">
            {{ getPriorityLabel(todo.priority) }}
          </span>
          <button class="todo-delete-btn" @click.stop="deleteTodo(todo)" :title="$t('todo.delete') || '删除'">
            <X :size="14" />
          </button>
        </div>
      </div>
      </div>
      <div v-if="expandedIds.has(todo.id)" class="todo-details">
        <textarea
          v-if="editingDescId === todo.id"
          v-model="editDescDraft"
          @blur="saveDesc(todo)"
          @keydown.ctrl.enter="saveDesc(todo)"
          @click.stop
          class="todo-desc-edit"
          ref="descInputRefs"
        ></textarea>
        <span 
          v-else 
          class="todo-desc-text" 
          @click.stop="startEditingDesc(todo)"
          :class="{ 'is-empty': !todo.description }"
        >
          {{ todo.description || '点击添加描述...' }}
        </span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, ref, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { X, Plus } from 'lucide-vue-next';

const { t } = useI18n();

const props = defineProps({
  todos: {
    type: Array,
    default: () => []
  }
});

const emit = defineEmits(['update-status', 'delete-todo', 'create-todo']);

const showCompleted = ref(false);
const expandedIds = ref(new Set());
const isAdding = ref(false);
const newTitle = ref('');
const newDesc = ref('');

const editingDescId = ref(null);
const editDescDraft = ref('');
const descInputRefs = ref([]);

function startEditingDesc(todo) {
  editingDescId.value = todo.id;
  editDescDraft.value = todo.description || '';
  nextTick(() => {
    // 聚焦到对应的 textarea
    if (descInputRefs.value) {
      let inputs = Array.isArray(descInputRefs.value) ? descInputRefs.value : [descInputRefs.value];
      const el = inputs.find(el => el && el.value === editDescDraft.value);
      if (el) el.focus();
      else if (inputs.length > 0 && inputs[0]) inputs[0].focus();
    }
  });
}

async function saveDesc(todo) {
  if (editingDescId.value !== todo.id) return;
  const newDescStr = editDescDraft.value.trim();
  
  if (newDescStr !== todo.description) {
    try {
      if (window.appAPI && window.appAPI.updateEventDescription) {
        await window.appAPI.updateEventDescription(todo.id, newDescStr);
        todo.description = newDescStr;
      }
    } catch (err) {
      console.error('更新描述失败', err);
    }
  }
  
  editingDescId.value = null;
}

function toggleExpand(id) {
  const newSet = new Set(expandedIds.value);
  if (newSet.has(id)) {
    newSet.delete(id);
  } else {
    newSet.add(id);
  }
  expandedIds.value = newSet;
}

function cancelAdd() {
  isAdding.value = false;
  newTitle.value = '';
  newDesc.value = '';
}

function saveNewTodo() {
  if (!newTitle.value.trim()) return;
  
  const pad = (n) => String(n).padStart(2, '0');
  const d = new Date();
  const dateStr = `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}`;

  emit('create-todo', {
    title: newTitle.value.trim(),
    description: newDesc.value.trim(),
    type: 'todo',
    date: dateStr
  });
  
  cancelAdd();
}

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

async function deleteTodo(todo) {
  if (confirm(t('todo.confirm_delete') || '确定要删除这条待办吗？')) {
    try {
      if (window.appAPI && window.appAPI.deleteEvent) {
        await window.appAPI.deleteEvent(todo.id);
        emit('delete-todo', todo.id);
      }
    } catch (err) {
      console.error('删除待办失败', err);
    }
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

.inbox-view.is-mobile .todo-list {
  background: transparent !important;
  border: none !important;
  padding: 0 !important;
  border-radius: 0 !important;
}

.empty-state {
  color: var(--text-tertiary);
  text-align: center;
  padding: var(--space-4);
  font-size: var(--text-sm);
}

.todo-item-wrapper {
  display: flex;
  flex-direction: column;
  gap: 4px;
  background: var(--bg-primary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  transition: all 0.2s;
}

.todo-item-wrapper:hover {
  border-color: var(--accent-primary);
}

.todo-details {
  padding: 0 12px 12px 38px;
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.5;
}

.todo-desc-text {
  display: block;
  white-space: pre-wrap;
  cursor: text;
  padding: 4px;
  border-radius: var(--radius-sm);
  transition: background 0.2s;
  min-height: 20px;
}

.todo-desc-text:hover {
  background: var(--surface-secondary);
}

.todo-desc-text.is-empty {
  font-style: italic;
  opacity: 0.6;
}

.todo-desc-edit {
  width: 100%;
  min-height: 60px;
  background: var(--surface-input);
  border: 1px solid var(--accent-primary);
  border-radius: var(--radius-sm);
  padding: 6px;
  color: var(--text-primary);
  font-size: 12px;
  resize: vertical;
  font-family: inherit;
}

.todo-desc-edit:focus {
  outline: none;
}

.todo-item {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  cursor: pointer;
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
  min-width: 0;
}

.todo-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.todo-delete-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: var(--radius-sm);
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  opacity: 0;
  transition: all 0.2s;
}

.todo-item:hover .todo-delete-btn {
  opacity: 1;
}

.todo-delete-btn:hover {
  background: color-mix(in srgb, var(--error) 15%, transparent);
  color: var(--error);
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
  justify-content: space-between;
  align-items: center;
  padding-bottom: 8px;
}

.add-todo-btn {
  background: transparent;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-sm);
  transition: all 0.2s;
}

.add-todo-btn:hover {
  background-color: var(--bg-tertiary);
  color: var(--text-primary);
}

.add-todo-form {
  display: flex;
  flex-direction: column;
  gap: 8px;
  background: var(--surface-secondary);
  padding: 12px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-primary);
  margin-bottom: var(--space-2);
}

.add-todo-input, .add-todo-textarea {
  width: 100%;
  background: var(--surface-input);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
  padding: 8px 12px;
  color: var(--text-primary);
  font-size: 13px;
}

.add-todo-input:focus, .add-todo-textarea:focus {
  outline: none;
  border-color: var(--accent-primary);
}

.add-todo-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 4px;
}

.add-todo-cancel {
  background: transparent;
  border: 1px solid var(--border-subtle);
  color: var(--text-secondary);
  padding: 4px 12px;
  border-radius: var(--radius-sm);
  font-size: 12px;
  cursor: pointer;
}

.add-todo-save {
  background: var(--accent-primary);
  border: none;
  color: var(--bg-primary);
  padding: 4px 12px;
  border-radius: var(--radius-sm);
  font-size: 12px;
  cursor: pointer;
}

.add-todo-save:disabled {
  opacity: 0.5;
  cursor: not-allowed;
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
