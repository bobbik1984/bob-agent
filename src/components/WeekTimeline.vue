<template>
  <div class="week-timeline">
    <div class="timeline-header">
      <div class="time-axis">
        <div
          v-for="hour in hours"
          :key="hour"
          class="time-tick"
          :style="{ left: getHourPosition(hour) + '%' }"
        >
          {{ hour }}:00
        </div>
      </div>
    </div>
    <div class="timeline-body">
      <div
        v-for="day in days"
        :key="day.date"
        class="day-row"
      >
        <div class="day-label">{{ day.name }}</div>
        <div class="day-track" ref="trackRefs">
          <!-- 背景网格线 -->
          <div
            v-for="hour in hours"
            :key="'grid-'+hour"
            class="track-grid-line"
            :style="{ left: getHourPosition(hour) + '%' }"
          ></div>

          <!-- 事件块（可拖拽 + 可调整） -->
          <div
            v-for="event in day.events"
            :key="event.id"
            class="event-block"
            :class="[event.priority || 'medium', { dragging: dragState.eventId === event.id }]"
            :style="{
              left: getEventLeftPct(event) + '%',
              width: getEventWidthPct(event) + '%'
            }"
            :title="event.title + '\n' + formatTimeRange(event)"
            @mousedown.prevent="startDragMove($event, event, day)"
          >
            <!-- 左拖柄 -->
            <div class="resize-handle left" @mousedown.stop.prevent="startResize($event, event, day, 'left')"></div>
            <span class="event-title">{{ event.title }}</span>
            <!-- 右拖柄 -->
            <div class="resize-handle right" @mousedown.stop.prevent="startResize($event, event, day, 'right')"></div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, reactive, onMounted, onUnmounted } from 'vue';

const props = defineProps({
  weekEvents: {
    type: Array,
    default: () => []
  }
});

const emit = defineEmits(['update-time']);

// 8:00 到 24:00 (共 16 小时)
const hours = Array.from({ length: 17 }, (_, i) => i + 8);
const startHour = 8;
const totalHours = 16;

const getHourPosition = (hour) => {
  return ((hour - startHour) / totalHours) * 100;
};

// ── 拖拽状态 ─────────────────────────────────────────
const dragState = reactive({
  active: false,
  type: null,       // 'move' | 'resize-left' | 'resize-right'
  eventId: null,
  event: null,
  dayEvents: null,
  trackEl: null,
  startX: 0,
  origStartHour: 0,
  origEndHour: 0,
  // 实时偏移（小时）
  deltaHours: 0,
});

function pxToHours(trackEl, px) {
  const trackWidth = trackEl.getBoundingClientRect().width;
  return (px / trackWidth) * totalHours;
}

function clamp(val, min, max) {
  return Math.max(min, Math.min(max, val));
}

// 将小时数对齐到 15 分钟
function snapToQuarter(h) {
  return Math.round(h * 4) / 4;
}

function startDragMove(e, event, day) {
  const trackEl = e.target.closest('.day-track');
  if (!trackEl) return;
  const s = parseEventHours(event);
  Object.assign(dragState, {
    active: true,
    type: 'move',
    eventId: event.id,
    event,
    dayEvents: day.events,
    trackEl,
    startX: e.clientX,
    origStartHour: s.startH,
    origEndHour: s.endH,
    deltaHours: 0,
  });
}

function startResize(e, event, day, side) {
  const trackEl = e.target.closest('.day-track');
  if (!trackEl) return;
  const s = parseEventHours(event);
  Object.assign(dragState, {
    active: true,
    type: side === 'left' ? 'resize-left' : 'resize-right',
    eventId: event.id,
    event,
    dayEvents: day.events,
    trackEl,
    startX: e.clientX,
    origStartHour: s.startH,
    origEndHour: s.endH,
    deltaHours: 0,
  });
}

