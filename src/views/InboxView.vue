<template>
  <div class="app-page-view inbox-view" :class="{ 'is-mobile': isMobile }">
    <!-- 移动端二级导航 Tab 栏 -->
    <div v-if="isMobile" class="mobile-tab-grid">
      <button class="mobile-tab-item" :class="{ active: activeTab === 'timeline' }" @click="activeTab = 'timeline'">
        <Calendar :size="20" class="tab-icon" />
        <span>日程</span>
      </button>
      <button class="mobile-tab-item" :class="{ active: activeTab === 'todo' }" @click="activeTab = 'todo'">
        <div style="position: relative; display: inline-flex;">
          <CheckSquare :size="20" class="tab-icon" />
          <span v-if="overdueEvents.length > 0" class="red-dot" style="top: -2px; right: -6px;"></span>
        </div>
        <span>待办</span>
      </button>
      <button class="mobile-tab-item" :class="{ active: activeTab === 'cron' }" @click="activeTab = 'cron'">
        <Zap :size="20" class="tab-icon" />
        <span>自动任务</span>
      </button>
    </div>

    <div class="app-page-scroll inbox-content-wrapper" :class="{ 'is-timeline-tab': activeTab === 'timeline' && isMobile }">
      <div v-if="isLoading" class="loading-state">
        <Loader2 :size="24" class="animate-spin" />
        <span>{{ $t('inbox.loading') }}</span>
      </div>

      <div v-else class="app-page-scroll-content inbox-content">

        <!-- 过期事件 -->
        <div v-if="overdueEvents.length > 0 && activeTab === 'todo'" class="section">
          <h3 class="section-title" style="color: var(--color-error)">
            <AlertTriangle :size="16" class="section-icon" />
            {{ $t('inbox.overdue_events') || '过期日程' }}
          </h3>
          <TodoList :todos="overdueEvents" @update-status="onTodoStatusUpdate" @delete-todo="onTodoDelete" @create-todo="onCreateEvent" />
        </div>

        <div v-if="activeTab === 'timeline'" class="section">
          <h3 class="section-title">
            <Calendar :size="16" class="section-icon" />
            {{ $t('inbox.this_week') || '本周日程' }}
          </h3>
          <WeekTimeline :weekEvents="events" @create-event="onCreateEvent" />
        </div>

        <div v-if="activeTab === 'todo'" class="section">
          <h3 class="section-title">
            <CheckSquare :size="16" class="section-icon" />
            {{ $t('inbox.todo_list') }}
          </h3>
          <TodoList :todos="todos" @update-status="onTodoStatusUpdate" @delete-todo="onTodoDelete" @create-todo="onCreateEvent" />
        </div>

        <!-- T-1211: 自动任务 -->
        <div v-if="activeTab === 'cron'" class="section">
          <h3 class="section-title">
            <Timer :size="16" class="section-icon" />
            {{ $t('inbox.auto_tasks') }}
            <span class="cron-count" v-if="cronJobs.length">{{ cronJobs.length }}</span>
          </h3>

          <div v-if="cronJobs.length === 0" class="empty-cron">
            <Clock :size="32" class="empty-icon" />
            <p>{{ $t('inbox.empty_cron') }}</p>
            <p class="empty-hint">{{ $t('inbox.empty_cron_hint') }}</p>
          </div>

          <div v-else class="cron-list">
            <div
              v-for="job in cronJobs"
              :key="job.id"
              class="cron-card"
              :class="{ disabled: !job.enabled }"
            >
              <div class="cron-main">
                <div class="cron-title-row">
                  <span class="cron-title">{{ job.title || $t('inbox.unnamed_task') }}</span>
                </div>
                <div class="cron-desc">{{ describeCron(job.cron_expr) }}</div>
                <div class="cron-prompt">{{ job.prompt_template }}</div>
                <div class="cron-meta" v-if="job.last_run > 0">
                  {{ $t('inbox.last_run') }}{{ formatTime(job.last_run) }}
                </div>
              </div>
              <div class="cron-actions">
                <button
                  class="cron-toggle"
                  :title="job.enabled ? $t('inbox.pause') : $t('inbox.resume')"
                  @click="toggleJob(job)"
                >
                  <Pause v-if="job.enabled" :size="14" />
                  <Play v-else :size="14" />
                </button>
                <button
                  class="cron-delete"
                  :title="$t('inbox.delete')"
                  @click="deleteJob(job)"
                >
                  <Trash2 :size="14" />
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, onUnmounted, computed, inject } from 'vue';
import { useI18n } from 'vue-i18n';
import { listen } from '@tauri-apps/api/event';
import { formatRelativeTime as formatTime } from '@/utils/date';
import { AlertTriangle, Calendar, Loader2, Timer, Clock, Pause, Play, Trash2, Bell, Menu, CheckSquare, Zap } from 'lucide-vue-next';
import WeekTimeline from '../components/WeekTimeline.vue';
import TodoList from '../components/TodoList.vue';

