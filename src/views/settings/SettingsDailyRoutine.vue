<template>
  <section class="settings-section card">
    <h3 class="section-title">
      <Sunrise :size="16" class="section-icon" />
      {{ t('settings.daily_routine') }}
    </h3>


    <!-- 任务列表 -->
    <div v-if="loading" class="routine-loading">
      <Loader2 :size="16" class="spin" />
      <span>{{ t('settings.loading') || '加载中...' }}</span>
    </div>

    <div v-else-if="jobs.length === 0" class="routine-empty">
      <CalendarCheck :size="32" class="empty-icon" />
      <p class="empty-text">{{ t('settings.daily_routine_empty') }}</p>
      <p class="empty-hint">{{ t('settings.daily_routine_hint') }}</p>
    </div>

    <TransitionGroup v-else name="job-list" tag="div" class="routine-jobs">
      <div v-for="job in jobs" :key="job.id" class="routine-job-card">
        <div class="job-main">
          <div class="job-icon-wrap">
            <Zap :size="14" />
          </div>
          <div class="job-info">
            <div class="job-title">{{ job.title }}</div>
            <div class="job-prompt">{{ job.prompt_template }}</div>
            <div class="job-meta">
              <span v-if="job.last_run > 0" class="job-last-run">
                {{ t('settings.last_run') }}: {{ formatTime(job.last_run) }}
              </span>
              <span v-else class="job-last-run never">
                {{ t('settings.never_run') }}
              </span>
            </div>
          </div>
        </div>
        <div class="job-actions">
          <button
            class="job-toggle"
            :class="{ active: job.enabled }"
            :title="job.enabled ? t('settings.disable') : t('settings.enable')"
            @click="toggleJob(job)"
          >
            <div class="toggle-track">
              <div class="toggle-thumb"></div>
            </div>
          </button>
          <button
            class="job-delete"
            :title="t('settings.delete')"
            @click="deleteJob(job)"
          >
            <Trash2 :size="14" />
          </button>
        </div>
      </div>
    </TransitionGroup>


  </section>
</template>

<script setup>
import { ref, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { formatRelativeTime as formatTime } from '@/utils/date';
import { Sunrise, Loader2, CalendarCheck, Zap, Trash2 } from 'lucide-vue-next';

const { t } = useI18n();
defineProps({ config: { type: Object, required: true } });
defineEmits(['config-changed']);

const loading = ref(true);
const jobs = ref([]);

async function loadJobs() {
  loading.value = true;
  try {
    const result = await window.appAPI.listCronJobs();
    if (result?.ok && result.jobs) {
      // 只显示 @daily_startup 类型的任务
      jobs.value = result.jobs.filter(j => j.cron_expr === '@daily_startup');
    }
  } catch (err) {
    console.error('[SettingsDailyRoutine] Failed to load jobs:', err);
  } finally {
    loading.value = false;
  }
}

async function toggleJob(job) {
  const newEnabled = !job.enabled;
  try {
    const result = await window.appAPI.toggleCronJob(job.id, newEnabled);
    if (result?.ok) {
      job.enabled = newEnabled;
    }
  } catch (err) {
    console.error('[SettingsDailyRoutine] Toggle failed:', err);
  }
}

async function deleteJob(job) {
  try {
    const result = await window.appAPI.removeCronJob(job.id);
    if (result?.ok) {
      jobs.value = jobs.value.filter(j => j.id !== job.id);
    }
  } catch (err) {
    console.error('[SettingsDailyRoutine] Delete failed:', err);
  }
}


onMounted(() => {
  loadJobs();
});
</script>

<style scoped>
.settings-section {
  margin-bottom: var(--space-5);
}

.section-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--text-lg);
  font-weight: 500;
  margin-bottom: var(--space-4);
  color: var(--text-primary);
}

.section-desc {
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  margin: 0 0 var(--space-4) 0;
  line-height: 1.5;
}

/* Loading */
.routine-loading {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: var(--space-4);
  color: var(--text-tertiary);
  font-size: var(--text-sm);
}

.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

/* Empty state */
.routine-empty {
  text-align: center;
  padding: var(--space-6) var(--space-4);
}

.empty-icon {
  color: var(--text-tertiary);
  opacity: 0.4;
  margin-bottom: var(--space-3);
}

.empty-text {
  font-size: var(--text-sm);
  color: var(--text-secondary);
  margin: 0 0 var(--space-1) 0;
}

.empty-hint {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  margin: 0;
}

/* Job cards */
.routine-jobs {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.routine-job-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-3) var(--space-3);
  background: var(--bg-tertiary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  transition: all var(--duration-fast) var(--ease-out);
}

.routine-job-card:hover {
  border-color: var(--border-default);
  background: var(--bg-hover);
}

.job-main {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  flex: 1;
  min-width: 0;
}

.job-icon-wrap {
  flex-shrink: 0;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-sm);
  background: var(--bg-hover);
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  justify-content: center;
  margin-top: 2px;
}

.job-info {
  flex: 1;
  min-width: 0;
}

.job-title {
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--text-primary);
  margin-bottom: 2px;
}

.job-prompt {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  line-height: 1.4;
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

.job-meta {
  margin-top: 4px;
}

.job-last-run {
  font-size: 11px;
  color: var(--text-tertiary);
}

.job-last-run.never {
  font-style: italic;
  opacity: 0.6;
}

/* Actions */
.job-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-shrink: 0;
  margin-left: var(--space-3);
}

/* Toggle switch */
.job-toggle {
  background: none;
  border: none;
  cursor: pointer;
  padding: 2px;
}

.toggle-track {
  width: 32px;
  height: 18px;
  border-radius: 9px;
  background: var(--bg-active);
  transition: background var(--duration-fast) var(--ease-out);
  position: relative;
}

.job-toggle.active .toggle-track {
  background: var(--user-accent);
}

.toggle-thumb {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--text-primary);
  position: absolute;
  top: 2px;
  left: 2px;
  transition: transform var(--duration-fast) var(--ease-out);
}

.job-toggle.active .toggle-thumb {
  transform: translateX(14px);
  background: var(--bg-primary);
}

/* Delete button */
.job-delete {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 4px;
  border-radius: var(--radius-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all var(--duration-fast) var(--ease-out);
}

.job-delete:hover {
  background: var(--color-error-bg);
  color: var(--color-error);
}



/* Transition animation */
.job-list-enter-active,
.job-list-leave-active {
  transition: all var(--duration-normal) var(--ease-out);
}

.job-list-enter-from {
  opacity: 0;
  transform: translateX(-10px);
}

.job-list-leave-to {
  opacity: 0;
  transform: translateX(10px);
}
</style>
