<template>
  <div class="ticket-card-wrapper" :class="{ 'is-expired': isExpired }">
    <div class="ticket-header">
      <div class="ticket-title-row">
        <component :is="categoryIcon" class="category-icon" />
        <span class="ticket-title">{{ node.label }}</span>
        <span class="ticket-status" :class="ticketStatusClass">{{ displayStatus }}</span>
      </div>
    </div>

    <div class="ticket-body">
      <!-- 针对机票和火车的特殊高亮排版 -->
      <div v-if="isTravel" class="travel-route">
        <div class="route-point">
          <div class="time">{{ formatTimeOnly(metadata.start_time) }}</div>
          <div class="location">{{ originLabel }}</div>
        </div>
        <div class="route-arrow">
          <Plane v-if="metadata.category === 'flight'" class="arrow-icon" />
          <Train v-else-if="metadata.category === 'train'" class="arrow-icon" />
          <ArrowRight v-else class="arrow-icon" />
          <div class="duration" v-if="metadata.flight_info?.flight_number">{{ metadata.flight_info.flight_number }}</div>
        </div>
        <div class="route-point right">
          <div class="time">{{ formatTimeOnly(metadata.end_time) || '--:--' }}</div>
          <div class="location">{{ destinationLabel }}</div>
        </div>
      </div>

      <!-- 常规场馆/时间排版 (电影、展会等) -->
      <div v-else class="generic-info">
        <div class="info-group">
          <label>{{ $t('calendar.time') || 'Time' }}</label>
          <div class="val">{{ metadata.start_time }}</div>
        </div>
        <div class="info-group" v-if="metadata.venue">
          <label>{{ $t('calendar.venue') || 'Venue' }}</label>
          <div class="val">{{ metadata.venue }}</div>
        </div>
      </div>

      <div class="sub-info-grid" v-if="hasSubInfo">
        <div class="info-cell" v-if="seatLabel">
          <label>{{ $t('ticket.seat') || 'Seat/Gate' }}</label>
          <div class="val highlight">{{ seatLabel }}</div>
        </div>
        <div class="info-cell" v-if="metadata.flight_info?.carrier">
          <label>{{ $t('ticket.carrier') || 'Carrier' }}</label>
          <div class="val">{{ metadata.flight_info.carrier }}</div>
        </div>
        <div class="info-cell" v-if="metadata.flight_info?.pnr">
          <label>PNR</label>
          <div class="val">{{ metadata.flight_info.pnr }}</div>
        </div>
      </div>
    </div>

    <div class="ticket-footer" v-if="metadata.barcode_data">
      <div class="qr-container">
        <qrcode-vue 
          :value="metadata.barcode_data" 
          :size="160" 
          level="H" 
          background="var(--bg-tertiary)"
          foreground="var(--text-primary)"
          render-as="svg"
        />
        <div class="qr-center-logo">
          <component :is="categoryIcon" class="qr-inner-icon" />
        </div>
      </div>
      <div class="barcode-raw" v-if="metadata.barcode_type !== 'qr'">
        {{ metadata.barcode_data }}
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue';
import { Plane, Film, Ticket, Calendar, Train, ArrowRight, CreditCard, Music } from 'lucide-vue-next';
import QrcodeVue from 'qrcode.vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  node: {
    type: Object,
    required: true
  }
});

const { t } = useI18n();

const metadata = computed(() => {
  if (typeof props.node.metadata === 'string') {
    try {
      return JSON.parse(props.node.metadata);
    } catch {
      return {};
    }
  }
  return props.node.metadata || {};
});

const isExpired = computed(() => {
  if (!metadata.value.start_time) return false;
  const startTime = new Date(metadata.value.start_time).getTime();
  return Date.now() > startTime + 24 * 3600 * 1000; // 过期一天后
});

const isTravel = computed(() => {
  return metadata.value.category === 'flight' || metadata.value.category === 'train';
});

const categoryIcon = computed(() => {
  switch (metadata.value.category) {
    case 'flight': return Plane;
    case 'movie': return Film;
    case 'train': return Train;
    case 'membership': return CreditCard;
    case 'concert': return Music;
    case 'exhibition': return Ticket;
    default: return Ticket;
  }
});

const displayStatus = computed(() => {
  if (isExpired.value) return t('ticket.status_expired') || 'Expired';
  if (metadata.value.status === 'upcoming') return t('ticket.status_upcoming') || 'Upcoming';
  return metadata.value.status || '';
});