const props = defineProps({
  activePanel: { type: String, default: 'timeline' }
});
const emit = defineEmits(['toggle-sidebar', 'update:activePanel']);
const { t, locale } = useI18n();
const isMobile = inject('isMobile');
const activeTab = computed({
  get: () => props.activePanel,
  set: (val) => emit('update:activePanel', val)
});

const isLoading = ref(true);
const events = ref([]);
const todos = ref([]);
const cronJobs = ref([]);
const reminders = ref([]);

const overdueEvents = computed(() => {
  const today = new Date();
  const pad = (n) => String(n).padStart(2, '0');
  const todayStr = `${today.getFullYear()}-${pad(today.getMonth()+1)}-${pad(today.getDate())}`;

  return events.value.filter(e => e.status === 'pending' && e.date && e.date < todayStr && !e.linked_ticket_id);
});

onMounted(async () => {
  window.addEventListener('sync:refresh-events', reloadEvents);
  try {
    const [allEvents, cronResult] = await Promise.all([
      window.appAPI.listEvents(),
      window.appAPI.listCronJobs(),
    ]);

    // 过滤分配
    events.value = allEvents.filter(e => e.type === 'event');
    todos.value = allEvents.filter(e => e.type === 'todo');

    if (cronResult?.jobs) {
      cronJobs.value = cronResult.jobs;
    }
  } catch (err) {
    console.error('加载事件失败', err);
  } finally {
    isLoading.value = false;
  }
});

// 监听 scheduler:completed 实时刷新
let unlistenScheduler = null;
onMounted(() => {
  unlistenScheduler = window.appAPI.onSchedulerCompleted?.((payload) => {
    console.log('[InboxView] scheduler:completed', payload);
    // 刷新 cron job 列表（更新 last_run 等）
    window.appAPI.listCronJobs().then(result => {
      if (result?.jobs) cronJobs.value = result.jobs;
    });
  });
});
onUnmounted(() => {
  if (unlistenScheduler) unlistenScheduler();
});



// T-1307: 监听待办提醒事件
let unlistenReminder = null;
onMounted(() => {
  unlistenReminder = window.appAPI.onTodoReminder?.((payload) => {
    // 避免重复
    if (!reminders.value.find(r => r.id === payload.id)) {
      reminders.value.push(payload);
    }
  });
});
onUnmounted(() => {
  if (unlistenReminder) unlistenReminder();
});


  const reloadEvents = async () => {
    try {
      const allEvents = await window.appAPI.listEvents();
      events.value = allEvents.filter(e => e.type === 'event');
      todos.value = allEvents.filter(e => e.type === 'todo');
    } catch (err) {
      console.error('Failed to reload events', err);
    }
  };

  let unlistenCalendar = null;
  onMounted(() => {
    window.addEventListener('ticket-created', reloadEvents);
    if (window.appAPI.onCalendarUpdated) {
      unlistenCalendar = window.appAPI.onCalendarUpdated(reloadEvents);
    }
  });

  onUnmounted(() => {
    window.removeEventListener('sync:refresh-events', reloadEvents);
    window.removeEventListener('ticket-created', reloadEvents);
    if (unlistenCalendar) unlistenCalendar();
  });

  function dismissReminder(index) {
  reminders.value.splice(index, 1);
}

