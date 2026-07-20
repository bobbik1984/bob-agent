<template>
  <div class="week-timeline" :class="{ 'is-mobile-view': isMobile }" @touchstart="handleTouchStart" @touchend="handleTouchEnd">
    <!-- 导航控制栏 -->
    <div class="timeline-header-row">
      <div class="timeline-title-area">
        <div v-if="isMobile" class="mobile-section-title">
          <Calendar :size="16" class="section-icon" />
          本周日程
        </div>
        <div class="current-month-display">
          {{ currentMonthDisplay }}
        </div>
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
            :class="{ 'drag-over': dragOverDay === day.dateStr }"
            @click.self="onTrackClick(day, $event)"
            @dragover.prevent
            @dragenter="onDragEnter(day, $event)"
            @dragleave="onDragLeave(day, $event)"
            @drop="onDrop(day, $event)"
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
                height: (resizingEventId === event.id ? resizingEventHeight : event.height) + 'px',
                left: event.left,
                width: event.width
              }"
              :class="{ 'is-short': event.height < 30 }"
              draggable="true"
              @dragstart="onDragStart(event, $event)"
              @dragend="onDragEnd"
              @click.stop="openDetail(event.raw)"
            >
              <template v-if="getTicketInfo(event.raw)">
                <div class="event-time" v-if="event.height >= 40">{{ formatTimeRange(event.raw) || '待确认' }}</div>
                <div class="ticket-dense-info" :class="{ 'row-layout': event.height < 40 }">
                  <div class="ticket-header" style="display: flex; align-items: center; gap: 4px; opacity: 0.9; font-weight: 500;">
                    <component :is="getTicketInfo(event.raw).type === 'flight' ? Plane : Train" :size="12" class="ticket-icon" />
                    <span class="ticket-code">{{ getTicketInfo(event.raw).code }}</span>
                  </div>
                  <div class="ticket-route" v-if="getTicketInfo(event.raw).origin && getTicketInfo(event.raw).dest" style="font-size: 0.95em; opacity: 0.8; margin-top: 2px;">
                    <span v-if="event.height < 40" style="margin: 0 4px;">|</span>
                    {{ getTicketInfo(event.raw).origin }} ➔ {{ getTicketInfo(event.raw).dest }}
                  </div>
                </div>
              </template>
              <template v-else>
                <div class="event-time" v-if="event.height >= 40">{{ formatTimeRange(event.raw) }}</div>
                <div class="event-title">{{ event.title }}</div>
              </template>
              <div class="resize-handle" @mousedown.stop.prevent="startResize(event, $event)"></div>
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
        <div class="detail-field" v-if="detailEvent.notes">
          <label>{{ $t('timeline.notes') || '备注' }}</label>
          <span>{{ detailEvent.notes }}</span>
        </div>
        <div class="detail-actions" style="margin-top: 16px; display: flex; gap: 8px;">
          <button v-if="detailEvent.linked_ticket_id" class="btn btn-primary" @click="viewTicket(detailEvent.linked_ticket_id)">
            {{ $t('calendar.view_ticket') || '查看凭证' }}
          </button>
          <button class="btn btn-danger" @click="handleDelete(detailEvent)">{{ $t('modal.confirm_delete') || '删除' }}</button>
        </div>
      </div>
    </div>
    <!-- 新建日程弹窗 -->
    <div v-if="newEventDialog" class="detail-overlay" @click.self="newEventDialog = false">
      <div class="detail-card">
        <h3 class="detail-title">{{ $t('timeline.new_event_title') }}</h3>
        <div class="detail-field" style="display: flex; flex-direction: column; gap: 8px;">
          <input 
            type="text" 
            v-model="newEventTitle" 
            class="bob-input" 
            style="width: 100%; padding: 10px; border-radius: var(--radius-sm); border: 1px solid var(--border-default); background: var(--bg-primary); color: var(--text-primary); outline: none;" 
            @keyup.enter="confirmCreateEvent"
            ref="newEventInput"
            autofocus
          />
        </div>
        <div class="detail-actions" style="margin-top: 16px;">
          <button class="btn btn-ghost" @click="newEventDialog = false">{{ $t('modal.cancel') || '取消' }}</button>
          <button class="btn btn-primary" @click="confirmCreateEvent" :disabled="!newEventTitle.trim()">{{ $t('modal.confirm') || '确定' }}</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted, nextTick, inject, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Calendar, Plane, Train } from 'lucide-vue-next';

const { t, tm } = useI18n();
const isMobile = inject('isMobile');
const activeDrawer = inject('activeDrawer', null);

const props = defineProps({
  weekEvents: { type: Array, default: () => [] }
});

const emit = defineEmits(['update-time', 'delete-event', 'create-event']);

