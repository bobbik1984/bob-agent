<template>
  <div class="inbox-view">
    <div class="inbox-header">
      <h2 class="inbox-title">📅 智能收件箱</h2>
      <p class="inbox-subtitle">日程管理 + 待办清单</p>
    </div>

    <div v-if="isLoading" class="loading-state">
      ⏳ 加载中...
    </div>

    <div v-else class="inbox-content">
      <div class="section">
        <h3 class="section-title">本周日程</h3>
        <WeekTimeline :weekEvents="events" />
      </div>

      <div class="section">
        <h3 class="section-title">待办清单</h3>
        <TodoList :todos="todos" @update-status="onTodoStatusUpdate" />
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue';
import WeekTimeline from '../components/WeekTimeline.vue';
import TodoList from '../components/TodoList.vue';

const isLoading = ref(true);
const events = ref([]);
const todos = ref([]);

onMounted(async () => {
  try {
    const allEvents = await window.electronAPI.listEvents();

    // 过滤分配
    events.value = allEvents.filter(e => e.type === 'event');
    todos.value = allEvents.filter(e => e.type === 'todo');
  } catch (err) {
    console.error('加载事件失败', err);
  } finally {
    isLoading.value = false;
  }
});

function onTodoStatusUpdate({ id, status }) {
  const todo = todos.value.find(t => t.id === id);
  if (todo) {
    todo.status = status;
  }
}
</script>

<style scoped>
.inbox-view {
  height: 100%;
  display: flex;
  flex-direction: column;
  padding: var(--space-6);
  gap: var(--space-6);
  overflow-y: auto;
}

.inbox-header {
  text-align: left;
}

.inbox-title {
  font-size: var(--text-2xl);
  font-weight: 600;
  color: var(--text-primary);
}

.inbox-subtitle {
  color: var(--text-tertiary);
  margin-top: var(--space-2);
}

.loading-state {
  text-align: center;
  color: var(--text-tertiary);
  padding: var(--space-8);
}

.inbox-content {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.section-title {
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: var(--space-3);
}
</style>