function onTodoStatusUpdate({ id, status }) {
  let item = todos.value.find(t => t.id === id);
  if (!item) item = events.value.find(e => e.id === id);
  if (item) {
    item.status = status;
  }
}

function onTodoDelete(id) {
  todos.value = todos.value.filter(t => t.id !== id);
  events.value = events.value.filter(e => e.id !== id);
}

async function onCreateEvent(payload) {
  try {
    const res = await window.appAPI.confirmEvent(payload);
    if (res?.ok) {
      // reload events
      const allEvents = await window.appAPI.listEvents();
      events.value = allEvents.filter(e => e.type === 'event');
      todos.value = allEvents.filter(e => e.type === 'todo');
    }
  } catch (e) {
    console.error('Failed to create event', e);
  }
}

async function toggleJob(job) {
  const newEnabled = !job.enabled;
  const result = await window.appAPI.toggleCronJob(job.id, newEnabled);
  if (result?.ok) {
    job.enabled = newEnabled;
  }
}

async function deleteJob(job) {
  const result = await window.appAPI.removeCronJob(job.id);
  if (result?.ok) {
    cronJobs.value = cronJobs.value.filter(j => j.id !== job.id);
  }
}


// 将 cron 表达式翻译为人类可读描述
function describeCron(expr) {
  if (!expr) return '';
  const parts = expr.trim().split(/\s+/);
  if (parts.length !== 5) return expr;
  const [min, hour, dom, mon, dow] = parts;
  const isZh = locale.value.startsWith('zh');

  const weekdays = isZh
    ? { '0': '周日', '1': '周一', '2': '周二', '3': '周三', '4': '周四', '5': '周五', '6': '周六' }
    : { '0': 'Sun', '1': 'Mon', '2': 'Tue', '3': 'Wed', '4': 'Thu', '5': 'Fri', '6': 'Sat' };

  if (min.startsWith('*/')) {
    return isZh ? `每 ${min.slice(2)} 分钟` : `Every ${min.slice(2)} minutes`;
  }
  if (hour.startsWith('*/')) {
    return isZh ? `每 ${hour.slice(2)} 小时` : `Every ${hour.slice(2)} hours`;
  }

  let timeStr = '';
  if (hour !== '*' && min !== '*') {
    timeStr = `${hour.padStart(2, '0')}:${min.padStart(2, '0')}`;
  } else if (hour !== '*') {
    timeStr = `${hour}:00`;
  }

  if (dom === '*' && mon === '*' && dow === '*') {
    if (timeStr) {
      return isZh ? `每天 ${timeStr}` : `Every day at ${timeStr}`;
    }
    return isZh ? '每分钟' : 'Every minute';
  }
  if (dom === '*' && mon === '*' && dow === '1-5') {
    return isZh ? `工作日 ${timeStr}` : `Weekdays at ${timeStr}`;
  }
  if (dom === '*' && mon === '*' && dow === '0,6') {
    return isZh ? `周末 ${timeStr}` : `Weekends at ${timeStr}`;
  }
  if (dom === '*' && mon === '*' && dow !== '*') {
    const days = dow.split(',').map(d => weekdays[d] || (isZh ? `周${d}` : `day ${d}`)).join(isZh ? '、' : ', ');
    return isZh ? `每${days} ${timeStr}` : `Every ${days} at ${timeStr}`;
  }
  if (dom !== '*' && mon === '*') {
    return isZh ? `每月${dom}日 ${timeStr}` : `Every month on day ${dom} at ${timeStr}`;
  }
  return expr;
}
</script>

<style scoped>
.inbox-view {
  min-width: 0;
}