const weekOffset = ref(0);
const scrollContainer = ref(null);
const resizingEventId = ref(null);
const resizingEventHeight = ref(null);

// ── 移动端滑动切换上下周 ───────────────────────
const touchStartX = ref(0);
const touchStartY = ref(0);

function handleTouchStart(e) {
  if (isMobile.value && e.touches && e.touches[0]) {
    touchStartX.value = e.touches[0].clientX;
    touchStartY.value = e.touches[0].clientY;
  }
}

function handleTouchEnd(e) {
  if (isMobile.value && e.changedTouches && e.changedTouches[0]) {
    const diffX = e.changedTouches[0].clientX - touchStartX.value;
    const diffY = e.changedTouches[0].clientY - touchStartY.value;
    // 阈值检查：水平滑动位移 > 50px 且水平方向主导（比垂直滚动角度更偏向水平，比例 1.8 避开常规垂直滚动）
    if (Math.abs(diffX) > 50 && Math.abs(diffX) > Math.abs(diffY) * 1.8) {
      if (diffX > 0) {
        // 向右滑 -> 返回上一周
        weekOffset.value--;
      } else {
        // 向左滑 -> 进到下一周
        weekOffset.value++;
      }
    }
  }
}

// ── UI 常量 ───────────────────────────────────
const PIXELS_PER_HOUR = 60; // 每小时60px高度

// 当前时间线
const currentTimeTop = ref(0);
const nowTracker = ref(new Date());
let timeInterval = null;

function updateCurrentTime() {
  nowTracker.value = new Date();
  currentTimeTop.value = (nowTracker.value.getHours() + nowTracker.value.getMinutes() / 60) * PIXELS_PER_HOUR;
}

function scrollToCurrentTime() {
  if (scrollContainer.value) {
    const now = new Date();
    const currentH = now.getHours() + now.getMinutes() / 60;
    const viewportH = scrollContainer.value.clientHeight;
    if (viewportH === 0) return;
    const offsetRows = viewportH / PIXELS_PER_HOUR / 3;
    let scrollY = Math.max(0, currentH - offsetRows) * PIXELS_PER_HOUR;
    scrollContainer.value.scrollTo({ top: scrollY, behavior: 'smooth' });
  }
}

onMounted(() => {
  updateCurrentTime();
  timeInterval = setInterval(updateCurrentTime, 60000); // 每分钟更新
  
  // 智能滚动：优先让当前时间线可见，其次考虑最早事件
  setTimeout(scrollToCurrentTime, 300); // 延迟执行以确保 DOM 已完成渲染且过渡动画结束
});

if (activeDrawer) {
  watch(activeDrawer, (newVal) => {
    if (newVal === 'schedule') {
      setTimeout(scrollToCurrentTime, 300);
    }
  });
}

onUnmounted(() => {
  if (timeInterval) clearInterval(timeInterval);
});

// ── 事件详情弹窗 ──────────────────────────────────
const detailEvent = ref(null);

const justDragged = ref(false);

function openDetail(event) {
  if (justDragged.value) {
    return;
  }
  detailEvent.value = event;
}

function viewTicket(ticketId) {
  // close detail popover
  detailEvent.value = null;
  // dispatch event to switch to kg view and open ticket
  window.dispatchEvent(new CustomEvent('switch-view', { detail: 'kg' }));
  setTimeout(() => {
    window.dispatchEvent(new CustomEvent('open-ticket-view', { detail: ticketId }));
  }, 100);
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
  if (!event.start_time) return '';
  const s = new Date(event.start_time);
  const pad = (n) => String(n).padStart(2, '0');
  // For 00:00 events without end_time, we might not want to show 00:00, but formatTimeRange handles rendering.
  // If it's a 00:00 event with NO end time, let's just return "待定" or something?
  // The user mentioned: 如果像 19 日那样缺失具体时间，我们应该优雅地显示为“待确认时间”或者隐藏时间
  if (s.getHours() === 0 && s.getMinutes() === 0 && !event.end_time) {
    return ''; // return empty so it doesn't show "00:00"
  }
  
  let str = `${pad(s.getHours())}:${pad(s.getMinutes())}`;
  if (event.end_time) {
    const e = new Date(event.end_time);
    str += ` - ${pad(e.getHours())}:${pad(e.getMinutes())}`;
  }
  return str;
}

