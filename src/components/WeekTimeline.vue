<template>
  <div class="week-timeline">
    <!-- 导航控制栏 -->
    <div class="timeline-header-row">
      <div v-if="isMobile" class="mobile-section-title">
        <Calendar :size="16" class="section-icon" />
        本周日程
      </div>
      <div class="timeline-controls">
        <button class="nav-btn" @click="weekOffset--">&lsaquo;</button>
        <button class="nav-btn this-week-btn" @click="weekOffset = 0" :disabled="weekOffset === 0">{{ $t('timeline.this_week') || '本周' }}</button>
        <button class="nav-btn" @click="weekOffset++">&rsaquo;</button>
      </div>
    </div>

    <!-- 垂直网格视图 -->
    <div class="calendar-wrapper">
      <!-- 头部：日期列 -->
      <div class="calendar-header">
        <div class="time-axis-header">GMT+8</div>
        <div 
          v-for="day in days" 
          :key="day.dateStr" 
          class="day-header"
          :class="{ today: day.isToday }"
        >
          <div class="day-name">{{ day.name }}</div>
          <div class="day-date" :class="{ 'today-circle': day.isToday }">{{ day.dayNum }}</div>
        </div>
      </div>

      <!-- 主体：可滚动的时间网格 -->
      <div class="calendar-body" ref="scrollContainer">
        <div class="time-axis">
          <div v-for="h in Math.floor(24)" :key="h" class="time-slot-label">
            <span>{{ String(h).padStart(2, '0') }}:00</span>
          </div>
        </div>

        <div class="days-grid">
          <div 
            v-for="day in days" 
            :key="day.dateStr" 
            class="day-column"
            @click.self="onTrackClick(day, $event)"
          >
            <!-- 背景网格线 -->
            <div v-for="h in Math.floor(24)" :key="h" class="grid-cell" @click.self="onCellClick(day, h, $event)"></div>
            
            <!-- 当前时间线 (仅今天显示) -->
            <div v-if="day.isToday" class="current-time-line" :style="{ top: currentTimeTop + 'px' }">
              <div class="current-time-dot"></div>
            </div>

            <!-- 事件卡片 -->
            <div
              v-for="event in day.layoutEvents"
              :key="event.id"
              class="event-card"
              :style="{
                top: event.top + 'px',
                height: event.height + 'px',
                left: event.left,
                width: event.width
              }"
              :class="{ 'is-short': event.height < 30 }"
              @click.stop="openDetail(event.raw)"
            >
              <div class="event-time" v-if="event.height >= 40">{{ formatTimeRange(event.raw) }}</div>
              <div class="event-title">{{ event.title }}</div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 事件详情弹窗 -->
    <div v-if="detailEvent" class="detail-overlay" @click.self="detailEvent = null">
      <div class="detail-card">
        <h3 class="detail-title">{{ detailEvent.title }}</h3>
        <div class="detail-field">
          <label>{{ $t('timeline.time') || '时间' }}</label>
          <span>{{ formatTimeRange(detailEvent) }}</span>
        </div>
        <div v-if="detailEvent.location" class="detail-field">
          <label>{{ $t('timeline.location') || '地点' }}</label>
          <span>{{ detailEvent.location }}</span>
        </div>
        <div v-if="detailEvent.notes" class="detail-field">
          <label>{{ $t('timeline.notes') || '备注' }}</label>
          <span>{{ detailEvent.notes }}</span>
        </div>
        <div class="detail-actions">
          <button class="btn btn-ghost" @click="detailEvent = null">{{ $t('modal.cancel') || '取消' }}</button>
          <button class="btn btn-danger" @click="handleDelete(detailEvent)">{{ $t('modal.confirm_delete') || '删除' }}</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted, nextTick, inject } from 'vue';
import { useI18n } from 'vue-i18n';
import { Calendar } from 'lucide-vue-next';

const { t, tm } = useI18n();
const isMobile = inject('isMobile');

const props = defineProps({
  weekEvents: { type: Array, default: () => [] }
});

const emit = defineEmits(['update-time', 'delete-event', 'create-event']);

const weekOffset = ref(0);
const scrollContainer = ref(null);

