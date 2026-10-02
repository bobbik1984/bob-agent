<template>
  <button class="ticket-card-wrapper" :class="[`ticket-${category}`, { 'is-expired': isExpired }]" type="button" @click="showDetail = true" :aria-label="`${t('ticket.open')} ${node.label}`">
    <div class="ticket-card-top">
      <component :is="categoryIcon" class="category-icon" :aria-label="t(`ticket.category_${category}`)" role="img" />
      <span class="ticket-card-date">{{ ticketDateLabel }}</span>
    </div>
    <div class="ticket-card-main">
      <strong class="ticket-card-title">{{ isTravel && originLabel && destinationLabel ? `${originLabel} → ${destinationLabel}` : node.label }}</strong>
      <span class="ticket-card-subtitle">{{ isTravel ? (metadata.flight_info?.flight_number || node.label) : (metadata.venue || seatLabel || node.label) }}</span>
    </div>
  </button>

  <!-- Detail Modal -->
  <Teleport to="body">
    <div v-if="showDetail" class="bp-modal-overlay" @click.self="showDetail = false">
      <div class="boarding-pass-modern-card">
        <div class="bp-modern-header">
          <span class="bp-modern-icon"><component :is="categoryIcon" style="width:14px;height:14px;" /></span>
          <span>{{ node.label }}</span>
        </div>

        <div class="bp-route-row" v-if="isTravel">
          <div class="bp-airport-group is-origin">
            <input v-if="isEditing" v-model="editForm.origin" class="bp-edit-input bp-airport-code" style="width: 85px; text-align: center;" />
            <span v-else class="bp-airport-code">{{ originLabel }}</span>
            <input v-if="isEditing" v-model="editForm.originTerminal" class="bp-edit-input" placeholder="Terminal" style="width: 85px; font-size: 1.05em; text-align: center; height: 26px; box-sizing: border-box;" />
            <div v-else class="bp-terminal" style="height: 26px; line-height: 26px;">{{ metadata.flight_info?.origin_terminal || '' }}</div>
            
            <input type="text" v-if="isEditing" v-model="editForm.date" @input="formatDateInput('date')" class="bp-edit-input" placeholder="YYYY-MM-DD" maxlength="10" style="width: 115px; font-size: 0.85em; text-align: center; padding: 2px; height: 22px; box-sizing: border-box;" />
            <div v-else style="font-size: 0.85em; opacity: 0.7; height: 22px; line-height: 22px; text-align: center;">{{ metadata.start_time ? metadata.start_time.split(' ')[0] : '' }}</div>
            
            <input type="text" v-if="isEditing" v-model="editForm.time" @input="formatTimeInput('time')" class="bp-edit-input" placeholder="HH:MM" maxlength="5" style="width: 90px; font-size: 1.1em; font-weight: 500; text-align: center; padding: 2px; height: 26px; box-sizing: border-box;" />
            <div v-else style="font-size: 1.2em; font-weight: 600; height: 26px; line-height: 26px; text-align: center;">{{ (metadata.start_time && metadata.start_time.includes(' ') && metadata.start_time.split(' ')[1] !== '00:00:00') ? formatTimeOnly(metadata.start_time) : '' }}</div>
          </div>
          
          <div class="bp-route-center">
            <component :is="categoryIcon" style="width: 20px; height: 20px;" />
          </div>
          
          <div class="bp-airport-group is-destination">
            <input v-if="isEditing" v-model="editForm.destination" class="bp-edit-input bp-airport-code" style="width: 85px; text-align: center;" />
            <span v-else class="bp-airport-code">{{ destinationLabel }}</span>
            <input v-if="isEditing" v-model="editForm.destinationTerminal" class="bp-edit-input" placeholder="Terminal" style="width: 85px; font-size: 1.05em; text-align: center; height: 26px; box-sizing: border-box;" />
            <div v-else class="bp-terminal" style="height: 26px; line-height: 26px;">{{ metadata.flight_info?.destination_terminal || '' }}</div>
            
            <input type="text" v-if="isEditing" v-model="editForm.endDate" @input="formatDateInput('endDate')" class="bp-edit-input" placeholder="YYYY-MM-DD" maxlength="10" style="width: 115px; font-size: 0.85em; text-align: center; padding: 2px; height: 22px; box-sizing: border-box;" />
            <div v-else style="font-size: 0.85em; opacity: 0.7; height: 22px; line-height: 22px; text-align: center;">{{ metadata.end_time ? metadata.end_time.split(' ')[0] : '' }}</div>
            
            <input type="text" v-if="isEditing" v-model="editForm.endTime" @input="formatTimeInput('endTime')" class="bp-edit-input" placeholder="HH:MM" maxlength="5" style="width: 90px; font-size: 1.1em; font-weight: 500; text-align: center; padding: 2px; height: 26px; box-sizing: border-box;" />
            <div v-else style="font-size: 1.2em; font-weight: 600; height: 26px; line-height: 26px; text-align: center;">{{ (metadata.end_time && metadata.end_time.includes(' ') && metadata.end_time.split(' ')[1] !== '00:00:00') ? formatTimeOnly(metadata.end_time) : '' }}</div>
          </div>
        </div>
        <div class="bp-route-row" v-else style="display: flex; flex-direction: column; gap: 8px;">
          <input v-if="isEditing" v-model="editForm.title" class="bp-edit-input bp-airport-code" style="width: 100%;" />
          <span v-else class="bp-airport-code" style="font-size:1.6em; white-space: normal; height: auto; line-height: 1.2;">{{ node.label }}</span>
        </div>

        <div class="bp-modern-divider"></div>

        <div class="bp-detail-grid">
          <div class="bp-modern-field" v-if="isTravel || metadata.passenger_name || isEditing">
            <div class="bp-modern-label">{{ $t('ticket.passenger') || 'Passenger' }}</div>
            <input v-if="isEditing" v-model="editForm.passenger_name" class="bp-edit-input" />
            <div v-else class="bp-modern-value">{{ metadata.flight_info?.passenger_name || metadata.passenger_name || '' }}</div>
          </div>
          <div class="bp-modern-field" v-if="isTravel">
            <div class="bp-modern-label">{{ $t('ticket.flight') || 'Flight' }}</div>
            <input v-if="isEditing" v-model="editForm.flight_number" class="bp-edit-input" />
            <div v-else class="bp-modern-value">{{ metadata.flight_info?.flight_number || '' }}</div>
          </div>
          <div class="bp-modern-field" v-if="!isTravel">
            <div class="bp-modern-label">{{ $t('ticket.date') || 'Date' }}</div>
            <input type="text" v-if="isEditing" v-model="editForm.date" @input="formatDateInput('date')" class="bp-edit-input" placeholder="YYYY-MM-DD" maxlength="10" />
            <div v-else class="bp-modern-value">{{ metadata.start_time ? metadata.start_time.split(' ')[0] : '' }}</div>
          </div>
          <div class="bp-modern-field" v-if="!isTravel">
            <div class="bp-modern-label">{{ $t('ticket.time') || 'Time' }}</div>
            <input type="text" v-if="isEditing" v-model="editForm.time" @input="formatTimeInput('time')" class="bp-edit-input" placeholder="HH:MM" maxlength="5" />
            <div v-else class="bp-modern-value">{{ (metadata.start_time && metadata.start_time.includes(' ') && metadata.start_time.split(' ')[1] !== '00:00:00') ? formatTimeOnly(metadata.start_time) : '' }}</div>
          </div>
          <div class="bp-modern-field">
            <div class="bp-modern-label">{{ $t('ticket.seat') || 'Seat' }}</div>
            <input v-if="isEditing" v-model="editForm.seat" class="bp-edit-input" />
            <div v-else class="bp-modern-value">{{ seatLabel || '' }}</div>
          </div>
          <div class="bp-modern-field" v-if="isTravel">
            <div class="bp-modern-label">{{ $t('ticket.pnr') || 'PNR' }}</div>
            <input v-if="isEditing" v-model="editForm.pnr" class="bp-edit-input" />
            <div v-else class="bp-modern-value">{{ metadata.flight_info?.pnr || '' }}</div>
          </div>
          <div class="bp-modern-field" v-if="!isTravel" style="grid-column: span 3; align-items: flex-start;">
            <div class="bp-modern-label">{{ $t('ticket.venue') || 'Venue' }}</div>
            <input v-if="isEditing" v-model="editForm.venue" class="bp-edit-input" style="text-align: left;" />
            <div v-else class="bp-modern-value" style="text-align: left;">{{ metadata.venue || '' }}</div>
          </div>
        </div>

        <div class="bp-modern-qr-section" v-if="barcodeData && isQrBarcode">
          <div class="bp-modern-qr-wrapper">
            <qrcode-vue :value="barcodeData" :size="200" level="M" />
          </div>
        </div>
        <p v-else-if="barcodeData" class="bp-barcode-notice">{{ t('ticket.other_barcode_format') }}</p>
        <p v-else class="bp-barcode-notice">{{ t('ticket.barcode_not_saved') }}</p>

        <div class="bp-modern-actions">
          <button class="bp-modern-btn bp-modern-btn-danger" @click="deleteTicket" title="删除">
            <Trash2 style="width:16px;height:16px;" />
          </button>
          <button v-if="!isEditing" class="bp-modern-btn bp-modern-btn-primary" @click="startEdit">编辑</button>
          <button v-if="isEditing" class="bp-modern-btn bp-modern-btn-primary" @click="saveEdit">保存</button>
          <button class="bp-modern-btn bp-modern-btn-dismiss" @click="showDetail = false">关闭</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup>