function getTicketInfo(event) {
  if (!event.linked_ticket_id || !event.ticket_metadata) return null;
  try {
    const meta = JSON.parse(event.ticket_metadata);
    if (meta.category === 'flight' || meta.category === 'train') {
      const type = meta.category;
      const flight_info = meta.flight_info || {};
      const train_info = meta.train_info || {};
      
      let code = '';
      let origin = '';
      let dest = '';
      
      if (type === 'flight') {
        code = flight_info.flight_number || '';
        origin = flight_info.origin || '';
        dest = flight_info.destination || '';
      } else if (type === 'train') {
        code = train_info.train_number || '';
        origin = train_info.origin || '';
        dest = train_info.destination || '';
      }
      
      return {
        type,
        code,
        origin,
        dest
      };
    }
  } catch (e) {
    console.warn("Failed to parse ticket_metadata", e);
  }
  return null;
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

const newEventDialog = ref(false);
const newEventTitle = ref('');
const pendingEventData = ref(null);
const newEventInput = ref(null);

function createNewEvent(day, startHour) {
  pendingEventData.value = { day, startHour };
  newEventTitle.value = '';
  newEventDialog.value = true;
  nextTick(() => {
    if (newEventInput.value) {
      newEventInput.value.focus();
    }
  });
}

function confirmCreateEvent() {
  if (!newEventTitle.value.trim() || !pendingEventData.value) return;
  const { day, startHour } = pendingEventData.value;
  const title = newEventTitle.value.trim();
  
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
  
  newEventDialog.value = false;
  pendingEventData.value = null;
}

const dragOverDay = ref(null);

function onDragStart(event, e) {
  justDragged.value = true;
  const rect = e.currentTarget.getBoundingClientRect();
  const grabOffset = e.clientY - rect.top;
  e.dataTransfer.setData('text/plain', JSON.stringify({ eventId: event.id, grabOffset }));
}

function onDragEnd() {
  // 捕获阶段拦截并阻止拖拽结束引发的 click 事件
  const preventClick = (e) => {
    e.stopPropagation();
    e.preventDefault();
    window.removeEventListener('click', preventClick, true);
  };
  window.addEventListener('click', preventClick, true);
  setTimeout(() => {
    window.removeEventListener('click', preventClick, true);
  }, 50);

  setTimeout(() => {
    justDragged.value = false;
  }, 200);
}

function onDragEnter(day, e) {
  dragOverDay.value = day.dateStr;
}

function onDragLeave(day, e) {
  if (dragOverDay.value === day.dateStr) {
    dragOverDay.value = null;
  }
}

async function onDrop(day, e) {
  dragOverDay.value = null;
  const dragDataStr = e.dataTransfer.getData('text/plain');
  if (!dragDataStr) return;
  try {
    const { eventId, grabOffset } = JSON.parse(dragDataStr);
    const ev = props.weekEvents.find(event => event.id === eventId);
    if (!ev) return;
    
    const columnRect = e.currentTarget.getBoundingClientRect();
    const y = e.clientY - columnRect.top;
    
    let targetY = y - grabOffset;
    if (targetY < 0) targetY = 0;
    if (targetY > 24 * PIXELS_PER_HOUR) targetY = 24 * PIXELS_PER_HOUR;
    
    const snappedY = Math.round(targetY / 15) * 15;
    const startHour = snappedY / PIXELS_PER_HOUR;
    
    const start = new Date(ev.start_time);
    const end = ev.end_time ? new Date(ev.end_time) : new Date(start.getTime() + 60 * 60 * 1000);
    const durationMs = end.getTime() - start.getTime();
    
    const parts = day.dateStr.split('-');
    const targetDate = new Date(parseInt(parts[0]), parseInt(parts[1]) - 1, parseInt(parts[2]));
    const startH = Math.floor(startHour);
    const startM = Math.round((startHour - startH) * 60);
    
    const newStart = new Date(targetDate);
    newStart.setHours(startH, startM, 0, 0);
    
    const newEnd = new Date(newStart.getTime() + durationMs);
    
    await updateEventTimes(ev.id, newStart.toISOString(), newEnd.toISOString());
  } catch (err) {
    console.error('Failed to parse drag data', err);
  }
}

function startResize(event, startEvent) {
  justDragged.value = true;
  resizingEventId.value = event.id;
  resizingEventHeight.value = event.height;

  const startY = startEvent.clientY;
  const startHeight = event.height;
  const ev = props.weekEvents.find(e => e.id === event.id);
  if (!ev) return;
  
  const onMouseMove = (moveEvent) => {
    const deltaY = moveEvent.clientY - startY;
    let newHeight = startHeight + deltaY;
    if (newHeight < 15) newHeight = 15;
    const snappedHeight = Math.round(newHeight / 15) * 15;
    resizingEventHeight.value = snappedHeight;
  };
  
  const onMouseUp = async (upEvent) => {
    window.removeEventListener('mousemove', onMouseMove);
    window.removeEventListener('mouseup', onMouseUp);
    
    // 捕获阶段拦截并阻止调整大小时长结束引发的 click 事件
    const preventClick = (e) => {
      e.stopPropagation();
      e.preventDefault();
      window.removeEventListener('click', preventClick, true);
    };
    window.addEventListener('click', preventClick, true);
    setTimeout(() => {
      window.removeEventListener('click', preventClick, true);
    }, 50);

    setTimeout(() => {
      justDragged.value = false;
    }, 200);

    const deltaY = upEvent.clientY - startY;
    let newHeight = startHeight + deltaY;
    if (newHeight < 15) newHeight = 15;
    const snappedHeight = Math.round(newHeight / 15) * 15;
    
    const durationHours = snappedHeight / PIXELS_PER_HOUR;
    const start = new Date(ev.start_time);
    const end = new Date(start.getTime() + durationHours * 60 * 60 * 1000);
    
    resizingEventId.value = null;
    resizingEventHeight.value = null;

    await updateEventTimes(ev.id, start.toISOString(), end.toISOString());
  };
  
  window.addEventListener('mousemove', onMouseMove);
  window.addEventListener('mouseup', onMouseUp);
}

async function updateEventTimes(id, startTime, endTime) {
  if (window.appAPI && window.appAPI.updateEventTime) {
    const success = await window.appAPI.updateEventTime(id, startTime, endTime);
    if (success) {
      const ev = props.weekEvents.find(e => e.id === id);
      if (ev) {
        ev.start_time = startTime;
        ev.end_time = endTime;
      }
      emit('update-time', { id, start_time: startTime, end_time: endTime });
    }
  }
}

// ── 以今天为中心的 7 天 ──────────────────────────
const weekdayNames = computed(() => tm('timeline.days') || ['周日', '周一', '周二', '周三', '周四', '周五', '周六']);

const days = computed(() => {
  const today = new Date(nowTracker.value);
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

const currentMonthDisplay = computed(() => {
  if (days.value.length === 0) return '';
  const midWeekDay = days.value[3];
  if (!midWeekDay || !midWeekDay.dateStr) return '';
  const parts = midWeekDay.dateStr.split('-');
  if (parts.length >= 2) {
    return `${parts[0]}年 ${parseInt(parts[1], 10)}月`;
  }
  return '';
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
  height: 900px; /* 固定高度，内部滚动 */
}

.timeline-header-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--space-3);
  padding-bottom: var(--space-2);
  border-bottom: 1px solid var(--border-subtle);
}

.timeline-title-area {
  display: flex;
  align-items: center;
  gap: 12px;
}

.current-month-display {
  font-size: 1.4em;
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: 0.5px;
}

.timeline-controls {
  display: flex;
  gap: var(--space-1);
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
  transition: background-color 0.2s ease, outline 0.2s ease;
}
.day-column.drag-over {
  background: var(--accent-glow, rgba(128, 128, 128, 0.08));
  outline: 1px dashed var(--user-accent, var(--accent-primary));
  outline-offset: -1px;
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
  user-select: none;
}
.resize-handle {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 6px;
  cursor: ns-resize;
  background: transparent;
  z-index: 10;
}
.resize-handle:hover {
  background: var(--user-accent, var(--accent-primary));
  opacity: 0.5;
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

.ticket-dense-info {
  display: flex;
  flex-direction: column;
  margin-top: 2px;
}
.ticket-dense-info.row-layout {
  flex-direction: row;
  align-items: center;
  margin-top: 0;
}
.ticket-header {
  white-space: nowrap;
}
.ticket-route {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ticket-code {
  font-size: 12px;
}
.is-short .ticket-code {
  font-size: 11px;
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

.week-timeline.is-mobile-view {
  flex: 1;
  min-height: 0;
  height: 100% !important;
  background: transparent !important;
  border: none !important;
  border-radius: 0 !important;
  padding: 0 !important;
}

.week-timeline.is-mobile-view .timeline-header-row {
  padding: 0 16px;
  margin: 0 0 12px 0;
  justify-content: space-between;
  align-items: center;
  height: 36px;
}

.week-timeline.is-mobile-view .timeline-controls {
  margin: 0 !important;
  padding: 0 !important;
}

.week-timeline.is-mobile-view .mobile-section-title {
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--text-secondary);
  display: flex;
  align-items: center;
  gap: 6px;
}

.week-timeline.is-mobile-view .section-icon {
  opacity: 0.8;
}

.week-timeline.is-mobile-view .calendar-header {
  background: transparent !important;
  padding-right: 0 !important;
}

.week-timeline.is-mobile-view .time-axis,
.week-timeline.is-mobile-view .time-axis-header {
  background: transparent !important;
}

.week-timeline.is-mobile-view .calendar-wrapper {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  border: none !important;
  border-radius: 0 !important;
  background: transparent !important;
}
</style>
