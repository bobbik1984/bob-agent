<template>
  <div class="week-timeline">
    <!-- ========== 横向模式（宽屏） ========== -->
    <template v-if="!isNarrow">
      <div class="timeline-header">
        <div class="time-axis">
          <span
            v-for="h in majorHours"
            :key="h"
            class="tick-label"
            :style="{ left: getHourPct(h) + '%' }"
          >{{ h === 24 ? 0 : h }}</span>
        </div>
      </div>
      <div class="timeline-body">
        <div v-for="day in days" :key="day.dateStr" class="day-row">
          <div class="day-label" :class="{ today: day.isToday }">
            <span class="day-name">{{ day.name }}</span>
            <span class="day-date">{{ day.dateLabel }}</span>
          </div>
          <div class="day-track" ref="trackRefs">
            <!-- 网格线 -->
            <div
              v-for="h in allHours"
              :key="'g-'+h"
              class="track-grid-line"
              :class="{ major: majorHours.includes(h) }"
              :style="{ left: getHourPct(h) + '%' }"
            ></div>
            <!-- 事件块 -->
            <div
              v-for="event in day.events"
              :key="event.id"
              class="event-block"
              :style="{
                left: getEventLeftPct(event) + '%',
                width: getEventWidthPct(event) + '%'
              }"
              :title="event.title + '\n' + formatTimeRange(event)"
              @mousedown.prevent="startDragMove($event, event, day)"
              @click.stop="openDetail(event)"
            >
              <div class="resize-handle left" @mousedown.stop.prevent="startResize($event, event, day, 'left')"></div>
              <span class="event-title">{{ event.title }}</span>
              <div class="resize-handle right" @mousedown.stop.prevent="startResize($event, event, day, 'right')"></div>
            </div>
          </div>
        </div>
      </div>
    </template>

    <!-- ========== 竖向模式（窄屏） ========== -->
    <template v-else>
      <div class="vertical-timeline">
        <div v-for="day in days" :key="day.dateStr" class="v-day-group">
          <div class="v-day-header" :class="{ today: day.isToday }">
            {{ day.name }} · {{ day.dateLabel }}
          </div>
          <div v-if="day.events.length === 0" class="v-empty">暂无日程</div>
          <div
            v-for="event in day.events"
            :key="event.id"
            class="v-event-item"
            @click="openDetail(event)"
          >
            <span class="v-event-time">{{ formatTimeRange(event) }}</span>
            <span class="v-event-title">{{ event.title }}</span>
          </div>
        </div>
      </div>
    </template>

    <!-- ========== 事件详情弹窗 ========== -->
    <div v-if="detailEvent" class="detail-overlay" @click.self="detailEvent = null">
      <div class="detail-card">
        <h3 class="detail-title">{{ detailEvent.title }}</h3>
        <div class="detail-field">
          <label>时间</label>
          <span>{{ formatTimeRange(detailEvent) }}</span>
        </div>
        <div v-if="detailEvent.location" class="detail-field">
          <label>地点</label>
          <span>{{ detailEvent.location }}</span>
        </div>
        <div v-if="detailEvent.notes" class="detail-field">
          <label>备注</label>
          <span>{{ detailEvent.notes }}</span>
        </div>
        <div class="detail-actions">
          <button class="btn btn-ghost" @click="detailEvent = null">关闭</button>
          <button class="btn btn-danger" @click="handleDelete(detailEvent)">删除</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, reactive, onMounted, onUnmounted } from 'vue';

const props = defineProps({
  weekEvents: { type: Array, default: () => [] }
});

const emit = defineEmits(['update-time', 'delete-event']);

// ── 响应式宽度检测 ────────────────────────────────
const isNarrow = ref(false);
function checkWidth() {
  isNarrow.value = window.innerWidth < 700;
}
onMounted(() => {
  checkWidth();
  window.addEventListener('resize', checkWidth);
});
onUnmounted(() => {
  window.removeEventListener('resize', checkWidth);
});

// ── 时间轴常量 ───────────────────────────────────
const allHours = Array.from({ length: 25 }, (_, i) => i);
const majorHours = [0, 6, 12, 18, 24];
const startHour = 0;
const totalHours = 24;

const getHourPct = (h) => ((h - startHour) / totalHours) * 100;

// ── 事件详情弹窗 ──────────────────────────────────
const detailEvent = ref(null);
let clickTimer = null;

function openDetail(event) {
  // 拖拽后不打开
  if (dragState.moved) return;
  detailEvent.value = event;
}

async function handleDelete(event) {
  try {
    await window.electronAPI.deleteEvent(event.id);
    emit('delete-event', event.id);
    // 从本地数据中移除
    const idx = props.weekEvents.findIndex(e => e.id === event.id);
    if (idx !== -1) props.weekEvents.splice(idx, 1);
    detailEvent.value = null;
  } catch (err) {
    console.error('[WeekTimeline] 删除失败', err);
  }
}