import { useDialog } from '@/composables/useDialog.js';
const { showConfirm, showAlert, showPrompt } = useDialog();

import { computed, ref, watch, onMounted, onUnmounted } from 'vue';
import { PlaneTakeoff, Film, Ticket, Calendar, Train, ArrowRight, CreditCard, Music, ChevronDown, Trash2 } from 'lucide-vue-next';
import QrcodeVue from 'qrcode.vue';
import { useI18n } from 'vue-i18n';
import { ticketMetadata, ticketCategory, ticketBarcode, ticketGroup } from '@/tickets/wallet.js';

const props = defineProps({
  node: {
    type: Object,
    required: true
  }
});

const { t } = useI18n();

const expanded = ref(false);
const showDetail = ref(false);
const isEditing = ref(false);
const editForm = ref({});
const isAddedToCalendar = ref(false);

const formatDateInput = (field) => {
  let val = editForm.value[field] || '';
  val = val.replace(/\D/g, ''); 
  if (val.length > 8) val = val.substring(0, 8);
  if (val.length >= 7) {
    val = val.substring(0, 4) + '-' + val.substring(4, 6) + '-' + val.substring(6, 8);
  } else if (val.length >= 5) {
    val = val.substring(0, 4) + '-' + val.substring(4, 6);
  }
  editForm.value[field] = val;
};