const ticketStatusClass = computed(() => {
  if (isExpired.value) return 'status-expired';
  return 'status-active';
});

const originLabel = computed(() => {
  if (metadata.value.flight_info?.origin) return metadata.value.flight_info.origin;
  return metadata.value.venue?.split('-')[0] || '';
});

const destinationLabel = computed(() => {
  if (metadata.value.flight_info?.destination) return metadata.value.flight_info.destination;
  return metadata.value.venue?.split('-')[1] || '';
});

const seatLabel = computed(() => {
  if (metadata.value.seat_info) return metadata.value.seat_info;
  if (metadata.value.flight_info?.seat) return metadata.value.flight_info.seat;
  return '';
});

const hasSubInfo = computed(() => {
  return seatLabel.value || metadata.value.flight_info?.carrier || metadata.value.flight_info?.pnr;
});

function formatTimeOnly(dtStr) {
  if (!dtStr) return '';
  const parts = dtStr.split(' ');
  if (parts.length > 1) {
    return parts[1].substring(0, 5); // HH:MM
  }
  return dtStr;
}
</script>

<style scoped>
.ticket-card-wrapper {
  background-color: var(--bg-tertiary);
  border: 1px solid var(--border-subtle);
  border-radius: 14px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  width: 100%;
  max-width: 400px;
  margin: 0 auto;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.1);
  color: var(--text-primary);
  font-family: 'Inter', system-ui, sans-serif;
}

.ticket-card-wrapper.is-expired {
  opacity: 0.6;
  filter: grayscale(100%);
}

.ticket-header {
  padding: 16px 20px;
  border-bottom: 2px dashed var(--border-subtle);
  background-color: var(--bg-secondary);
}

.ticket-title-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.category-icon {
  width: 20px;
  height: 20px;
  color: var(--text-secondary);
}

.ticket-title {
  font-weight: 600;
  font-size: 16px;
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ticket-status {
  font-size: 12px;
  padding: 4px 8px;
  border-radius: 12px;
  font-weight: 500;
}

.status-active {
  background-color: var(--color-success);
  color: #fff;
}

.status-expired {
  background-color: var(--bg-root);
  color: var(--text-muted);
}

.ticket-body {
  padding: 24px 20px;
}

.travel-route {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 24px;
}

.route-point {
  display: flex;
  flex-direction: column;
}

.route-point.right {
  text-align: right;
}

.route-point .time {
  font-size: 28px;
  font-weight: 700;
  line-height: 1.2;
}

.route-point .location {
  font-size: 14px;
  color: var(--text-secondary);
  margin-top: 4px;
}

.route-arrow {
  display: flex;
  flex-direction: column;
  align-items: center;
  color: var(--text-muted);
  flex: 1;
  padding: 0 16px;
}

.arrow-icon {
  width: 24px;
  height: 24px;
  margin-bottom: 4px;
}

.duration {
  font-size: 12px;
  letter-spacing: 1px;
}

.generic-info {
  display: flex;
  flex-direction: column;
  gap: 16px;
  margin-bottom: 24px;
}

.info-group label {
  font-size: 12px;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.info-group .val {
  font-size: 18px;
  font-weight: 600;
  margin-top: 4px;
}

.sub-info-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(80px, 1fr));
  gap: 16px;
  padding-top: 16px;
  border-top: 1px solid var(--border-subtle);
}

.info-cell label {
  font-size: 11px;
  color: var(--text-secondary);
  text-transform: uppercase;
  margin-bottom: 4px;
  display: block;
}

.info-cell .val {
  font-size: 14px;
  font-weight: 500;
}

.info-cell .val.highlight {
  font-size: 18px;
  font-weight: 700;
}

.ticket-footer {
  padding: 24px;
  display: flex;
  flex-direction: column;
  align-items: center;
  background-color: var(--bg-secondary);
}

.qr-container {
  position: relative;
  width: 160px;
  height: 160px;
  padding: 10px;
  background-color: var(--bg-tertiary);
  border-radius: 12px;
}

.qr-center-logo {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 32px;
  height: 32px;
  background-color: var(--bg-tertiary);
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.qr-inner-icon {
  width: 20px;
  height: 20px;
  color: var(--text-primary);
}

.barcode-raw {
  margin-top: 12px;
  font-family: monospace;
  font-size: 12px;
  color: var(--text-muted);
  letter-spacing: 2px;
}
</style>