// ── UI 常量 ───────────────────────────────────
const PIXELS_PER_HOUR = 60; // 每小时60px高度

// 当前时间线
const currentTimeTop = ref(0);
let timeInterval = null;

function updateCurrentTime() {
  const now = new Date();
  currentTimeTop.value = (now.getHours() + now.getMinutes() / 60) * PIXELS_PER_HOUR;
}

onMounted(() => {
  updateCurrentTime();
  timeInterval = setInterval(updateCurrentTime, 60000); // 每分钟更新
  
  // 智能滚动：优先让当前时间线可见，其次考虑最早事件
  setTimeout(() => {
    if (scrollContainer.value) {
      const now = new Date();
      const currentH = now.getHours() + now.getMinutes() / 60;
      
      // 计算可视区域高度，将当前时间线定位在视口上方 1/3 处
      const viewportH = scrollContainer.value.clientHeight;
      const offsetRows = viewportH / PIXELS_PER_HOUR / 3; // 上方留约 1/3 视口
      
      let scrollY = Math.max(0, currentH - offsetRows) * PIXELS_PER_HOUR;
      
      scrollContainer.value.scrollTo({ top: scrollY, behavior: 'smooth' });
    }
  }, 300); // 延迟执行以确保 DOM 已完成渲染且过渡动画结束
});

onUnmounted(() => {
  if (timeInterval) clearInterval(timeInterval);
});

// ── 事件详情弹窗 ──────────────────────────────────
const detailEvent = ref(null);

function openDetail(event) {
  detailEvent.value = event;
}

async function handleDelete(event) {
  try {
    if (window.appAPI && window.appAPI.deleteEvent) {
      await window.appAPI.deleteEvent(event.id);
    }
    emit('delete-event', event.id);
    const idx = props.weekEvents.findIndex(e => e.id === event.id);
    if (idx !== -1) props.weekEvents.splice(idx, 1);
    detailEvent.value = null;
  } catch (err) {
    console.error('[WeekTimeline] 删除失败', err);
  }
}

// ── 格式化辅助 ──────────────────────────────────
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

function parseEventHours(event) {
  const s = new Date(event.start_time);
  const startH = s.getHours() + s.getMinutes() / 60;
  let endH = startH + 1;
  if (event.end_time) {
    const e = new Date(event.end_time);
    endH = e.getHours() + e.getMinutes() / 60;
  }
  return { startH, endH: Math.max(startH + 0.25, endH) }; // 至少15分钟高度
}

// ── 布局重叠算法 (Overlapping events) ─────────────
function layoutDayEvents(eventsList) {
  // 1. 转换并排序
  const events = eventsList.map(ev => {
    const { startH, endH } = parseEventHours(ev);
    return {
      raw: ev,
      id: ev.id,
      title: ev.title,
      startH,
      endH,
      top: startH * PIXELS_PER_HOUR,
      height: (endH - startH) * PIXELS_PER_HOUR
    };
  }).sort((a, b) => a.startH - b.startH);

  // 2. 分组（计算连通图）
  const groups = [];
  let currentGroup = [];
  let groupEnd = -1;

  for (const ev of events) {
    if (ev.startH >= groupEnd) {
      if (currentGroup.length > 0) {
        groups.push(currentGroup);
      }
      currentGroup = [ev];
      groupEnd = ev.endH;
    } else {
      currentGroup.push(ev);
      groupEnd = Math.max(groupEnd, ev.endH);
    }
  }
  if (currentGroup.length > 0) groups.push(currentGroup);

  // 3. 计算组内列数分配
  const result = [];
  for (const group of groups) {
    const columns = [];
    for (const ev of group) {
      let placed = false;
      for (const col of columns) {
        const lastEv = col[col.length - 1];
        if (lastEv.endH <= ev.startH) {
          col.push(ev);
          placed = true;
          break;
        }
      }
      if (!placed) {
        columns.push([ev]);
      }
    }
    
    const numCols = columns.length;
    columns.forEach((col, colIndex) => {
      col.forEach(ev => {
        ev.left = `${(colIndex / numCols) * 100}%`;
        // 留出1%的边距避免挤死
        ev.width = `${(100 / numCols) - 1}%`;
        result.push(ev);
      });
    });
  }
  
  return result;
}