const formatTimeInput = async (field) => {
  let val = editForm.value[field] || '';
  val = val.replace(/\D/g, ''); 
  if (val.length > 4) val = val.substring(0, 4);
  if (val.length >= 3) {
    val = val.substring(0, 2) + ':' + val.substring(2, 4);
  }
  editForm.value[field] = val;
};

const checkCalendar = async () => {
  if (window.appAPI.listEvents) {
    try {
      const allEvents = await window.appAPI.listEvents();
      isAddedToCalendar.value = allEvents.some(e => e.linked_ticket_id === props.node.id);
    } catch(e) {
      console.error("checkCalendar err", e);
    }
  }
};

const addToCalendar = async () => {
  if (isAddedToCalendar.value) return;
  try {
    const payload = {
      title: props.node.label,
      type: 'event',
      status: 'pending',
      date: metadata.value.start_time ? metadata.value.start_time.split(' ')[0] : '',
      startTime: metadata.value.start_time || '',
      endTime: metadata.value.end_time || '',
      linked_ticket_id: props.node.id
    };
    await window.appAPI.confirmEvent(payload);
    isAddedToCalendar.value = true;
  } catch(e) {
    console.error("Failed to add to calendar", e);
  }
};

const startEdit = async () => {
  editForm.value = {
    title: props.node.label,
    origin: originLabel.value,
    originTerminal: metadata.value.flight_info?.origin_terminal || '',
    destination: destinationLabel.value,
    destinationTerminal: metadata.value.flight_info?.destination_terminal || '',
    date: metadata.value.start_time ? metadata.value.start_time.split(' ')[0] : '',
    time: (metadata.value.start_time && metadata.value.start_time.includes(' ')) ? metadata.value.start_time.split(' ')[1].substring(0,5) : '',
    endDate: metadata.value.end_time ? metadata.value.end_time.split(' ')[0] : '',
    endTime: (metadata.value.end_time && metadata.value.end_time.includes(' ')) ? metadata.value.end_time.split(' ')[1].substring(0,5) : '',
    flight_number: metadata.value.flight_info?.flight_number || '',
    passenger_name: metadata.value.flight_info?.passenger_name || metadata.value.passenger_name || '',
    seat: seatLabel.value,
    pnr: metadata.value.flight_info?.pnr || '',
    venue: metadata.value.venue || '',
    barcode_data: ticketBarcode(props.node)
  };
  isEditing.value = true;
};

