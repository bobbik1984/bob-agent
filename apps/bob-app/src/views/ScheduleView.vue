<script setup>
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import CustomSelect from '../components/CustomSelect.vue';

const props = defineProps({
  saveItem: { type: Function, default: null },
  schedules: {
    type: Array,
    default: () => [],
  },
  projects: {
    type: Array,
    default: () => [],
  },
  onlineProjects: {
    type: Array,
    default: () => [],
  },
  busy: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits([
  'create-schedule',
  'update-status',
  'open-project-todo',
  'promote-todo-project',
]);

const { t } = useI18n();
const activeSubtab = ref('timeline'); // 'timeline' | 'todo' | 'cron'
const showCreateForm = ref(false);
const editor = ref(null), editingItem = ref(null), saving = ref(false), saveError = ref('');
let returnFocus = null;
const timelineViewMode = ref('grid'); // 'grid' | 'list'
const weekOffset = ref(0);
const selectedProjectFilter = ref('__all__');

const scheduleKindOptions = computed(() => [
  { value: 'event', label: t('scheduleKindEvent') },
  { value: 'todo', label: t('scheduleKindTodo') },
]);

const allAvailableProjects = computed(() => {
  const map = new Map();
  for (const p of props.projects || []) {
    if (p && p.id) {
      map.set(p.id, {
        id: p.id,
        title: p.title || p.name || p.id,
        source: 'local',
      });
    }
  }
  for (const op of props.onlineProjects || []) {
    if (op && op.id && !map.has(op.id)) {
      map.set(op.id, {
        id: op.id,
        title: `${op.title || op.name || op.id} (Cloud)`,
        source: 'online',
      });
    }
  }
  return Array.from(map.values());
});

const projectSelectOptions = computed(() => [
  { value: '', label: t('inboxProjectUnassigned') },
  ...allAvailableProjects.value.map(p => ({
    value: p.id,
    label: p.title,
  })),
]);

const draft = reactive({
  title: '',
  kind: 'event', // 'event' | 'todo'
  projectId: '',
  dueDate: '',
  notes: '',
});

const inboxDraft = reactive({
  content: '',
  projectId: '',
});

const timelineItems = computed(() =>
  (props.schedules || []).filter(item => item.kind === 'event'),
);

const todoItems = computed(() =>
  (props.schedules || []).filter(item => item.kind !== 'event'),
);

const filteredTodoItems = computed(() => {
  const list = todoItems.value;
  if (selectedProjectFilter.value === '__all__') return list;
  if (selectedProjectFilter.value === '__unassigned__') {
    return list.filter(item => !item.project_id);
  }
  return list.filter(item => item.project_id === selectedProjectFilter.value);
});

const filteredItems = computed(() => {
  if (activeSubtab.value === 'timeline') return timelineItems.value;
  if (activeSubtab.value === 'todo') return filteredTodoItems.value;
  return [];
});

// Smart Week Calendar calculation
// If weekOffset === 0 and today's week has no events, anchor the initial week to the nearest scheduled event
// so canonical fixture events (like '跨平台架构评审会议') and upcoming events are immediately visible on the grid.
function getMondayOf(dateMs) {
  const d = new Date(Number(dateMs) || Date.now());
  const day = d.getDay();
  const diff = d.getDate() - day + (day === 0 ? -6 : 1);
  const mon = new Date(d);
  mon.setDate(diff);
  mon.setHours(0, 0, 0, 0);
  return mon;
}

const baseAnchorMonday = computed(() => getMondayOf(Date.now()));

const activeMonday = computed(() => {
  const mon = new Date(baseAnchorMonday.value);
  mon.setDate(mon.getDate() + weekOffset.value * 7);
  return mon;
});

const dayLabelKeys = ['dayMon', 'dayTue', 'dayWed', 'dayThu', 'dayFri', 'daySat', 'daySun'];

const weekDays = computed(() => {
  const mon = activeMonday.value;
  const todayStr = new Date().toDateString();
  const days = [];
  for (let i = 0; i < 7; i += 1) {
    const d = new Date(mon);
    d.setDate(mon.getDate() + i);
    const yyyy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, '0');
    const dd = String(d.getDate()).padStart(2, '0');
    days.push({
      index: i,
      labelKey: dayLabelKeys[i],
      dateNum: d.getDate(),
      monthNum: d.getMonth() + 1,
      dateStr: `${yyyy}-${mm}-${dd}`,
      isToday: d.toDateString() === todayStr,
      startMs: d.getTime(),
      endMs: d.getTime() + 86400000,
    });
  }
  return days;
});

const weekRangeLabel = computed(() => {
  const days = weekDays.value;
  if (!days.length) return '';
  const first = new Date(days[0].startMs);
  const last = new Date(days[6].startMs);
  const y1 = first.getFullYear();
  const m1 = first.getMonth() + 1;
  const d1 = first.getDate();
  const m2 = last.getMonth() + 1;
  const d2 = last.getDate();
  return `${y1}/${String(m1).padStart(2, '0')}/${String(d1).padStart(2, '0')} - ${String(m2).padStart(2, '0')}/${String(d2).padStart(2, '0')}`;
});

// Always render full 24 hours (00:00 - 23:00) so the timeline is complete without top/bottom gaps
const visibleHours = computed(() => Array.from({ length: 24 }, (_, i) => i));

const nowMinutes = ref(new Date().getHours() * 60 + new Date().getMinutes());
let nowTimer = null;
const currentTimeTopPercent = computed(() => (nowMinutes.value / (24 * 60)) * 100);

function getSlotEvents(dayObj, hour) {
  return timelineItems.value.filter(ev => {
    const ts = Number(ev.start_at || ev.due_at_ms || 0);
    if (!ts || ts < dayObj.startMs || ts >= dayObj.endMs) return false;
    return new Date(ts).getHours() === hour;
  });
}

async function openEditor(item = null, date = '') {
  editingItem.value = item; saveError.value = '';
  returnFocus = document.activeElement;
  const local = item ? new Date(Number(item.start_at || item.due_at_ms)) : null;
  const localDate = local ? new Date(local.getTime() - local.getTimezoneOffset() * 60000).toISOString().slice(0,16) : date;
  Object.assign(draft, {title:item?.title || '',kind:item?.kind || (activeSubtab.value === 'todo' ? 'todo':'event'),projectId:item?.project_id || '',dueDate:localDate,notes:item?.notes || ''});
  showCreateForm.value = true;
  await nextTick(); editor.value?.showModal();
}
function closeEditor() {
  if (saving.value) return;
  editor.value?.close(); showCreateForm.value = false; returnFocus?.focus?.();
}
function handleSlotClick(dayObj, hour) {
  openEditor(null, `${dayObj.dateStr}T${String(hour).padStart(2,'0')}:00`);
}

function getProjectName(projectId) {
  if (!projectId) return '';
  const found = allAvailableProjects.value.find(p => p.id === projectId);
  return found ? found.title : projectId;
}

function switchSubtab(tab) {
  activeSubtab.value = tab;
  if (tab === 'timeline') draft.kind = 'event';
  else if (tab === 'todo') draft.kind = 'todo';
}

async function submitNewSchedule() {
  if (saving.value || !draft.title.trim()) return;
  const startAt = draft.dueDate ? Date.parse(draft.dueDate) : Date.now();
  if (!Number.isFinite(startAt)) return;
  saving.value = true; saveError.value = '';
  try {
    if (!props.saveItem) throw new Error('UNAVAILABLE');
    await props.saveItem({id:editingItem.value?.id || `sched-${crypto.randomUUID()}`,
      expectedVersion:editingItem.value?.version ?? null,title:draft.title.trim(),kind:draft.kind,
      projectId:draft.projectId || null,startAt,notes:draft.notes.trim()});
    saving.value = false; closeEditor();
  } catch { saveError.value = t('scheduleSaveFailed'); }
  finally { saving.value = false; }
}

function submitInboxCapture() {
  const raw = inboxDraft.content.trim();
  if (!raw) return;
  const lines = raw.split(/\r?\n/);
  const title = lines[0].trim();
  const notes = lines.slice(1).join('\n').trim();
  emit('create-schedule', {
    id: `sched-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    title,
    kind: 'todo',
    projectId: inboxDraft.projectId || null,
    startAt: Date.now(),
    notes,
  });
  inboxDraft.content = '';
}

function handleInboxKeydown(event) {
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
    event.preventDefault();
    submitInboxCapture();
  }
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

function formatHour(h) {
  return `${String(h).padStart(2, '0')}:00`;
}

function isCompleted(item) {
  return item?.status === 'completed';
}

function toggleCompleted(item) {
  const nextStatus = isCompleted(item) ? 'scheduled' : 'completed';
  emit('update-status', { id: item.id, status: nextStatus });
}

function cancelItem(item) {
  emit('update-status', { id: item.id, status: 'cancelled' });
}

const gridBodyRef = ref(null);

function scrollToCurrentTime(smooth = false) {
  const container = gridBodyRef.value;
  if (!container) return;
  const days = weekDays.value;
  const hasToday = days.some(d => d.isToday);
  let targetHour = Math.max(0, new Date().getHours() - 2);

  if (!hasToday && days.length === 7) {
    const weekStart = days[0].startMs;
    const weekEnd = days[6].endMs;
    const weekEvents = timelineItems.value
      .map(ev => Number(ev.start_at || ev.due_at_ms || 0))
      .filter(ts => ts >= weekStart && ts < weekEnd)
      .sort((a, b) => a - b);
    if (weekEvents.length > 0) {
      const firstEventHour = new Date(weekEvents[0]).getHours();
      targetHour = Math.max(0, firstEventHour - 4);
    } else {
      targetHour = 8;
    }
  }

  const slotEl = container.querySelector('.hour-slot-cell');
  const slotHeight = slotEl ? slotEl.offsetHeight : 49.6;
  const maxScroll = Math.max(0, container.scrollHeight - container.clientHeight);
  const scrollY = Math.min(maxScroll, Math.max(0, targetHour * slotHeight));
  try {
    container.scrollTo({ top: scrollY, behavior: smooth ? 'smooth' : 'auto' });
  } catch {
    container.scrollTop = scrollY;
  }
}

onMounted(() => {
  nowTimer = setInterval(() => {
    const d = new Date();
    nowMinutes.value = d.getHours() * 60 + d.getMinutes();
  }, 60000);
  setTimeout(() => scrollToCurrentTime(false), 80);
});

onUnmounted(() => {
  if (nowTimer) clearInterval(nowTimer);
});

watch([activeSubtab, timelineViewMode, weekOffset], () => {
  if (activeSubtab.value === 'timeline' && timelineViewMode.value === 'grid') {
    setTimeout(() => scrollToCurrentTime(false), 60);
  }
});
</script>

<template>
  <section
    class="primary-view schedule-view"
    :class="{ 'is-timeline-subtab': activeSubtab === 'timeline' }"
    :aria-label="t('scheduleViewTitle')"
  >
    <header class="view-subtab-bar">
      <div class="view-subtabs" role="tablist">
        <button
          type="button"
          class="view-subtab-btn"
          :class="{ active: activeSubtab === 'timeline' }"
          @click="switchSubtab('timeline')"
        >
          <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
            <rect x="3" y="4" width="18" height="18" rx="2" />
            <path d="M16 2v4M8 2v4M3 10h18" />
          </svg>
          <span>{{ t('scheduleTabTimeline') }}</span>
          <span class="subtab-count">{{ timelineItems.length }}</span>
        </button>
        <button
          type="button"
          class="view-subtab-btn"
          :class="{ active: activeSubtab === 'todo' }"
          @click="switchSubtab('todo')"
        >
          <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
            <rect x="3" y="3" width="18" height="18" rx="2" />
            <path d="m9 12 2 2 4-4" />
          </svg>
          <span>{{ t('scheduleTabTodo') }}</span>
          <span class="subtab-count">{{ todoItems.length }}</span>
        </button>
        <button
          type="button"
          class="view-subtab-btn"
          :class="{ active: activeSubtab === 'cron' }"
          @click="switchSubtab('cron')"
        >
          <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="9" />
            <path d="M12 7v5l3 3" />
          </svg>
          <span>{{ t('scheduleTabCron') }}</span>
        </button>
      </div>

      <button
        type="button"
        class="compact-add-btn"
        :class="{ active: showCreateForm }"
        :style="{ visibility: activeSubtab === 'cron' ? 'hidden' : 'visible' }"
        @click="openEditor()"
      >
        + {{ t('scheduleToggleNewBtn') }}
      </button>
    </header>

    <div class="view-content-body">
      <!-- Collapsible Quick Schedule / Event Form -->
      <dialog ref="editor" class="schedule-editor" aria-labelledby="schedule-editor-title" @cancel.prevent="closeEditor" @click="event => { if (event.target === editor) closeEditor(); }">
      <form v-if="showCreateForm"
        class="quick-schedule-form hub-card"
        @submit.prevent="submitNewSchedule"
      >
        <header class="schedule-editor-header"><h3 id="schedule-editor-title">{{ t(editingItem ? 'scheduleEditTitle' : 'scheduleToggleNewBtn') }}</h3><button type="button" :disabled="saving" @click="closeEditor">{{ t('scheduleClose') }}</button></header>
        <p v-if="saveError" role="alert">{{ saveError }}</p>
        <fieldset :disabled="saving" class="schedule-editor-fields">
        <div class="quick-schedule-row">
          <CustomSelect
            v-model="draft.kind"
            :disabled="!!editingItem"
            :options="scheduleKindOptions"
            class="schedule-kind-select"
          />
          <input
            v-model="draft.title"
            type="text"
            class="schedule-title-input"
            :placeholder="t('scheduleNewTitlePlaceholder')"
            :aria-label="t('scheduleNewTitlePlaceholder')"
            autofocus
            required
          />
          <CustomSelect
            v-model="draft.projectId"
            :disabled="!!editingItem"
            :options="projectSelectOptions"
            class="schedule-project-select"
          />
          <input
            v-model="draft.dueDate"
            type="datetime-local"
            class="schedule-date-input"
            :aria-label="t('scheduleDueLabel')"
          />
          <button
            type="submit"
            class="primary-action-btn"
            :disabled="busy || saving || !draft.title.trim()"
          >
            {{ t(editingItem ? 'scheduleSaveChanges' : 'scheduleAddBtn') }}
          </button>
        </div>
        <input
          v-model="draft.notes"
          type="text"
          class="schedule-notes-input"
          :placeholder="t('scheduleNotesPlaceholder')"
        />
        </fieldset>
      </form>
      </dialog>

      <!-- SUBTAB 1: WEEK TIMELINE & CALENDAR GRID VIEW -->
      <template v-if="activeSubtab === 'timeline'">
        <section class="week-calendar-card hub-card">
          <div class="week-calendar-toolbar">
            <strong class="week-range-label" :title="weekRangeLabel"><span class="week-range-full">{{ weekRangeLabel }}</span><span class="week-range-short">{{ weekRangeLabel.slice(5).replaceAll('/', '.') }}</span></strong>
            <div class="week-nav-btns">
              <button type="button" class="calendar-icon-btn" :title="t('calendarPrevWeek')" :aria-label="t('calendarPrevWeek')" @click="weekOffset -= 1"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m14 6-6 6 6 6" /></svg></button>
              <button type="button" class="calendar-icon-btn" :class="{ active: weekOffset === 0 }" :title="t('calendarThisWeek')" :aria-label="t('calendarThisWeek')" @click="weekOffset = 0"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="7"/></svg></button>
              <button type="button" class="calendar-icon-btn" :title="t('calendarNextWeek')" :aria-label="t('calendarNextWeek')" @click="weekOffset += 1"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m10 6 6 6-6 6" /></svg></button>
            </div>
            <div class="week-calendar-mode-btns">
              <button type="button" class="calendar-icon-btn" :class="{ active: timelineViewMode === 'grid' }" :aria-pressed="timelineViewMode === 'grid'" :title="t('calendarViewGrid')" :aria-label="t('calendarViewGrid')" @click="timelineViewMode = 'grid'"><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="5" width="18" height="16" rx="2"/><path d="M7 3v4m10-4v4M3 11h18m-12 0v10m6-10v10"/></svg></button>
              <button type="button" class="calendar-icon-btn" :class="{ active: timelineViewMode === 'list' }" :aria-pressed="timelineViewMode === 'list'" :title="t('calendarViewList')" :aria-label="t('calendarViewList')" @click="timelineViewMode = 'list'"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6h12M9 12h12M9 18h12M3 6h1m-1 6h1m-1 6h1"/></svg></button>
            </div>
          </div>

          <!-- 7-Day 24-Hour Week Calendar Grid (Inherited from legacy WeekTimeline.vue) -->
          <div v-if="timelineViewMode === 'grid'" class="week-calendar-grid-wrapper">
            <div class="week-grid-header">
              <div class="time-col-corner"></div>
              <div class="week-grid-header-days">
                <div
                  v-for="day in weekDays"
                  :key="day.dateStr"
                  class="day-col-header"
                  :class="{ today: day.isToday }"
                >
                  <span class="day-name">{{ t(day.labelKey) }}</span>
                  <span class="day-num">{{ day.dateNum }}</span>
                </div>
              </div>
            </div>

            <div class="week-grid-body" ref="gridBodyRef">
              <div class="time-axis-col">
                <div v-for="h in visibleHours" :key="h" class="time-axis-cell">
                  <span :aria-label="`${formatHour(h)}–${h === 23 ? '24:00' : formatHour(h + 1)}`">{{ h === 0 ? 24 : h }}</span>
                </div>
              </div>

              <div class="days-columns-grid">
                <div
                  v-for="day in weekDays"
                  :key="day.dateStr"
                  class="day-column"
                  :class="{ today: day.isToday }"
                >
                  <div
                    v-if="day.isToday"
                    class="current-time-line"
                    :style="{ top: `${currentTimeTopPercent}%` }"
                  >
                    <span class="current-time-dot"></span>
                  </div>
                  <div
                    v-for="h in visibleHours"
                    :key="h"
                    class="hour-slot-cell"
                    :title="t('calendarSlotAddTitle')"
                    @click="handleSlotClick(day, h)"
                  >
                    <div
                      v-for="ev in getSlotEvents(day, h)"
                      :key="ev.id"
                      class="calendar-event-block"
                      :class="{ done: isCompleted(ev), cancelled: ev.status === 'cancelled' }"
                      role="button" tabindex="0"
                      @keydown.enter.stop="openEditor(ev)" @keydown.space.prevent.stop="openEditor(ev)"
                      @click.stop="openEditor(ev)"
                    >
                      <span class="calendar-event-time">{{ new Date(Number(ev.start_at || ev.due_at_ms)).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) }}</span>
                      <strong class="calendar-event-title">{{ ev.title }}</strong>
                      <span v-if="ev.project_id" class="calendar-event-proj">
                        {{ getProjectName(ev.project_id) }}
                      </span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <!-- Scheduled Events Summary List (Rendered inside .week-calendar-card for zero layout shift) -->
          <div v-else class="week-calendar-list-wrapper">
            <div v-if="!timelineItems.length" class="view-empty-card">
              <p>{{ t('scheduleEmptyHint') }}</p>
            </div>
            <div v-else class="schedule-items-list">
              <article
                v-for="item in timelineItems"
                :key="item.id"
                class="schedule-item-card hub-card"
                :class="{ done: isCompleted(item), cancelled: item.status === 'cancelled' }"
              >
                <div class="schedule-item-main">
                  <button
                    type="button"
                    class="schedule-check-btn"
                    :class="{ checked: isCompleted(item) }"
                    :aria-label="isCompleted(item) ? t('scheduleReopenBtn') : t('scheduleMarkDoneBtn')"
                    @click="toggleCompleted(item)"
                  >
                    <svg v-if="isCompleted(item)" class="check-svg" viewBox="0 0 24 24" aria-hidden="true">
                      <path d="M20 6 9 17l-5-5" />
                    </svg>
                  </button>
                  <div class="schedule-item-text">
                    <div class="schedule-item-title-row">
                      <button type="button" class="schedule-detail-link" @click="openEditor(item)">{{ item.title }}</button>
                      <span v-if="item.project_id" class="project-bound-badge">
                        {{ getProjectName(item.project_id) }}
                      </span>
                      <span class="schedule-kind-badge">
                        {{ t('scheduleKindEvent') }}
                      </span>
                      <span class="schedule-status-badge" :class="item.status">
                        {{
                          t(
                            isCompleted(item)
                              ? 'scheduleStatusCompleted'
                              : item.status === 'cancelled'
                                ? 'scheduleStatusCancelled'
                                : 'scheduleStatusScheduled',
                          )
                        }}
                      </span>
                    </div>
                    <p v-if="item.notes" class="schedule-item-notes">{{ item.notes }}</p>
                    <span v-if="item.start_at || item.due_at_ms" class="schedule-item-due">
                      <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
                        <circle cx="12" cy="12" r="9" />
                        <path d="M12 7v5l3 2" />
                      </svg>
                      {{ formatDue(item.start_at || item.due_at_ms) }}
                    </span>
                  </div>
                </div>
                <div class="schedule-item-actions">
                  <button
                    v-if="item.project_id"
                    type="button"
                    class="mini-ghost-btn"
                    @click="emit('open-project-todo', item)"
                  >
                    {{ t('todoOpenInProjectBtn') }}
                  </button>
                  <button type="button" class="mini-ghost-btn" @click="toggleCompleted(item)">
                    {{ isCompleted(item) ? t('scheduleReopenBtn') : t('scheduleMarkDoneBtn') }}
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
          </div>
        </section>
      </template>

      <!-- SUBTAB 2: INSPIRATION & PROJECT TODO INBOX (dashboard.bobbik.org/#inbox Closed Loop) -->
      <template v-else-if="activeSubtab === 'todo'">
        <form class="inbox-capture-card hub-card" @submit.prevent="submitInboxCapture">
          <div class="inbox-capture-header">
            <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4" />
            </svg>
            <strong>{{ t('inboxCaptureTitle') }}</strong>
          </div>
          <textarea
            v-model="inboxDraft.content"
            class="inbox-capture-textarea"
            rows="2"
            :placeholder="t('inboxCapturePlaceholder')"
            @keydown="handleInboxKeydown"
          ></textarea>
          <div class="inbox-capture-toolbar">
            <div class="inbox-project-picker">
              <span class="inbox-picker-label">{{ t('inboxProjectSelectLabel') }}</span>
              <CustomSelect
                v-model="inboxDraft.projectId"
                :options="projectSelectOptions"
                class="inbox-project-select"
              />
            </div>
            <div class="inbox-submit-group">
              <span class="inbox-shortcut-hint">{{ t('inboxShortcutHint') }}</span>
              <button
                type="submit"
                class="primary-action-btn"
                :disabled="busy || !inboxDraft.content.trim()"
              >
                {{ t('inboxSubmitTodoBtn') }}
              </button>
            </div>
          </div>
        </form>

        <!-- Project Filter Strip -->
        <div v-if="allAvailableProjects.length > 0" class="todo-project-filter-bar">
          <button
            type="button"
            class="node-status-chip"
            :class="{ active: selectedProjectFilter === '__all__' }"
            @click="selectedProjectFilter = '__all__'"
          >
            {{ t('todoFilterAll') }} ({{ todoItems.length }})
          </button>
          <button
            type="button"
            class="node-status-chip"
            :class="{ active: selectedProjectFilter === '__unassigned__' }"
            @click="selectedProjectFilter = '__unassigned__'"
          >
            {{ t('todoFilterUnassigned') }} ({{ todoItems.filter(i => !i.project_id).length }})
          </button>
          <button
            v-for="proj in allAvailableProjects"
            :key="proj.id"
            type="button"
            class="node-status-chip"
            :class="{ active: selectedProjectFilter === proj.id }"
            @click="selectedProjectFilter = proj.id"
          >
            {{ proj.title }} ({{ todoItems.filter(i => i.project_id === proj.id).length }})
          </button>
        </div>

        <div v-if="!filteredTodoItems.length" class="view-empty-card hub-card">
          <p>{{ t('scheduleEmptyTodo') }}</p>
        </div>

        <div v-else class="schedule-items-list">
          <article
            v-for="item in filteredTodoItems"
            :key="item.id"
            class="schedule-item-card hub-card"
            :class="{ done: isCompleted(item), cancelled: item.status === 'cancelled' }"
          >
            <div class="schedule-item-main">
              <button
                type="button"
                class="schedule-check-btn"
                :class="{ checked: isCompleted(item) }"
                :aria-label="isCompleted(item) ? t('scheduleReopenBtn') : t('scheduleMarkDoneBtn')"
                @click="toggleCompleted(item)"
              >
                <svg v-if="isCompleted(item)" class="check-svg" viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M20 6 9 17l-5-5" />
                </svg>
              </button>
              <div class="schedule-item-text">
                <div class="schedule-item-title-row">
                  <button type="button" class="schedule-detail-link" @click="openEditor(item)">{{ item.title }}</button>
                  <span v-if="item.project_id" class="project-bound-badge">
                    {{ getProjectName(item.project_id) }}
                  </span>
                  <span v-else class="schedule-kind-badge">
                    {{ t('todoFilterUnassigned') }}
                  </span>
                  <span class="schedule-status-badge" :class="item.status">
                    {{
                      t(
                        isCompleted(item)
                          ? 'scheduleStatusCompleted'
                          : item.status === 'cancelled'
                            ? 'scheduleStatusCancelled'
                            : 'scheduleStatusScheduled',
                      )
                    }}
                  </span>
                </div>
                <p v-if="item.notes" class="schedule-item-notes">{{ item.notes }}</p>
                <span v-if="item.start_at || item.due_at_ms" class="schedule-item-due">
                  <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
                    <circle cx="12" cy="12" r="9" />
                    <path d="M12 7v5l3 2" />
                  </svg>
                  {{ formatDue(item.start_at || item.due_at_ms) }}
                </span>
              </div>
            </div>
            <div class="schedule-item-actions">
              <button
                v-if="item.project_id"
                type="button"
                class="primary-action-btn"
                @click="emit('open-project-todo', item)"
              >
                {{ t('todoOpenInProjectBtn') }}
              </button>
              <button
                v-else
                type="button"
                class="mini-ghost-btn"
                @click="emit('promote-todo-project', item)"
              >
                {{ t('todoPromoteToProjectBtn') }}
              </button>
              <button type="button" class="mini-ghost-btn" @click="toggleCompleted(item)">
                {{ isCompleted(item) ? t('scheduleReopenBtn') : t('scheduleMarkDoneBtn') }}
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

      <!-- SUBTAB 3: CRON & DAILY ROUTINES -->
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