// ── 新建事件 ──────────────────────────────────────
function onCellClick(day, hour, e) {
  // 点击某个格子，hour 是 1~24
  const startHour = hour - 1; // 因为循环是 1-24
  createNewEvent(day, startHour);
}

function onTrackClick(day, e) {
  // 点在空白处
  const rect = e.currentTarget.getBoundingClientRect();
  const y = e.clientY - rect.top;
  const startHour = Math.floor(y / PIXELS_PER_HOUR);
  createNewEvent(day, startHour);
}

function createNewEvent(day, startHour) {
  const title = prompt(t('timeline.new_event_title') || '请输入新日程的标题：', '新日程');
  if (!title) return;
  
  const parts = day.dateStr.split('-');
  const start = new Date(parseInt(parts[0]), parseInt(parts[1]) - 1, parseInt(parts[2]));
  start.setHours(startHour, 0, 0, 0);
  
  const end = new Date(start);
  end.setHours(startHour + 1, 0, 0, 0);
  
  const fmt = (d) => {
    const pad = (n) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:00`;
  };

  emit('create-event', {
    title,
    type: 'event',
    date: day.dateStr,
    startTime: fmt(start),
    endTime: fmt(end)
  });
}

// ── 以今天为中心的 7 天 ──────────────────────────
const weekdayNames = computed(() => tm('timeline.days') || ['周日', '周一', '周二', '周三', '周四', '周五', '周六']);

const days = computed(() => {
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const result = [];
  
  const baseDate = new Date(today);
  // 以今天为中心（往过去推 3 天，往未来推 3 天）
  baseDate.setDate(baseDate.getDate() - 3 + (weekOffset.value * 7));

  for (let offset = 0; offset < 7; offset++) {
    const d = new Date(baseDate);
    d.setDate(d.getDate() + offset);
    const dateStr = `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;
    
    const dayEvents = props.weekEvents.filter(ev => {
      if (!ev.start_time) return false;
      const evDate = new Date(ev.start_time);
      return evDate.getFullYear() === d.getFullYear()
        && evDate.getMonth() === d.getMonth()
        && evDate.getDate() === d.getDate();
    });

    const isToday = d.getTime() === today.getTime();
    
    const nameStr = weekdayNames.value[d.getDay()];

    result.push({
      dateStr,
      name: nameStr,
      dayNum: d.getDate(),
      isToday: isToday,
      rawEvents: dayEvents,
      layoutEvents: layoutDayEvents(dayEvents)
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
  height: 600px; /* 固定高度，内部滚动 */
}

.timeline-controls {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-1);
  margin-bottom: var(--space-3);
  padding-bottom: var(--space-2);
}

.nav-btn {
  font-size: 16px;
  line-height: 1;
  padding: 0;
  width: 28px;
  height: 28px;
  border-radius: 14px;
  background: var(--surface-primary);
  border: 1px solid var(--border-subtle);
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.2s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.nav-btn.this-week-btn {
  width: auto;
  padding: 0 12px;
  font-size: 12px;
}

.nav-btn:hover:not(:disabled) {
  border-color: var(--accent-primary);
  color: var(--accent-primary);
  background: color-mix(in srgb, var(--accent-primary) 8%, transparent);
}

.nav-btn:disabled {
  opacity: 0.4;
  cursor: default;
}

/* 垂直网格结构 */
.calendar-wrapper {
  display: flex;
  flex-direction: column;
  flex: 1;
  overflow: hidden;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--bg-primary);
}

.calendar-header {
  display: flex;
  border-bottom: 1px solid var(--border-subtle);
  background: var(--surface-secondary);
  padding-right: 0;
}

.time-axis-header {
  width: 60px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 10px;
  color: var(--text-tertiary);
  border-right: 1px solid var(--border-subtle);
}

.day-header {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: var(--space-2) 0;
  border-right: 1px solid var(--border-subtle);
}
.day-header:last-child {
  border-right: none;
}

.day-name {
  font-size: 11px;
  color: var(--text-secondary);
  margin-bottom: 2px;
}

.day-date {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
}

.day-header.today .day-name {
  color: var(--accent-primary);
  font-weight: 600;
}
.today-circle {
  background: var(--user-accent, var(--accent-primary));
  color: var(--text-inverse, white);
}

.calendar-body {
  display: flex;
  flex: 1;
  overflow-y: auto;
  position: relative;
  scrollbar-width: none; /* Firefox */
  -ms-overflow-style: none;  /* Internet Explorer 10+ */
}

.calendar-body::-webkit-scrollbar {
  display: none; /* WebKit */
}

.time-axis {
  width: 60px;
  flex-shrink: 0;
  border-right: 1px solid var(--border-subtle);
  background: var(--surface-primary);
  height: 1440px;
}

.time-slot-label {
  height: 60px; /* PIXELS_PER_HOUR */
  display: flex;
  justify-content: center;
  padding-top: 8px; /* Offset to align with the grid line */
  box-sizing: border-box;
}
.time-slot-label span {
  font-size: 10px;
  color: var(--text-tertiary);
  transform: translateY(-50%); /* Align the text center exactly with the border line */
}

.days-grid {
  display: flex;
  flex: 1;
  position: relative;
  height: 1440px;
}

.day-column {
  flex: 1;
  position: relative;
  border-right: 1px solid var(--border-subtle);
  min-width: 0; /* flex bug fix */
}
.day-column:last-child {
  border-right: none;
}

.grid-cell {
  height: 60px;
  border-bottom: 1px solid var(--border-subtle);
  box-sizing: border-box;
}

/* 当前时间线 */
.current-time-line {
  position: absolute;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--color-error);
  z-index: 10;
  pointer-events: none;
}
.current-time-dot {
  position: absolute;
  left: -4px;
  top: -4px;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--color-error);
}

