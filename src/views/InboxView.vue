<template>
  <div class="inbox-view">
    <div class="inbox-content-wrapper">
      <div class="inbox-header">
        <h2 class="inbox-title">
          <Calendar :size="24" class="title-icon" />{{ $t('inbox.title') }}</h2>
      </div>

      <div v-if="isLoading" class="loading-state">
        <Loader2 :size="24" class="animate-spin" />
        <span>{{ $t('inbox.loading') }}</span>
      </div>

      <div v-else class="inbox-content">
        <!-- T-1307: 待办提醒横幅 -->
        <div v-if="reminders.length > 0" class="reminder-section">
          <div
            v-for="(r, ri) in reminders"
            :key="r.id || ri"
            class="reminder-alert"
          >
            <Bell :size="14" class="reminder-icon" />
            <span class="reminder-text">
              <strong>{{ r.title }}</strong>
              <span v-if="r.date" class="reminder-date">{{ r.date }}</span>
            </span>
            <button class="reminder-dismiss" @click="dismissReminder(ri)">&times;</button>
          </div>
        </div>
        <div class="section">
          <h3 class="section-title">{{ $t('inbox.this_week') }}</h3>
          <WeekTimeline :weekEvents="events" />
        </div>

        <div class="section">
          <h3 class="section-title">{{ $t('inbox.todo_list') }}</h3>
          <TodoList :todos="todos" @update-status="onTodoStatusUpdate" />
        </div>

        <!-- T-1211: 自动任务区域 -->
        <div class="section">
          <h3 class="section-title">
            <Timer :size="16" class="section-icon" />
            自动任务
            <span class="cron-count" v-if="cronJobs.length">{{ cronJobs.length }}</span>
          </h3>

          <div v-if="cronJobs.length === 0" class="empty-cron">
            <Clock :size="32" class="empty-icon" />
            <p>暂无定时任务</p>
            <p class="empty-hint">在对话中告诉 Bob「每天早上8点帮我播报新闻」即可创建</p>
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
                  <span class="cron-title">{{ job.title || '未命名任务' }}</span>
                  <code class="cron-expr">{{ job.cron_expr }}</code>
                </div>
                <div class="cron-desc">{{ describeCron(job.cron_expr) }}</div>
                <div class="cron-prompt">{{ job.prompt_template }}</div>
                <div class="cron-meta" v-if="job.last_run > 0">
                  上次执行：{{ formatTime(job.last_run) }}
                </div>
              </div>
              <div class="cron-actions">
                <button
                  class="cron-toggle"
                  :title="job.enabled ? '暂停' : '恢复'"
                  @click="toggleJob(job)"
                >
                  <Pause v-if="job.enabled" :size="14" />
                  <Play v-else :size="14" />
                </button>
                <button
                  class="cron-delete"
                  title="删除"
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
import { ref, onMounted, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { Calendar, Loader2, Timer, Clock, Pause, Play, Trash2, Bell } from 'lucide-vue-next';
import WeekTimeline from '../components/WeekTimeline.vue';
import TodoList from '../components/TodoList.vue';

const { t } = useI18n();

const isLoading = ref(true);
const events = ref([]);
const todos = ref([]);
const cronJobs = ref([]);
const reminders = ref([]);

onMounted(async () => {
  try {
    const [allEvents, cronResult] = await Promise.all([
      window.electronAPI.listEvents(),
      window.electronAPI.listCronJobs(),
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
  unlistenScheduler = window.electronAPI.onSchedulerCompleted?.((payload) => {
    console.log('[InboxView] scheduler:completed', payload);
    // 刷新 cron job 列表（更新 last_run 等）
    window.electronAPI.listCronJobs().then(result => {
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
  unlistenReminder = window.electronAPI.onTodoReminder?.((payload) => {
    // 避免重复
    if (!reminders.value.find(r => r.id === payload.id)) {
      reminders.value.push(payload);
    }
  });
});
onUnmounted(() => {
  if (unlistenReminder) unlistenReminder();
});

function dismissReminder(index) {
  reminders.value.splice(index, 1);
}

function onTodoStatusUpdate({ id, status }) {
  const todo = todos.value.find(t => t.id === id);
  if (todo) {
    todo.status = status;
  }
}

async function toggleJob(job) {
  const newEnabled = !job.enabled;
  const result = await window.electronAPI.toggleCronJob(job.id, newEnabled);
  if (result?.ok) {
    job.enabled = newEnabled;
  }
}

async function deleteJob(job) {
  const result = await window.electronAPI.removeCronJob(job.id);
  if (result?.ok) {
    cronJobs.value = cronJobs.value.filter(j => j.id !== job.id);
  }
}

function formatTime(ms) {
  if (!ms) return '';
  const d = new Date(ms);
  const now = new Date();
  const isToday = d.toDateString() === now.toDateString();
  const time = d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' });
  return isToday ? `今天 ${time}` : d.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' }) + ' ' + time;
}

// 将 cron 表达式翻译为人类可读描述
function describeCron(expr) {
  if (!expr) return '';
  const parts = expr.trim().split(/\s+/);
  if (parts.length !== 5) return expr;
  const [min, hour, dom, mon, dow] = parts;

  const weekdays = { '0': '周日', '1': '周一', '2': '周二', '3': '周三', '4': '周四', '5': '周五', '6': '周六' };

  // 常见模式匹配
  if (min.startsWith('*/')) return `每 ${min.slice(2)} 分钟`;
  if (hour.startsWith('*/')) return `每 ${hour.slice(2)} 小时`;

  let timeStr = '';
  if (hour !== '*' && min !== '*') {
    timeStr = `${hour.padStart(2, '0')}:${min.padStart(2, '0')}`;
  } else if (hour !== '*') {
    timeStr = `${hour}:00`;
  }

  if (dom === '*' && mon === '*' && dow === '*') {
    return timeStr ? `每天 ${timeStr}` : '每分钟';
  }
  if (dom === '*' && mon === '*' && dow === '1-5') {
    return `工作日 ${timeStr}`;
  }
  if (dom === '*' && mon === '*' && dow === '0,6') {
    return `周末 ${timeStr}`;
  }
  if (dom === '*' && mon === '*' && dow !== '*') {
    const days = dow.split(',').map(d => weekdays[d] || `周${d}`).join('、');
    return `每${days} ${timeStr}`;
  }
  if (dom !== '*' && mon === '*') {
    return `每月${dom}日 ${timeStr}`;
  }
  return expr; // fallback: 原始表达式
}
</script>

<style scoped>
.inbox-view {
  flex: 1;
  min-width: 0;
  height: 100%;
  overflow-y: auto;
  padding: var(--space-6) var(--space-8);
}

.inbox-content-wrapper {
  max-width: 1000px;
  width: 100%;
  margin: 0 auto;
  padding: 0;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.inbox-header {
  text-align: left;
}

.inbox-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-2xl);
  font-weight: 600;
  color: var(--text-primary);
}

.title-icon {
  color: var(--text-secondary);
}

.inbox-subtitle {
  color: var(--text-tertiary);
  margin-top: var(--space-2);
}

.loading-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-3);
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
  display: flex;
  align-items: center;
  gap: 6px;
}



.cron-count {
  font-size: 11px;
  font-weight: 500;
  background: var(--accent-primary);
  color: #fff;
  border-radius: 10px;
  padding: 0 7px;
  line-height: 18px;
  margin-left: 4px;
}

/* ── 空状态 ── */
.empty-cron {
  text-align: center;
  padding: var(--space-6) var(--space-4);
  color: var(--text-tertiary);
}

.empty-icon {
  opacity: 0.3;
  margin-bottom: var(--space-2);
}

.empty-hint {
  font-size: 12px;
  opacity: 0.7;
  margin-top: 4px;
}

/* ── Cron 卡片列表 ── */
.cron-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.cron-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 14px;
  background: var(--surface-secondary);
  border: 1px solid var(--border-primary);
  border-radius: var(--radius-lg, 10px);
  transition: opacity 0.2s, border-color 0.2s;
}

.cron-card.disabled {
  opacity: 0.45;
}

.cron-card:hover {
  border-color: var(--accent-primary);
}

.cron-main {
  flex: 1;
  min-width: 0;
}

.cron-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}

.cron-title {
  font-weight: 600;
  font-size: 14px;
  color: var(--text-primary);
}

.cron-expr {
  font-size: 11px;
  font-family: var(--font-mono);
  background: var(--surface-input);
  padding: 1px 6px;
  border-radius: 4px;
  color: var(--text-muted);
}

.cron-desc {
  font-size: 12px;
  color: var(--accent-primary);
  margin-bottom: 4px;
}

.cron-prompt {
  font-size: 12px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

.cron-meta {
  font-size: 11px;
  color: var(--text-muted);
  margin-top: 4px;
  opacity: 0.7;
}

/* ── 操作按钮 ── */
.cron-actions {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex-shrink: 0;
}

.cron-toggle, .cron-delete {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-sm, 6px);
  border: 1px solid var(--border-primary);
  background: var(--surface-primary);
  color: var(--text-muted);
  cursor: pointer;
  transition: all 0.15s;
}

.cron-toggle:hover {
  color: var(--accent-primary);
  border-color: var(--accent-primary);
  background: color-mix(in srgb, var(--accent-primary) 8%, transparent);
}

.cron-delete:hover {
  color: #e74c3c;
  border-color: #e74c3c;
  background: color-mix(in srgb, #e74c3c 8%, transparent);
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
</style>