// ── 拖拽状态 ─────────────────────────────────────
const dragState = reactive({
  active: false,
  moved: false,
  type: null,
  eventId: null,
  event: null,
  trackEl: null,
  startX: 0,
  origStartHour: 0,
  origEndHour: 0,
  deltaHours: 0,
});

function pxToHours(trackEl, px) {
  return (px / trackEl.getBoundingClientRect().width) * totalHours;
}
function clamp(val, min, max) { return Math.max(min, Math.min(max, val)); }
function snapToQuarter(h) { return Math.round(h * 4) / 4; }

function startDragMove(e, event, day) {
  const trackEl = e.target.closest('.day-track');
  if (!trackEl) return;
  const s = parseEventHours(event);
  Object.assign(dragState, {
    active: true, moved: false, type: 'move',
    eventId: event.id, event, trackEl,
    startX: e.clientX,
    origStartHour: s.startH, origEndHour: s.endH,
    deltaHours: 0,
  });
}

function startResize(e, event, day, side) {
  const trackEl = e.target.closest('.day-track');
  if (!trackEl) return;
  const s = parseEventHours(event);
  Object.assign(dragState, {
    active: true, moved: false,
    type: side === 'left' ? 'resize-left' : 'resize-right',
    eventId: event.id, event, trackEl,
    startX: e.clientX,
    origStartHour: s.startH, origEndHour: s.endH,
    deltaHours: 0,
  });
}

function onMouseMove(e) {
  if (!dragState.active) return;
  const dx = e.clientX - dragState.startX;
  if (Math.abs(dx) > 3) dragState.moved = true;
  dragState.deltaHours = pxToHours(dragState.trackEl, dx);
}

async function onMouseUp() {
  if (!dragState.active) return;
  if (!dragState.moved) {
    dragState.active = false;
    return;
  }

  const dh = snapToQuarter(dragState.deltaHours);
  let newStartH, newEndH;

  if (dragState.type === 'move') {
    const duration = dragState.origEndHour - dragState.origStartHour;
    newStartH = clamp(dragState.origStartHour + dh, startHour, startHour + totalHours - duration);
    newEndH = newStartH + duration;
  } else if (dragState.type === 'resize-left') {
    newStartH = clamp(dragState.origStartHour + dh, startHour, dragState.origEndHour - 0.25);
    newEndH = dragState.origEndHour;
  } else {
    newStartH = dragState.origStartHour;
    newEndH = clamp(dragState.origEndHour + dh, dragState.origStartHour + 0.25, startHour + totalHours);
  }

  newStartH = snapToQuarter(newStartH);
  newEndH = snapToQuarter(newEndH);

  const origStart = new Date(dragState.event.start_time);
  const newStart = new Date(origStart);
  newStart.setHours(Math.floor(newStartH), Math.round((newStartH % 1) * 60), 0, 0);
  const newEnd = new Date(origStart);
  newEnd.setHours(Math.floor(newEndH), Math.round((newEndH % 1) * 60), 0, 0);

  const fmt = (d) => {
    const pad = (n) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:00`;
  };

  dragState.event.start_time = fmt(newStart);
  dragState.event.end_time = fmt(newEnd);

  try {
    await window.electronAPI.updateEventTime(dragState.event.id, fmt(newStart), fmt(newEnd));
  } catch (err) {
    console.error('[WeekTimeline] Failed to persist:', err);
  }

  dragState.active = false;
  dragState.eventId = null;
  dragState.deltaHours = 0;
}

onMounted(() => {
  document.addEventListener('mousemove', onMouseMove);
  document.addEventListener('mouseup', onMouseUp);
});
onUnmounted(() => {
  document.removeEventListener('mousemove', onMouseMove);
  document.removeEventListener('mouseup', onMouseUp);
});

// ── 事件位置计算 ─────────────────────────────────
function parseEventHours(event) {
  const s = new Date(event.start_time);
  const startH = s.getHours() + s.getMinutes() / 60;
  let endH = startH + 1;
  if (event.end_time) {
    const e = new Date(event.end_time);
    endH = e.getHours() + e.getMinutes() / 60;
  }
  return { startH: Math.max(startHour, startH), endH: Math.min(startHour + totalHours, endH) };
}

function getEventLeftPct(event) {
  const { startH, endH } = parseEventHours(event);
  let effectiveStart = startH;
  if (dragState.eventId === event.id && dragState.active) {
    const dh = snapToQuarter(dragState.deltaHours);
    if (dragState.type === 'move') {
      const dur = endH - startH;
      effectiveStart = clamp(startH + dh, startHour, startHour + totalHours - dur);
    } else if (dragState.type === 'resize-left') {
      effectiveStart = clamp(startH + dh, startHour, endH - 0.25);
    }
  }
  return ((effectiveStart - startHour) / totalHours) * 100;
}

function getEventWidthPct(event) {
  const { startH, endH } = parseEventHours(event);
  let effStart = startH, effEnd = endH;
  if (dragState.eventId === event.id && dragState.active) {
    const dh = snapToQuarter(dragState.deltaHours);
    if (dragState.type === 'move') {
      const dur = endH - startH;
      effStart = clamp(startH + dh, startHour, startHour + totalHours - dur);
      effEnd = effStart + dur;
    } else if (dragState.type === 'resize-left') {
      effStart = clamp(startH + dh, startHour, endH - 0.25);
    } else if (dragState.type === 'resize-right') {
      effEnd = clamp(endH + dh, startH + 0.25, startHour + totalHours);
    }
  }
  let span = effEnd - effStart;
  if (span < 0.25) span = 0.25;
  return (span / totalHours) * 100;
}

function formatTimeRange(event) {
  const s = new Date(event.start_time);
  const pad = (n) => String(n).padStart(2, '0');
  let str = `${pad(s.getHours())}:${pad(s.getMinutes())}`;
  if (event.end_time) {
    const e = new Date(event.end_time);
    str += ` - ${pad(e.getHours())}:${pad(e.getMinutes())}`;
  }
  return str;
}

// ── 以今天为中心的 7 天 ──────────────────────────
const weekdayNames = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];

const days = computed(() => {
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const result = [];

  for (let offset = -3; offset <= 3; offset++) {
    const d = new Date(today);
    d.setDate(d.getDate() + offset);
    const dateStr = `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;
    const dayEvents = props.weekEvents.filter(ev => {
      if (!ev.start_time) return false;
      const evDate = new Date(ev.start_time);
      return evDate.getFullYear() === d.getFullYear()
        && evDate.getMonth() === d.getMonth()
        && evDate.getDate() === d.getDate();
    });

    result.push({
      dateStr,
      name: offset === 0 ? '今天' : weekdayNames[d.getDay()],
      dateLabel: `${d.getMonth()+1}/${d.getDate()}`,
      isToday: offset === 0,
      events: dayEvents,
    });
  }
  return result;
});
</script>