const saveEdit = async () => {
  try {
    let newStartTime = editForm.value.date;
    if (editForm.value.time) {
      newStartTime += ' ' + editForm.value.time + ':00';
    } else if (newStartTime) {
      newStartTime += ' 00:00:00';
    }
    
    let newEndTime = editForm.value.endDate;
    if (editForm.value.endTime) {
      newEndTime += ' ' + editForm.value.endTime + ':00';
    } else if (newEndTime) {
      newEndTime += ' 00:00:00';
    }

    let newMetadata = { ...metadata.value, flight_info: { ...(metadata.value.flight_info || {}) } };
    newMetadata.start_time = newStartTime;
    newMetadata.end_time = newEndTime;
    newMetadata.venue = editForm.value.venue;
    if (metadata.value.passenger_name !== undefined) newMetadata.passenger_name = editForm.value.passenger_name;
    
    if (isTravel.value) {
      if (!newMetadata.flight_info) newMetadata.flight_info = {};
      newMetadata.flight_info.origin = editForm.value.origin;
      newMetadata.flight_info.origin_terminal = editForm.value.originTerminal;
      newMetadata.flight_info.destination = editForm.value.destination;
      newMetadata.flight_info.destination_terminal = editForm.value.destinationTerminal;
      newMetadata.flight_info.flight_number = editForm.value.flight_number;
      newMetadata.flight_info.passenger_name = editForm.value.passenger_name;
      newMetadata.flight_info.seat = editForm.value.seat;
      newMetadata.flight_info.pnr = editForm.value.pnr;
      newMetadata.seat_info = editForm.value.seat;
    }
    
    const newTitle = editForm.value.title || props.node.label;
    const result = await window.appAPI.kgUpdateTicket(props.node.id, newTitle, newMetadata);
    if (result?.error || result?.ok === false) throw new Error(result.error || 'Ticket update failed');
    
    if (isAddedToCalendar.value && window.appAPI.listEvents) {
      try {
        const allEvents = await window.appAPI.listEvents();
        const event = allEvents.find(e => e.linked_ticket_id === props.node.id);
        if (event && window.appAPI.deleteEvent) {
          await window.appAPI.deleteEvent(event.id);
          const payload = {
            title: newTitle,
            type: 'event',
            status: 'pending',
            linked_ticket_id: props.node.id,
            date: newStartTime ? newStartTime.split(' ')[0] : '',
            startTime: newStartTime || '',
            endTime: newEndTime || ''
          };
          await window.appAPI.confirmEvent(payload);
        }
      } catch (e) {
        console.error("Failed to sync calendar on edit", e);
      }
    }
    
    isEditing.value = false;
    window.dispatchEvent(new CustomEvent('ticket-created'));
  } catch (e) {
    console.error('Failed to save edit', e);
  }
};

  watch(showDetail, (newVal) => {
    if (newVal) {
      checkCalendar();
    }
  });

