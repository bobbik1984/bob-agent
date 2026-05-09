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
        <div class="day-track">
          <!-- 背景网格线 -->
          <div
            v-for="hour in hours"
            :key="'grid-'+hour"
            class="track-grid-line"
            :style="{ left: getHourPosition(hour) + '%' }"
          ></div>

          <!-- 事件块 -->
          <div
            v-for="event in day.events"
            :key="event.id"
            class="event-block"
            :class="[event.priority || 'medium']"
            :style="{
              left: getEventStartPct(event) + '%',
              width: getEventSpanPct(event) + '%'
            }"
            :title="event.title"
          >
            <span class="event-title">{{ event.title }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue';

const props = defineProps({
  weekEvents: {
    type: Array,
    default: () => []
  }
});

// 8:00 到 24:00 (共 16 小时)
const hours = Array.from({ length: 17 }, (_, i) => i + 8);
const startHour = 8;
const totalHours = 16;

const getHourPosition = (hour) => {
  return ((hour - startHour) / totalHours) * 100;
};

// 工具：解析ISO字符串并按周归类
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

  // 假定仅传入本周的 events
  props.weekEvents.forEach(event => {
    if (!event.start_time) return;
    const start = new Date(event.start_time);
    const dayOfWeek = start.getDay();
    dayMap[dayOfWeek].events.push(event);
  });

  // 从周一到周日排序
  return [
    dayMap[1], dayMap[2], dayMap[3], dayMap[4], dayMap[5], dayMap[6], dayMap[0]
  ];
});

const getEventStartPct = (event) => {
  if (!event.start_time) return 0;
  const start = new Date(event.start_time);
  const h = start.getHours();
  const m = start.getMinutes();
  const floatHour = h + m / 60;

  if (floatHour < startHour) return 0;
  if (floatHour > 24) return 100;

  return ((floatHour - startHour) / totalHours) * 100;
};

const getEventSpanPct = (event) => {
  if (!event.start_time) return 0;
  const start = new Date(event.start_time);
  const h1 = start.getHours() + start.getMinutes() / 60;
  let floatHourStart = Math.max(startHour, h1);

  let floatHourEnd = floatHourStart + 1; // 默认1小时
  if (event.end_time) {
    const end = new Date(event.end_time);
    floatHourEnd = end.getHours() + end.getMinutes() / 60;
  }

  if (floatHourEnd > 24) floatHourEnd = 24;

  let span = floatHourEnd - floatHourStart;
  if (span < 0.5) span = 0.5; // 最小宽度

  return (span / totalHours) * 100;
};
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
  padding-left: 60px; /* 留出左侧 day-label 的宽度 */
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
  cursor: pointer;
  transition: transform 0.2s;
}

.event-block:hover {
  transform: translateY(-2px);
  z-index: 3;
}

.event-block.high { background: var(--color-error); }
.event-block.medium { background: var(--color-warning); color: #000; }
.event-block.low { background: var(--color-success); }

.event-title {
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: none;
}
</style>