<style scoped>
.week-timeline {
  display: flex;
  flex-direction: column;
  background: var(--surface-card);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  padding: var(--space-4);
}

/* ═══════════════════════════════════════════════════
   横向模式
   ═══════════════════════════════════════════════════ */
.timeline-header {
  display: flex;
  padding-left: 72px;
  margin-bottom: var(--space-2);
  position: relative;
  height: 18px;
}

.time-axis {
  flex: 1;
  position: relative;
}

.tick-label {
  position: absolute;
  transform: translateX(-50%);
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  bottom: 0;
  line-height: 1;
}

.timeline-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.day-row {
  display: flex;
  align-items: center;
  height: 32px;
}

.day-label {
  width: 72px;
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  line-height: 1.2;
}

.day-label.today .day-name {
  color: #2776bb;
  font-weight: 600;
}

.day-name {
  font-size: var(--text-sm);
  color: var(--text-secondary);
  font-weight: 500;
}

.day-date {
  font-size: 10px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
}

.day-track {
  flex: 1;
  height: 100%;
  position: relative;
  background: var(--surface-glass);
  border-radius: var(--radius-sm);
  overflow: hidden;
}

.track-grid-line {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--border-subtle);
  opacity: 0.3;
  z-index: 1;
}

.track-grid-line.major {
  opacity: 0.7;
}

.event-block {
  position: absolute;
  top: 0;
  height: 100%;
  border-radius: 3px;
  background: #2776bb;
  color: white;
  display: flex;
  align-items: center;
  padding: 0 6px;
  font-size: 11px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  z-index: 2;
  cursor: grab;
  transition: opacity 0.15s;
  user-select: none;
}

.event-block:hover {
  opacity: 0.85;
  z-index: 3;
}

.event-title {
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: none;
  flex: 1;
}

/* 拖拽手柄 */
.resize-handle {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 6px;
  cursor: ew-resize;
  z-index: 5;
}
.resize-handle.left { left: 0; }
.resize-handle.right { right: 0; }
.resize-handle:hover { background: rgba(255, 255, 255, 0.2); }

/* ═══════════════════════════════════════════════════
   竖向模式（窄屏）
   ═══════════════════════════════════════════════════ */
.vertical-timeline {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.v-day-group {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.v-day-header {
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--text-secondary);
  padding-bottom: var(--space-1);
  border-bottom: 1px solid var(--border-subtle);
}

.v-day-header.today {
  color: #2776bb;
  font-weight: 600;
}

.v-empty {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  padding: var(--space-1) 0;
}

.v-event-item {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  background: var(--bg-primary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: border-color 0.2s;
}

.v-event-item:hover {
  border-color: #2776bb;
}

.v-event-time {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  white-space: nowrap;
  flex-shrink: 0;
}

.v-event-title {
  font-size: var(--text-sm);
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ═══════════════════════════════════════════════════
   详情弹窗
   ═══════════════════════════════════════════════════ */
.detail-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.45);
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
}

.detail-card {
  background: var(--bg-primary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-lg);
  padding: var(--space-6);
  width: 360px;
  max-width: 90vw;
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.detail-title {
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--text-primary);
}

.detail-field {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.detail-field label {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  font-weight: 500;
}

.detail-field span {
  font-size: var(--text-sm);
  color: var(--text-primary);
}

.detail-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  margin-top: var(--space-2);
}
</style>