function onMouseMove(e) {
  if (!dragState.active) return;
  const dx = e.clientX - dragState.startX;
  dragState.deltaHours = pxToHours(dragState.trackEl, dx);
}

async function onMouseUp() {
  if (!dragState.active) return;

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

  // 更新原始事件的日期部分 + 新时间
  const origStart = new Date(dragState.event.start_time);
  const newStart = new Date(origStart);
  newStart.setHours(Math.floor(newStartH), Math.round((newStartH % 1) * 60), 0, 0);
  const newEnd = new Date(origStart);
  newEnd.setHours(Math.floor(newEndH), Math.round((newEndH % 1) * 60), 0, 0);

  const fmt = (d) => {
    const pad = (n) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:00`;
  };

  // 更新本地状态
  dragState.event.start_time = fmt(newStart);
  dragState.event.end_time = fmt(newEnd);

  // 持久化
  try {
    await window.electronAPI.updateEventTime(dragState.event.id, fmt(newStart), fmt(newEnd));
  } catch (err) {
    console.error('[WeekTimeline] Failed to persist time change:', err);
  }

  // 重置
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

// ── 计算工具 ─────────────────────────────────────────
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
  let effStart = startH;
  let effEnd = endH;

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

// ── 周布局 ─────────────────────────────────────────
const days = computed(() => {
  const dayMap = {
    0: { name: '周日', events: [] },
    1: { name: '周一', events: [] },
    2: { name: '周二', events: [] },
    3: { name: '周三', events: [] },
    4: { name: '周四', events: [] },
    5: { name: '周五', events: [] },
    6: { name: '周六', events: [] }
  };

  props.weekEvents.forEach(event => {
    if (!event.start_time) return;
    const start = new Date(event.start_time);
    const dayOfWeek = start.getDay();
    dayMap[dayOfWeek].events.push(event);
  });

  return [
    dayMap[1], dayMap[2], dayMap[3], dayMap[4], dayMap[5], dayMap[6], dayMap[0]
  ];
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
  overflow-x: auto;
}

.timeline-header {
  display: flex;
  padding-left: 60px;
  margin-bottom: var(--space-2);
  position: relative;
  height: 24px;
}

.time-axis {
  flex: 1;
  position: relative;
}

.time-tick {
  position: absolute;
  transform: translateX(-50%);
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  bottom: 0;
}

.timeline-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.day-row {
  display: flex;
  align-items: center;
  height: 40px;
}

.day-label {
  width: 60px;
  font-size: var(--text-sm);
  color: var(--text-secondary);
  font-weight: 500;
  flex-shrink: 0;
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
  z-index: 1;
}

.event-block {
  position: absolute;
  top: 4px;
  height: 32px;
  border-radius: 4px;
  background: var(--accent-primary);
  color: white;
  display: flex;
  align-items: center;
  padding: 0 8px;
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  z-index: 2;
  box-shadow: 0 2px 4px rgba(0,0,0,0.2);
  cursor: grab;
  transition: box-shadow 0.15s;
  user-select: none;
}

.event-block.dragging {
  cursor: grabbing;
  box-shadow: 0 4px 12px rgba(0,0,0,0.4);
  z-index: 10;
  opacity: 0.9;
}

.event-block:hover {
  box-shadow: 0 3px 8px rgba(0,0,0,0.3);
  z-index: 3;
}

.event-block.high { background: var(--color-error); }
.event-block.medium { background: var(--color-warning); color: #000; }
.event-block.low { background: var(--color-success); }

.event-title {
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: none;
  flex: 1;
}

/* ── 拖拽调整手柄 ──────────────────────────────── */
.resize-handle {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 6px;
  cursor: ew-resize;
  z-index: 5;
}

.resize-handle.left {
  left: 0;
  border-radius: 4px 0 0 4px;
}

.resize-handle.right {
  right: 0;
  border-radius: 0 4px 4px 0;
}

.resize-handle:hover {
  background: rgba(255, 255, 255, 0.25);
}
</style>
