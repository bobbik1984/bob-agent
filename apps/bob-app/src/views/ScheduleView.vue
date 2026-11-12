<script setup>
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import CustomSelect from '../components/CustomSelect.vue';
import { calendarEntries, entryDay } from '../todo-calendar.mjs';
import { calendarPatch, timedDayLayout } from '../calendar-gesture.mjs';
import TicketWalletPanel from '../components/TicketWalletPanel.vue';

const props = defineProps({
  saveItem: { type: Function, default: null },
  schedules: {
    type: Array,
    default: () => [],
  },
  notes: {
    type: Array,
    default: () => [],
  },
  recognizingTicket: {
    type: Boolean,
    default: false,
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
  'delete-schedule',
  'open-project-todo',
  'promote-todo-project',
  'save-ticket',
  'delete-ticket',
  'recognize-ticket-file',
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

const unassignedTodoCount = computed(() =>
  todoItems.value.filter(i => !i.project_id).length
);

const activeProjectsWithTodos = computed(() => {
  const counts = new Map();
  for (const item of todoItems.value) {
    if (item.project_id) {
      counts.set(item.project_id, (counts.get(item.project_id) || 0) + 1);
    }
  }
  const result = [];
  for (const [id, count] of counts.entries()) {
    const p = allAvailableProjects.value.find(proj => proj.id === id);
    result.push({
      id,
      title: p ? p.title : id,
      count,
    });
  }
  return result;
});

const projectSelectOptions = computed(() => [
  { value: '', label: t('todoNoProject') },
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
  deadline: '',
  startDateOnly: false, dueDateOnly: false,
  notes: '',
});

const inboxDraft = reactive({
  content: '',
  projectId: '',
});

const timelineItems = computed(() =>
  (props.schedules || []).filter(item => calendarEntries(item).length),
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
      endMs: new Date(d.getFullYear(), d.getMonth(), d.getDate()+1).getTime(),
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

const gesture = ref(null), calendarSaving=ref(false), calendarError=ref('');
let pointerDrag=null, suppressClickUntil=0;
const entries = computed(() => timelineItems.value.flatMap(item=>{
  const patch=gesture.value?.patch;
  if(patch?.id===item.id)item={...item,start_at:patch.startAt,end_at:patch.endAt,timing:patch.timing};
  if(item.start_at && item.end_at && !item.timing?.start_date && !item.timing?.due_date)
    return [{...item,key:item.id,at:Number(item.start_at),until:Number(item.end_at),boundary:item.kind==='event'?'event':'range',original:item}];
  return calendarEntries(item);
}));
function timedItems(day){return timedDayLayout(entries.value,day.startMs,day.endMs);}
function blockStyle(ev){return {top:`${ev.top}%`,height:`${ev.height}%`,left:`calc(${ev.lane/ev.lanes*100}% + 2px)`,width:`calc(${100/ev.lanes}% - 4px)`};}
function eventDescription(ev){return `${ev.title} · ${boundaryLabel(ev)} · ${ev.date || formatDue(ev.at)}${ev.until?' – '+formatDue(ev.until):''}`;}
function eventClick(ev){if(Date.now()>suppressClickUntil && !calendarSaving.value)openEditor(ev);}
function startGesture(e,ev,mode='move'){
  if(e.button!==0 || calendarSaving.value || props.busy)return;
  calendarError.value='';
  const columns=[...gridBodyRef.value.querySelectorAll('.day-column')];
  const column=columns.find(c=>c.dataset.date===entryDay(ev)) || e.currentTarget.closest('.day-column');
  const originIndex=columns.indexOf(column);
  pointerDrag={ev,mode,x:e.clientX,y:e.clientY,originIndex,columns,scroll:gridBodyRef.value.scrollTop,moved:false,pointer:e.pointerId};
  e.currentTarget.setPointerCapture(e.pointerId);
}
function moveGesture(e){
  const d=pointerDrag;if(!d || e.pointerId!==d.pointer)return;
  if(!d.moved && Math.hypot(e.clientX-d.x,e.clientY-d.y)<6)return;
  d.moved=true;e.preventDefault();
  const grid=gridBodyRef.value,rect=grid.getBoundingClientRect();
  if(!d.ev.date){if(e.clientY<rect.top+28)grid.scrollTop-=14;else if(e.clientY>rect.bottom-28)grid.scrollTop+=14;}
  const index=d.columns.findIndex(c=>{const b=c.getBoundingClientRect();return e.clientX>=b.left && e.clientX<=b.right;});
  if(index<0)return;
  const minutes=d.ev.date?0:Math.round(((e.clientY-d.y+grid.scrollTop-d.scroll)/d.columns[0].offsetHeight*1440)/15)*15;
  try{gesture.value={patch:calendarPatch(d.ev,d.mode,index-d.originIndex,minutes),valid:true};}
  catch{gesture.value={valid:false};}
}
function cancelGesture(){pointerDrag=null;gesture.value=null;suppressClickUntil=Date.now()+300;}
async function finishGesture(e){
  const d=pointerDrag;if(!d || e.pointerId!==d.pointer)return;
  pointerDrag=null;if(!d.moved)return;
  suppressClickUntil=Date.now()+400;
  if(!gesture.value?.valid){calendarError.value=t('scheduleInvalidDates');gesture.value=null;return;}
  const patch=gesture.value.patch;calendarSaving.value=true;
  try{if(!props.saveItem)throw new Error('UNAVAILABLE');await props.saveItem(patch);}
  catch{calendarError.value=t('scheduleSaveFailed');}
  finally{calendarSaving.value=false;gesture.value=null;}
}
function gestureKey(e){if(e.key==='Escape' && pointerDrag)cancelGesture();}
function allDayItems(day) {return entries.value.filter(ev=>ev.date===day.dateStr);}
function boundaryLabel(ev) {return t(ev.boundary==='due'?'todoDueLabel':ev.boundary==='start'?'todoStartLabel':'scheduleKindEvent');}
function dateValue(item,boundary) {return item.timing?.[`${boundary}_date`] || formatDue(boundary==='start'?item.start_at:item.end_at);}
function changePrecision(boundary) {
  const field=boundary==='start'?'dueDate':'deadline';
  const dateOnly=boundary==='start'?draft.startDateOnly:draft.dueDateOnly;
  if(dateOnly) draft[field]=draft[field].slice(0,10);
  else if(draft[field] && draft[field].length===10) draft[field]='';
}

async function openEditor(item = null, date = '') {
  item = item?.original || item;
  editingItem.value = item; saveError.value = '';
  returnFocus = document.activeElement;
  const asLocal = ms => ms ? new Date(Number(ms) - new Date(Number(ms)).getTimezoneOffset()*60000).toISOString().slice(0,16) : '';
  Object.assign(draft, {title:item?.title || '',kind:item?.kind || (activeSubtab.value === 'todo' ? 'todo':'event'),projectId:item?.project_id || '',dueDate:item ? (item.timing?.start_date || asLocal(item.start_at)) : date,deadline:item?.timing?.due_date || asLocal(item?.end_at),startDateOnly:!!item?.timing?.start_date,dueDateOnly:!!item?.timing?.due_date,notes:item?.notes || ''});
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
  const startAt = draft.dueDate && !(draft.kind==='todo' && draft.startDateOnly) ? Date.parse(draft.dueDate) : null;
  const endAt = draft.deadline && !draft.dueDateOnly ? Date.parse(draft.deadline) : null;
  if ((startAt !== null && !Number.isFinite(startAt)) || (endAt !== null && !Number.isFinite(endAt))
      || (draft.kind === 'event' && startAt === null) || (startAt !== null && endAt !== null && endAt < startAt)) {
    saveError.value = t('scheduleInvalidDates'); return;
  }
  saving.value = true; saveError.value = '';
  try {
    if (!props.saveItem) throw new Error('UNAVAILABLE');
    await props.saveItem({id:editingItem.value?.id || `sched-${crypto.randomUUID()}`,
      expectedVersion:editingItem.value?.version ?? null,title:draft.title.trim(),kind:draft.kind,
      projectId:draft.projectId || null,startAt,endAt,notes:draft.notes.trim(),
      timing:draft.kind==='todo' ? {...(editingItem.value?.timing || {}),
        start_date:draft.startDateOnly && draft.dueDate ? draft.dueDate : null,
        due_date:draft.dueDateOnly && draft.deadline ? draft.deadline : null} : {}});
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
    startAt: null,
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

function deleteItem(item) {
  if (!item?.id) return;
  emit('delete-schedule', item.id);
  if (editingItem.value?.id === item.id) {
    closeEditor();
  }
}

const ticketCount = computed(() =>
  (props.notes || []).filter(n =>
    (n.tags || []).some(tag => tag === 'ticket' || String(tag).startsWith('ticket:'))
  ).length,
);

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
  window.addEventListener('pointermove',moveGesture,{passive:false});
  window.addEventListener('pointerup',finishGesture);
  window.addEventListener('pointercancel',cancelGesture);
  window.addEventListener('keydown',gestureKey);
  nowTimer = setInterval(() => {
    const d = new Date();
    nowMinutes.value = d.getHours() * 60 + d.getMinutes();
  }, 60000);
  setTimeout(() => scrollToCurrentTime(false), 80);
});

onUnmounted(() => {
  if (nowTimer) clearInterval(nowTimer);
  window.removeEventListener('pointermove',moveGesture);window.removeEventListener('pointerup',finishGesture);window.removeEventListener('pointercancel',cancelGesture);window.removeEventListener('keydown',gestureKey);
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
        <header class="schedule-editor-header"><h3 id="schedule-editor-title">{{ t(editingItem ? 'scheduleEditTitle' : 'scheduleToggleNewBtn') }}</h3><div style="display:flex;gap:8px;"><button v-if="editingItem" type="button" class="mini-ghost-btn" :disabled="saving" @click="deleteItem(editingItem)">{{ t('scheduleDeleteBtn') }}</button><button type="button" :disabled="saving" @click="closeEditor">{{ t('scheduleClose') }}</button></div></header>
        <p v-if="saveError" role="alert">{{ saveError }}</p>
        <p v-if="editingItem?.timing?.inference_note" class="schedule-time-context">{{ editingItem.timing.inference_note }}</p>
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
            :disabled="!!editingItem && draft.kind !== 'todo'"
            :options="projectSelectOptions"
            class="schedule-project-select"
          />
          <label v-if="draft.kind === 'todo'"><input type="checkbox" v-model="draft.startDateOnly" @change="changePrecision('start')" />{{ t('todoStartDateOnly') }}</label>
          <label class="schedule-time-field">
            <span>{{ t(draft.kind === 'todo' ? 'todoStartOptional' : 'scheduleDueLabel') }}</span>
            <input v-model="draft.dueDate" :type="draft.kind === 'todo' && draft.startDateOnly ? 'date' : 'datetime-local'" class="schedule-date-input" :required="draft.kind === 'event'" :aria-label="t(draft.kind === 'todo' ? 'todoStartOptional' : 'scheduleDueLabel')" />
          </label>
          <label v-if="draft.kind === 'todo'"><input type="checkbox" v-model="draft.dueDateOnly" @change="changePrecision('due')" />{{ t('todoDueDateOnly') }}</label>
          <label class="schedule-time-field">
            <span>{{ t(draft.kind==='todo'?'todoDueOptional':'calendarEndOptional') }}</span>
            <input v-model="draft.deadline" :type="draft.dueDateOnly ? 'date' : 'datetime-local'" class="schedule-date-input" :aria-label="t(draft.kind==='todo'?'todoDueOptional':'calendarEndOptional')" />
          </label>
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

          <p v-if="calendarError" role="alert">{{ calendarError }}</p>
          <p v-if="calendarSaving" class="calendar-saving-status" role="status">{{ t('calendarSaving') }}</p>
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

            <div class="todo-all-day-row">
              <span>{{ t('todoAllDay') }}</span>
              <div v-for="day in weekDays" :key="day.dateStr" class="todo-all-day-cell">
                <button v-for="ev in allDayItems(day)" :key="ev.key" type="button" class="todo-calendar-marker" :class="{ done:isCompleted(ev) }" @pointerdown.stop="startGesture($event,ev)" @click="eventClick(ev)" :title="eventDescription(ev)" :aria-label="eventDescription(ev)">
                  <strong>{{ ev.title }}</strong>
                </button>
              </div>
            </div>
            <div class="week-grid-body" ref="gridBodyRef">
              <div class="time-axis-col">
                <div v-for="h in visibleHours" :key="h" class="time-axis-cell">
                  <span :aria-label="`${formatHour(h)}–${h === 23 ? '24:00' : formatHour(h + 1)}`">{{ h }}</span>
                </div>
              </div>

              <div class="days-columns-grid">
                <div
                  v-for="day in weekDays"
                  :key="day.dateStr"
                  class="day-column" :data-date="day.dateStr"
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
                  </div>
                  <div v-for="ev in timedItems(day)" :key="ev.key" class="calendar-event-block timed-event"
                    :class="{done:isCompleted(ev),cancelled:ev.status==='cancelled',dragging:gesture?.patch?.id===ev.id}"
                    :style="blockStyle(ev)" role="button" tabindex="0" :title="eventDescription(ev)" :aria-label="eventDescription(ev)"
                    @pointerdown.stop="startGesture($event,ev)" @click.stop="eventClick(ev)"
                    @keydown.enter.stop="eventClick(ev)" @keydown.space.prevent.stop="eventClick(ev)">
                    <strong class="calendar-event-title">{{ ev.title }}</strong>
                    <span v-if="(ev.boundary==='event'||ev.boundary==='range') && !ev.clipStart" class="calendar-resize-handle resize-start"
                      role="button" tabindex="0" :aria-label="t('calendarResizeStart')" :title="t('calendarResizeStart')"
                      @pointerdown.stop="startGesture($event,ev,'start')" @click.stop @keydown.enter.stop="openEditor(ev)"></span>
                    <span v-if="(ev.boundary==='event'||ev.boundary==='range') && !ev.clipEnd" class="calendar-resize-handle resize-end"
                      role="button" tabindex="0" :aria-label="t('calendarResizeEnd')" :title="t('calendarResizeEnd')"
                      @pointerdown.stop="startGesture($event,ev,'end')" @click.stop @keydown.enter.stop="openEditor(ev)"></span>
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
                        {{ t(item.kind === 'todo' ? 'scheduleKindTodo' : 'scheduleKindEvent') }}
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
                    <span v-if="dateValue(item, 'start') || dateValue(item, 'due')" class="schedule-item-due">
                      <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
                        <circle cx="12" cy="12" r="9" />
                        <path d="M12 7v5l3 2" />
                      </svg>
                      {{ dateValue(item, 'start') }} {{ dateValue(item, 'due') ? t('todoDueLabel') + ': ' + dateValue(item, 'due') : '' }}
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
        <!-- Project Filter Strip (Only show when multiple project categories exist, and only for projects with todos) -->
        <div v-if="(activeProjectsWithTodos.length + (unassignedTodoCount > 0 ? 1 : 0)) > 1" class="todo-project-filter-bar">
          <button
            type="button"
            class="node-status-chip"
            :class="{ active: selectedProjectFilter === '__all__' }"
            @click="selectedProjectFilter = '__all__'"
          >
            {{ t('todoFilterAll') }} ({{ todoItems.length }})
          </button>
          <button
            v-if="unassignedTodoCount > 0"
            type="button"
            class="node-status-chip"
            :class="{ active: selectedProjectFilter === '__unassigned__' }"
            @click="selectedProjectFilter = '__unassigned__'"
          >
            {{ t('todoFilterUnassigned') }} ({{ unassignedTodoCount }})
          </button>
          <button
            v-for="proj in activeProjectsWithTodos"
            :key="proj.id"
            type="button"
            class="node-status-chip"
            :class="{ active: selectedProjectFilter === proj.id }"
            @click="selectedProjectFilter = proj.id"
          >
            {{ proj.title }} ({{ proj.count }})
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
                <span v-if="dateValue(item, 'start')" class="schedule-item-due">{{ t('todoStartOptional') }}: {{ dateValue(item, 'start') }}</span>
                <span v-if="dateValue(item, 'due')" class="schedule-item-due">{{ t('todoDueOptional') }}: {{ dateValue(item, 'due') }}</span>

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
              <button
                type="button"
                class="mini-ghost-btn"
                @click="deleteItem(item)"
              >
                {{ t('scheduleDeleteBtn') }}
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

<style scoped>
.todo-all-day-row {display:grid;grid-template-columns:2.4rem repeat(7,minmax(0,1fr));border-bottom:1px solid var(--line);flex-shrink:0;max-height:12rem;overflow-y:auto;scrollbar-gutter:stable;}
.todo-all-day-row > span {font-size:.7rem;color:var(--text-secondary);padding:4px;}
.todo-all-day-cell {min-width:0;padding:3px;border-left:1px solid var(--line);}
.todo-calendar-marker {display:block;width:100%;text-align:left;overflow-wrap:anywhere;font-size:.75rem;padding:4px;margin-bottom:3px;background:var(--bg-secondary);color:var(--text-primary);border:1px solid var(--accent-primary);border-radius:var(--radius-btn);}
.todo-calendar-marker small,.schedule-time-context {color:var(--text-secondary);}
.todo-calendar-marker.done {opacity:.6;}
.week-calendar-card {position:relative;}
.calendar-saving-status {position:absolute;top:0;left:50%;transform:translateX(-50%);z-index:10;background:var(--bg-primary);color:var(--text-secondary);padding:3px 8px;border-radius:var(--radius-sm);font-size:.75rem;}
/* Time is encoded by position; titles are always the primary content. */
.timed-event {position:absolute;z-index:6;min-height:2rem;padding:7px 5px;cursor:grab;touch-action:none;user-select:none;box-sizing:border-box;background:var(--bg-secondary);border-color:var(--accent-primary);border-radius:4px;}
.timed-event .calendar-event-title {white-space:normal;overflow-wrap:anywhere;font-size:.8rem;line-height:1.25;font-weight:600;display:-webkit-box;-webkit-line-clamp:3;-webkit-box-orient:vertical;}
.timed-event.dragging {z-index:8;opacity:.8;cursor:grabbing;}
.calendar-resize-handle {position:absolute;left:0;right:0;height:8px;cursor:ns-resize;touch-action:none;}
.resize-start {top:0;}.resize-end {bottom:0;}
.calendar-resize-handle:hover,.calendar-resize-handle:focus-visible {background:var(--accent-soft);outline:1px solid var(--accent-primary);}
.todo-calendar-marker {touch-action:none;cursor:grab;border-radius:4px;}
.todo-calendar-marker strong {font-weight:600;}
</style>
