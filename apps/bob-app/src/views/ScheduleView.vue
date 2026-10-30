<script setup>
import { computed, reactive, ref } from 'vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  schedules: {
    type: Array,
    default: () => [],
  },
  busy: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits(['create-schedule', 'update-status']);

const { t } = useI18n();
const activeSubtab = ref('timeline'); // 'timeline' | 'todo' | 'cron'

const draft = reactive({
  title: '',
  kind: 'event', // 'event' | 'todo'
  dueDate: '',
  notes: '',
});

const timelineItems = computed(() =>
  (props.schedules || []).filter(item => item.kind === 'event'),
);

const todoItems = computed(() =>
  (props.schedules || []).filter(item => item.kind !== 'event'),
);

const filteredItems = computed(() => {
  if (activeSubtab.value === 'timeline') return timelineItems.value;
  if (activeSubtab.value === 'todo') return todoItems.value;
  return [];
});

function switchSubtab(tab) {
  activeSubtab.value = tab;
  if (tab === 'timeline') draft.kind = 'event';
  else if (tab === 'todo') draft.kind = 'todo';
}

function submitNewSchedule() {
  const title = draft.title.trim();
  if (!title) return;
  let dueAtMs = null;
  if (draft.dueDate) {
    const parsed = Date.parse(draft.dueDate);
    if (!Number.isNaN(parsed)) dueAtMs = parsed;
  }
  emit('create-schedule', {
    id: `sched-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    title,
    kind: draft.kind,
    dueAtMs,
    notes: draft.notes.trim(),
  });
  draft.title = '';
  draft.notes = '';
}

function formatDue(ms) {
  if (!ms) return '';
  try {
    return new Date(Number(ms)).toLocaleString([], {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return '';
  }
}

function toggleDone(item) {
  const nextStatus = item.status === 'done' ? 'scheduled' : 'done';
  emit('update-status', { id: item.id, status: nextStatus });
}

function cancelItem(item) {
  emit('update-status', { id: item.id, status: 'cancelled' });
}
</script>

<template>
  <section class="primary-view schedule-view" :aria-label="t('scheduleViewTitle')">
    <header class="view-hero">
      <div class="view-hero-titles">
        <h2>{{ t('scheduleViewTitle') }}</h2>
        <p>{{ t('scheduleViewSubtitle') }}</p>
      </div>
      <div class="view-subtabs" role="tablist">
        <button
          type="button"
          class="view-subtab-btn"
          :class="{ active: activeSubtab === 'timeline' }"
          @click="switchSubtab('timeline')"
        >
          📅 {{ t('scheduleTabTimeline') }}
          <span class="subtab-count">{{ timelineItems.length }}</span>
        </button>
        <button
          type="button"
          class="view-subtab-btn"
          :class="{ active: activeSubtab === 'todo' }"
          @click="switchSubtab('todo')"
        >
          ☑ {{ t('scheduleTabTodo') }}
          <span class="subtab-count">{{ todoItems.length }}</span>
        </button>
        <button
          type="button"
          class="view-subtab-btn"
          :class="{ active: activeSubtab === 'cron' }"
          @click="switchSubtab('cron')"
        >
          ⏱ {{ t('scheduleTabCron') }}
        </button>
      </div>
    </header>

    <div class="view-content-body">
      <template v-if="activeSubtab !== 'cron'">
        <form class="quick-schedule-form hub-card" @submit.prevent="submitNewSchedule">
          <div class="quick-schedule-row">
            <select v-model="draft.kind" class="schedule-kind-select">
              <option value="event">{{ t('scheduleKindEvent') }}</option>
              <option value="todo">{{ t('scheduleKindTodo') }}</option>
            </select>
            <input
              v-model="draft.title"
              type="text"
              class="schedule-title-input"
              :placeholder="t('scheduleNewTitlePlaceholder')"
              required
            />
            <input
              v-model="draft.dueDate"
              type="datetime-local"
              class="schedule-date-input"
              :aria-label="t('scheduleDueLabel')"
            />
            <button type="submit" class="primary-action-btn" :disabled="busy || !draft.title.trim()">
              {{ t('scheduleAddBtn') }}
            </button>
          </div>
          <input
            v-model="draft.notes"
            type="text"
            class="schedule-notes-input"
            :placeholder="t('scheduleNotesPlaceholder')"
          />
        </form>

        <div v-if="!filteredItems.length" class="view-empty-card hub-card">
          <p>{{ t('scheduleEmptyHint') }}</p>
        </div>

        <div v-else class="schedule-items-list">
          <article
            v-for="item in filteredItems"
            :key="item.id"
            class="schedule-item-card hub-card"
            :class="{ done: item.status === 'done', cancelled: item.status === 'cancelled' }"
          >
            <div class="schedule-item-main">
              <button
                type="button"
                class="schedule-check-btn"
                :class="{ checked: item.status === 'done' }"
                :aria-label="item.status === 'done' ? t('scheduleReopenBtn') : t('scheduleMarkDoneBtn')"
                @click="toggleDone(item)"
              >
                <span v-if="item.status === 'done'">✓</span>
              </button>
              <div class="schedule-item-text">
                <div class="schedule-item-title-row">
                  <strong>{{ item.title }}</strong>
                  <span class="schedule-kind-badge">
                    {{ item.kind === 'event' ? t('scheduleKindEvent') : t('scheduleKindTodo') }}
                  </span>
                  <span class="schedule-status-badge" :class="item.status">
                    {{
                      t(
                        item.status === 'done'
                          ? 'scheduleStatusDone'
                          : item.status === 'cancelled'
                            ? 'scheduleStatusCancelled'
                            : 'scheduleStatusScheduled',
                      )
                    }}
                  </span>
                </div>
                <p v-if="item.notes" class="schedule-item-notes">{{ item.notes }}</p>
                <span v-if="item.due_at_ms" class="schedule-item-due">
                  🕒 {{ formatDue(item.due_at_ms) }}
                </span>
              </div>
            </div>
            <div class="schedule-item-actions">
              <button type="button" class="mini-ghost-btn" @click="toggleDone(item)">
                {{ item.status === 'done' ? t('scheduleReopenBtn') : t('scheduleMarkDoneBtn') }}
              </button>
              <button
                v-if="item.status !== 'cancelled'"
                type="button"
                class="mini-ghost-btn"
                @click="cancelItem(item)"
              >
                {{ t('scheduleCancelBtn') }}
              </button>
            </div>
          </article>
        </div>
      </template>

      <template v-else>
        <section class="hub-card cron-card">
          <div class="hub-card-header">
            <div class="hub-card-title-group">
              <h3>{{ t('dailyRoutineCardTitle') }}</h3>
              <span class="hub-count-pill">@daily_startup</span>
            </div>
          </div>
          <p class="view-card-desc">{{ t('scheduleCronHint') }}</p>
          <div class="routine-tag-box">
            <code>@daily_startup</code>
            <span>{{ t('dailyRoutineTagHint') }}</span>
          </div>
        </section>
      </template>
    </div>
  </section>
</template>