.inbox-content-wrapper {
  /* Used as app-page-scroll */
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
}

.inbox-header {
  text-align: left;
}

.inbox-title {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  color: var(--text-tertiary);
  padding: var(--space-8);
}

.inbox-content {
  /* Used as app-page-scroll-content */
  max-width: 1000px;
  width: 100%;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.section-title {
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--text-secondary);
  margin: 0 0 var(--space-3) 0;
  height: 36px;
  display: flex;
  align-items: center;
  gap: 6px;
}

.section-icon {
  color: var(--text-tertiary);
}

.cron-count {
  background: var(--surface-tertiary);
  color: var(--text-secondary);
  font-size: 11px;
  padding: 2px 6px;
  border-radius: 12px;
  margin-left: 8px;
}

/* ── 空状态 ── */
.empty-cron {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px;
  color: var(--text-muted);
  background: var(--surface-secondary);
  border-radius: var(--radius-lg);
  border: 1px dashed var(--border-primary);
  text-align: center;
}

.empty-cron .empty-icon {
  margin-bottom: 12px;
  opacity: 0.5;
}

.empty-cron .empty-hint {
  font-size: 12px;
  margin-top: 4px;
}

/* ── 自动任务列表 ── */
.cron-list {
  display: grid;
  gap: 12px;
}

.cron-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px;
  background: var(--surface-card);
  border: 1px solid var(--border-primary);
  border-radius: var(--radius-lg);
  transition: all 0.2s;
}

.cron-card:hover {
  border-color: var(--border-strong);
  box-shadow: var(--shadow-sm);
}

.cron-card.disabled {
  opacity: 0.6;
  background: var(--surface-secondary);
}

.cron-main {
  flex: 1;
  min-width: 0;
  padding-right: 16px;
}

.cron-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}

.cron-title {
  font-weight: 500;
  color: var(--text-primary);
}

.cron-desc {
  font-size: 12px;
  color: var(--user-accent, var(--accent-primary));
  font-family: var(--font-mono);
  margin-bottom: 6px;
}

.cron-prompt {
  font-size: 13px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.cron-meta {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-top: 8px;
}

.cron-actions {
  display: flex;
  gap: 8px;
}

.cron-toggle,
.cron-delete {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: 8px;
  border: none;
  background: var(--surface-tertiary);
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.2s;
}

.cron-toggle:hover {
  background: var(--surface-hover);
  color: var(--text-primary);
}

.cron-delete:hover {
  background: color-mix(in srgb, var(--color-error) 15%, transparent);
  color: var(--color-error);
}

/* ── T-1307: 待办提醒 ── */
.reminder-section {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: var(--space-2);
}

.reminder-alert {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: color-mix(in srgb, var(--accent-primary) 6%, var(--surface-secondary));
  border: 1px solid color-mix(in srgb, var(--accent-primary) 20%, var(--border-primary));
  border-left: 3px solid var(--accent-primary);
  border-radius: var(--radius-lg, 10px);
  font-size: 13px;
}

.reminder-icon {
  color: var(--accent-primary);
  flex-shrink: 0;
}

.reminder-text {
  flex: 1;
  color: var(--text-primary);
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.reminder-date {
  font-size: 11px;
  color: var(--text-tertiary);
}

.reminder-dismiss {
  background: none;
  border: none;
  color: var(--text-muted);
  font-size: 16px;
  cursor: pointer;
  padding: 0 4px;
  line-height: 1;
  opacity: 0.5;
  transition: opacity 0.15s;
}

.reminder-dismiss:hover {
  opacity: 1;
}

.red-dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  background: var(--color-error);
  border-radius: 50%;
  position: absolute;
  top: 8px;
  right: 12px;
}

.inbox-view.is-mobile .inbox-content-wrapper.is-timeline-tab .inbox-content {
  padding: 12px 0 0 0 !important;
}
.inbox-view.is-mobile .inbox-content-wrapper.is-timeline-tab .section {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  margin: 0 !important;
}
</style>