/* 事件卡片 */
.event-card {
  position: absolute;
  background: var(--accent-glow, rgba(128, 128, 128, 0.15));
  border-left: 3px solid var(--user-accent, var(--accent-primary));
  color: var(--text-primary);
  border-radius: 4px;
  padding: 4px 6px;
  overflow: hidden;
  box-shadow: 0 1px 3px rgba(0,0,0,0.1);
  cursor: pointer;
  transition: all 0.2s ease;
  z-index: 5;
  display: flex;
  flex-direction: column;
}

.event-card:hover {
  filter: brightness(1.2);
  z-index: 6;
  transform: translateY(-1px);
  box-shadow: 0 4px 8px rgba(0,0,0,0.15);
}

.event-time {
  font-size: 10px;
  color: var(--user-accent, var(--accent-primary));
  font-weight: 600;
  margin-bottom: 2px;
  white-space: nowrap;
}

.event-title {
  font-size: 12px;
  font-weight: 500;
  line-height: 1.2;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
}

.is-short .event-title {
  font-size: 11px;
  white-space: nowrap;
  -webkit-line-clamp: 1;
}

/* 详情弹窗 */
.detail-overlay {
  position: fixed;
  inset: 0;
  background: var(--overlay-bg);
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
  box-shadow: var(--shadow-lg);
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
  white-space: pre-wrap;
}

.detail-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  margin-top: var(--space-2);
}

.timeline-header-row {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  width: 100%;
}

@media (max-width: 768px) {
  .week-timeline {
    flex: 1;
    min-height: 0;
    height: 100% !important;
    background: transparent !important;
    border: none !important;
    border-radius: 0 !important;
    padding: 0 !important;
  }

  .timeline-header-row {
    padding: 0 16px;
    margin: 0 0 12px 0;
    justify-content: space-between;
    align-items: center;
    height: 36px;
  }
  
  .timeline-controls {
    margin: 0 !important;
    padding: 0 !important;
  }
  
  .mobile-section-title {
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    gap: 6px;
  }
  
  .section-icon {
    opacity: 0.8;
  }

  .calendar-header {
    background: transparent !important;
    padding-right: 0 !important;
  }

  .time-axis,
  .time-axis-header {
    background: transparent !important;
  }

  .calendar-wrapper {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: none !important;
    border-radius: 0 !important;
    background: transparent !important;
  }
}
</style>