const metadata = computed(() => ticketMetadata(props.node));
const category = computed(() => ticketCategory(props.node));
const barcodeData = computed(() => ticketBarcode(props.node));
const isQrBarcode = computed(() => !metadata.value.barcode_type || ['qr', 'qr_code', 'qrcode'].includes(String(metadata.value.barcode_type).toLowerCase()));
const ticketDateLabel = computed(() => formatDateTime(metadata.value.start_time || metadata.value.date) || t('ticket.undated'));
const isExpired = computed(() => ticketGroup(props.node) === 'expired');

const isTravel = computed(() => {
  return category.value === 'flight' || category.value === 'train';
});

const categoryIcon = computed(() => {
  switch (category.value) {
    case 'flight': return PlaneTakeoff;
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
  if (await showConfirm('确定要删除此票据吗？')) {
    try {
      await window.appAPI.kgDeleteNode(props.node.id);
      showDetail.value = false;
      window.dispatchEvent(new CustomEvent('ticket-created')); // triggers a refresh in KnowledgeGraphView
    } catch (e) {
      console.error('Failed to delete ticket', e);
      await showAlert('删除失败');
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

const formatDateTime = (timeStr) => {
  if (!timeStr) return '';
  const parts = timeStr.split(' ');
  if (parts.length > 1 && parts[1] !== '00:00:00') {
    return `${parts[0]} ${parts[1].substring(0, 5)}`;
  }
  return parts[0];
};

const formatTimeOnly = (timeStr) => {
  if (!timeStr) return '';
  const parts = timeStr.split(' ');
  if (parts.length > 1) {
    return parts[1].substring(0, 5);
  }
  return '';
};

const handleTicketOpen = (e) => {
  if (e.detail === props.node.id) {
    showDetail.value = true;
  }
};

onMounted(() => {
  window.addEventListener('ticket-card-open', handleTicketOpen);
});

onUnmounted(() => {
  window.removeEventListener('ticket-card-open', handleTicketOpen);
});
</script>

<style scoped>
.ticket-card-wrapper {
  background: var(--ticket-general);
  border: 1px solid var(--ticket-edge);
  border-radius: 18px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  width: 100%;
  height: 190px;
  padding: 18px 20px 22px;
  box-shadow: 0 8px 20px var(--ticket-shadow);
  color: var(--ticket-ink);
  font-family: 'Inter', system-ui, sans-serif;
  text-align: left;
  cursor: pointer;
  transition: transform 180ms ease, box-shadow 180ms ease;
}

.ticket-card-wrapper:hover { transform: translateY(-3px); }
.ticket-card-wrapper:focus-visible { outline: 3px solid var(--user-accent); outline-offset: 3px; }
.ticket-flight { background: var(--ticket-flight); }
.ticket-train { background: var(--ticket-train); }
.ticket-movie { background: var(--ticket-movie); }
.ticket-exhibition { background: var(--ticket-exhibition); }
.ticket-concert { background: var(--ticket-concert); }
.ticket-membership { background: var(--ticket-membership); }
.ticket-card-wrapper.is-expired {
  background: var(--ticket-expired);
  color: var(--ticket-expired-ink);
}

.ticket-card-top { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.ticket-card-top .category-icon { width: 27px; height: 27px; padding: 0; background: none; color: inherit; }
.ticket-card-date { font-size: 12px; font-weight: 600; letter-spacing: .02em; }
.ticket-card-main { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.ticket-card-title { font-size: clamp(19px, 4vw, 25px); line-height: 1.2; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ticket-card-subtitle { font-size: 13px; opacity: .85; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
@media (prefers-reduced-motion: reduce) {
  .ticket-card-wrapper { transition: none; }
  .ticket-card-wrapper:hover { transform: none; }
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
  border-radius: var(--radius-default);
  background: var(--user-accent, #4f8cf7);
  color: var(--text-primary);
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
  border-radius: var(--radius-default);
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
  justify-content: flex-start;
}
.bp-modern-icon {
  display: flex;
  align-items: center;
}
.bp-route-row {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: start;
  gap: 12px;
  margin-top: 10px;
  margin-bottom: 24px;
}
.bp-route-center {
  opacity: 0.5;
  margin-top: 6px;
}
.bp-airport-code {
  font-size: 1.6em;
  font-weight: 700;
  letter-spacing: 1px;
  height: 34px;
  line-height: 34px;
  display: inline-block;
}
.bp-route-arrow {
  font-size: 1.1em;
  opacity: 0.5;
}
.bp-detail-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 20px 12px;
}
.bp-modern-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-height: 48px;
}
.bp-modern-field:nth-child(3n+1) {
  align-items: flex-start;
}
.bp-modern-field:nth-child(3n+1) .bp-modern-value,
.bp-modern-field:nth-child(3n+1) .bp-edit-input {
  text-align: left;
}
.bp-modern-field:nth-child(3n+2) {
  align-items: center;
}
.bp-modern-field:nth-child(3n+2) .bp-modern-value,
.bp-modern-field:nth-child(3n+2) .bp-edit-input {
  text-align: center;
}
.bp-modern-field:nth-child(3n) {
  align-items: flex-end;
}
.bp-modern-field:nth-child(3n) .bp-modern-value,
.bp-modern-field:nth-child(3n) .bp-edit-input {
  text-align: right;
}
.bp-modern-label {
  font-size: 0.75em;
  opacity: 0.7;
  letter-spacing: 0.3px;
}
.bp-modern-value {
  font-size: 1.15em;
  font-weight: 700;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  height: 28px;
  line-height: 28px;
  width: 100%;
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
.bp-barcode-notice { margin-top: 16px; color: var(--text-secondary); font-size: 12px; line-height: 1.5; text-align: center; }
.bp-modern-qr-wrapper {
  background: var(--text-primary);
  padding: 10px;
  border-radius: var(--radius-default);
}
.bp-modern-actions {
  display: flex;
  gap: 10px;
  margin-top: 16px;
}
.bp-modern-btn {
  flex: 1;
  padding: 10px;
  border-radius: var(--radius-default);
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
  color: var(--text-primary);
  flex: none;
  width: 44px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.bp-edit-input {
  background: var(--bg-primary);
  border: 1px solid var(--border-default);
  color: var(--text-primary);
  border-radius: var(--radius-default);
  padding: 4px 8px;
  font-size: 1.05em;
  width: 100%;
  box-sizing: border-box;
  font-family: inherit;
  height: 28px;
}
.bp-edit-input:focus {
  outline: none;
  border-color: var(--accent-primary);
}
.bp-modern-btn-primary {
  background: var(--accent-primary);
  color: var(--text-primary);
}


.bp-airport-group {
  display: grid;
  grid-template-rows: 34px 30px 32px 30px;
  align-items: center;
}
.bp-airport-group.is-origin {
  justify-items: start;
}
.bp-airport-group.is-destination {
  justify-items: end;
}
.bp-terminal {
  font-size: 1.1em;
  font-weight: 500;
  opacity: 0.7;
  margin-top: 4px;
}

</style>
