<template>
  <div class="ticket-card-wrapper" :class="{ 'is-expired': isExpired }" @click="showDetail = true" style="cursor: pointer;">
    <div class="ticket-header">
      <div class="ticket-title-row">
        <component :is="categoryIcon" class="category-icon" />
        <span class="ticket-title">{{ node.label }}</span>
        <span class="ticket-status" :class="ticketStatusClass"></span>
      </div>
      <div class="ticket-subtitle" v-if="metadata.start_time">
        {{ metadata.start_time.split(' ')[0] }}
      </div>
    </div>
  </div>

  <!-- Detail Modal -->
  <Teleport to="body">
    <div v-if="showDetail" class="bp-modal-overlay" @click.self="showDetail = false">
      <div class="boarding-pass-modern-card">
        <div class="bp-modern-header">
          <span class="bp-modern-icon"><component :is="categoryIcon" style="width:14px;height:14px;" /></span>
          <span>{{ isTravel ? 'Boarding Pass' : (metadata.category || 'Ticket') }}</span>
        </div>

        <div class="bp-route-row" v-if="isTravel">
          <span class="bp-airport-code">{{ originLabel }}</span>
          <span class="bp-route-arrow">→</span>
          <span class="bp-airport-code">{{ destinationLabel }}</span>
        </div>
        <div class="bp-route-row" v-else>
          <span class="bp-airport-code" style="font-size:1.3em;">{{ node.label }}</span>
        </div>

        <div class="bp-modern-divider"></div>

        <div class="bp-detail-grid">
          <div class="bp-modern-field" v-if="metadata.flight_info?.passenger_name || metadata.passenger_name">
            <div class="bp-modern-label">Passenger</div>
            <div class="bp-modern-value">{{ metadata.flight_info?.passenger_name || metadata.passenger_name }}</div>
          </div>
          <div class="bp-modern-field" v-if="metadata.flight_info?.flight_number">
            <div class="bp-modern-label">Flight</div>
            <div class="bp-modern-value">{{ metadata.flight_info.flight_number }}</div>
          </div>
          <div class="bp-modern-field" v-if="metadata.start_time">
            <div class="bp-modern-label">Date</div>
            <div class="bp-modern-value">{{ metadata.start_time.split(' ')[0] }}</div>
          </div>
          <div class="bp-modern-field" v-if="metadata.start_time && metadata.start_time.includes(' ') && metadata.start_time.split(' ')[1] !== '00:00:00'">
            <div class="bp-modern-label">Time</div>
            <div class="bp-modern-value">{{ formatTimeOnly(metadata.start_time) }}</div>
          </div>
          <div class="bp-modern-field" v-if="seatLabel">
            <div class="bp-modern-label">Seat</div>
            <div class="bp-modern-value">{{ seatLabel }}</div>
          </div>
          <div class="bp-modern-field" v-if="metadata.flight_info?.pnr">
            <div class="bp-modern-label">PNR</div>
            <div class="bp-modern-value">{{ metadata.flight_info.pnr }}</div>
          </div>
          <div class="bp-modern-field" v-if="metadata.venue && !isTravel">
            <div class="bp-modern-label">Venue</div>
            <div class="bp-modern-value">{{ metadata.venue }}</div>
          </div>
        </div>

        <div class="bp-modern-qr-section" v-if="metadata.barcode_data">
          <div class="bp-modern-qr-wrapper">
            <qrcode-vue :value="metadata.barcode_data" :size="200" level="M" />
          </div>
        </div>

        <div class="bp-modern-actions">
          <button class="bp-modern-btn bp-modern-btn-danger" @click="deleteTicket">删除</button>
          <button class="bp-modern-btn bp-modern-btn-dismiss" @click="showDetail = false">关闭</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup>
import { computed, ref } from 'vue';
import { Plane, Film, Ticket, Calendar, Train, ArrowRight, CreditCard, Music, ChevronDown } from 'lucide-vue-next';
import QrcodeVue from 'qrcode.vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  node: {
    type: Object,
    required: true
  }
});

const { t } = useI18n();

const expanded = ref(false);
const showDetail = ref(false);

const metadata = computed(() => {
  console.log("TicketCard node.label:", props.node.label);
  console.log("TicketCard raw metadata:", props.node.metadata);
  if (typeof props.node.metadata === 'string') {
    try {
      const parsed = JSON.parse(props.node.metadata);
      console.log("TicketCard parsed metadata:", parsed);
      return parsed;
    } catch (e) {
      console.error("TicketCard JSON parse error:", e);
      return {};
    }
  }
  return props.node.metadata || {};
});

const isExpired = computed(() => {
  if (!metadata.value.start_time) return false;
  // Handle both "YYYY-MM-DD HH:MM:SS" and "YYYY-MM-DD" formats
  const dtStr = metadata.value.start_time.replace(' ', 'T');
  const startTime = new Date(dtStr).getTime();
  if (isNaN(startTime)) return false;
  return Date.now() > startTime + 24 * 3600 * 1000;
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
  return isExpired.value ? 'status-expired' : 'status-active';
});

const deleteTicket = async () => {
  if (confirm('确定要删除此票据吗？')) {
    try {
      await window.appAPI.kgDeleteNode(props.node.id);
      showDetail.value = false;
      window.dispatchEvent(new CustomEvent('ticket-created')); // triggers a refresh in KnowledgeGraphView
    } catch (e) {
      console.error('Failed to delete ticket', e);
      alert('删除失败');
    }
  }
};

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
  border-radius: 12px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  width: 100%;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.08);
  color: var(--text-primary);
  font-family: 'Inter', system-ui, sans-serif;
}

.ticket-card-wrapper.is-expired {
  opacity: 0.5;
}

.ticket-card-wrapper.is-expired .category-icon {
  background: var(--text-muted, #999);
}

.ticket-subtitle {
  font-size: 12px;
  color: var(--text-tertiary);
  margin-top: 4px;
  padding-left: 33px;
}

.ticket-header {
  padding: 10px 14px;
}

.ticket-title-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.category-icon {
  width: 18px;
  height: 18px;
  padding: 5px;
  border-radius: 8px;
  background: var(--user-accent, #4f8cf7);
  color: #ffffff;
  box-sizing: content-box;
  flex-shrink: 0;
}

.ticket-title {
  font-weight: 600;
  font-size: 14px;
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ticket-status {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.status-active {
  background-color: var(--color-success);
}

.status-expired {
  background-color: var(--text-muted, #999);
}

/* ── Detail Modal (reuse ChatView boarding pass style names) ── */
.bp-modal-overlay {
  position: fixed;
  top: 0; left: 0; right: 0; bottom: 0;
  background: rgba(0,0,0,0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
  backdrop-filter: blur(4px);
}
.boarding-pass-modern-card {
  background: var(--bg-tertiary);
  color: var(--text-primary);
  border: 1px solid var(--border-default);
  border-radius: 16px;
  padding: 20px;
  width: 90%;
  max-width: 340px;
  box-shadow: 0 16px 40px rgba(0,0,0,0.3);
  animation: modalPop 0.3s cubic-bezier(0.175, 0.885, 0.32, 1.275);
  font-family: var(--font-sans, system-ui, sans-serif);
}
@keyframes modalPop {
  from { opacity: 0; transform: scale(0.9); }
  to { opacity: 1; transform: scale(1); }
}
.bp-modern-header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 0.8em;
  font-weight: 500;
  opacity: 0.7;
  margin-bottom: 8px;
  justify-content: flex-end;
}
.bp-modern-icon {
  display: flex;
  align-items: center;
}
.bp-route-row {
  display: flex;
  align-items: baseline;
  gap: 12px;
  margin-bottom: 4px;
}
.bp-airport-code {
  font-size: 1.6em;
  font-weight: 700;
  letter-spacing: 1px;
}
.bp-route-arrow {
  font-size: 1.1em;
  opacity: 0.5;
}
.bp-detail-grid {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 12px 8px;
}
.bp-modern-field {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.bp-modern-label {
  font-size: 0.7em;
  opacity: 0.6;
  letter-spacing: 0.3px;
}
.bp-modern-value {
  font-size: 1.05em;
  font-weight: 600;
}
.bp-modern-divider {
  height: 1px;
  background: var(--border-default);
  margin: 10px 0;
}
.bp-modern-qr-section {
  margin-top: 16px;
  display: flex;
  flex-direction: column;
  align-items: center;
}
.bp-modern-qr-wrapper {
  background: #ffffff;
  padding: 10px;
  border-radius: 10px;
}
.bp-modern-actions {
  display: flex;
  gap: 10px;
  margin-top: 16px;
}
.bp-modern-btn {
  flex: 1;
  padding: 10px;
  border-radius: 8px;
  border: none;
  font-weight: 600;
  cursor: pointer;
  transition: opacity 0.2s;
  font-size: 0.9em;
}
.bp-modern-btn:hover {
  opacity: 0.85;
}
.bp-modern-btn-dismiss {
  background: var(--bg-secondary);
  color: var(--text-secondary);
}
.bp-modern-btn-danger {
  background: var(--color-error, #f44336);
  color: #ffffff;
  flex: 0.5;
}
</style>
