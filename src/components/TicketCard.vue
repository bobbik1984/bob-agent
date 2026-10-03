<template>
  <button
    type="button"
    class="pass"
    :class="[kind, { 'is-expired': isExpired }]"
    :style="{ zIndex: index + 1 }"
    @click="openDetail"
    :aria-label="`${t('ticket.open') || '查看'} ${cardTitle}，${cardDate}`"
  >
    <div class="head">
      <span class="icon">
        <svg :class="{ 'flight-symbol': kind === 'flight' }" viewBox="0 0 24 24" aria-hidden="true" v-html="svgPath"></svg>
      </span>
      <span class="identity">
        <strong>{{ cardTitle }}</strong>
        <small>{{ cardSubtitle }}</small>
      </span>
      <span class="when">{{ cardDate }}</span>
    </div>
    <div class="body-title">{{ cardTitle }}</div>
    <div class="body-sub">{{ cardSubtitle }}</div>
    <div class="foot">
      <span>{{ cardTime }}</span>
      <span>{{ statusLabel }}</span>
    </div>
  </button>

  <!-- Detail Modal -->
  <Teleport to="body">
    <div v-if="showDetail" class="veil" @click.self="closeDetail">
      <div class="detail" :class="[kind, { 'expired-detail': isExpired }]">
        <div class="head">
          <span class="icon">
            <svg :class="{ 'flight-symbol': kind === 'flight' }" viewBox="0 0 24 24" aria-hidden="true" v-html="svgPath"></svg>
          </span>
          <span class="identity">
            <strong>{{ cardTitle }}</strong>
            <small>{{ cardSubtitle }}</small>
          </span>
          <span class="when">{{ cardDate }}</span>
        </div>
        <h3>{{ cardTitle }}</h3>
        <p>{{ cardSubtitle }}</p>
        <dl>
          <div>
            <dt>{{ t('ticket.date') || '日期' }}</dt>
            <dd>{{ cardDate }}</dd>
          </div>
          <div>
            <dt>{{ t('ticket.time') || '时间' }}</dt>
            <dd>{{ cardTime }}</dd>
          </div>
          <div>
            <dt>{{ t('ticket.info') || '票据信息' }}</dt>
            <dd>{{ cardInfoDetail || cardSubtitle || '—' }}</dd>
          </div>
          <div>
            <dt>{{ t('ticket.status') || '状态' }}</dt>
            <dd>{{ statusLabel }}</dd>
          </div>
        </dl>

        <!-- QR Code Section if available -->
        <div v-if="barcodeData && isQrBarcode" class="detail-qr-section">
          <div class="detail-qr-box">
            <qrcode-vue :value="barcodeData" :size="150" level="M" />
          </div>
          <div v-if="barcodeData.length <= 40" class="detail-qr-caption">{{ barcodeData }}</div>
        </div>
        <div v-else-if="barcodeData" class="detail-barcode-text">
          {{ barcodeData }}
        </div>

        <!-- Inline Edit Form when editing -->
        <div v-if="isEditing" class="detail-edit-form">
          <div class="edit-field">
            <label>{{ t('ticket.title') || '标题' }}</label>
            <input v-model="editForm.title" class="edit-input" />
          </div>
          <div class="edit-grid-2">
            <div class="edit-field">
              <label>{{ t('ticket.date') || '日期' }}</label>
              <input v-model="editForm.date" @input="formatDateInput('date')" class="edit-input" placeholder="YYYY-MM-DD" maxlength="10" />
            </div>
            <div class="edit-field">
              <label>{{ t('ticket.time') || '时间' }}</label>
              <input v-model="editForm.time" @input="formatTimeInput('time')" class="edit-input" placeholder="HH:MM" maxlength="5" />
            </div>
          </div>
          <div class="edit-grid-2" v-if="kind === 'flight' || kind === 'rail'">
            <div class="edit-field">
              <label>出发</label>
              <input v-model="editForm.origin" class="edit-input" />
            </div>
            <div class="edit-field">
              <label>到达</label>
              <input v-model="editForm.destination" class="edit-input" />
            </div>
          </div>
          <div class="edit-grid-2">
            <div class="edit-field" v-if="kind === 'flight' || kind === 'rail'">
              <label>{{ kind === 'flight' ? '航班号' : '车次' }}</label>
              <input v-model="editForm.flight_number" class="edit-input" />
            </div>
            <div class="edit-field">
              <label>{{ t('ticket.seat') || '座位' }}</label>
              <input v-model="editForm.seat" class="edit-input" />
            </div>
          </div>
          <div class="edit-field" v-if="kind !== 'flight' && kind !== 'rail'">
            <label>{{ t('ticket.venue') || '场馆/地点' }}</label>
            <input v-model="editForm.venue" class="edit-input" />
          </div>
          <div class="edit-buttons">
            <button class="edit-submit-btn" @click="saveEdit">{{ t('common.save') || '保存' }}</button>
            <button class="edit-cancel-btn" @click="isEditing = false">{{ t('common.cancel') || '取消' }}</button>
          </div>
        </div>

        <!-- Card bottom action bar: delete and edit -->
        <div v-if="!isEditing" class="detail-actions">
          <button class="detail-act-btn detail-del-btn" @click="deleteTicket" title="删除票据">
            <Trash2 :size="15" />
          </button>
          <button class="detail-act-btn detail-edit-btn" @click="startEdit">
            编辑
          </button>
        </div>
      </div>

      <!-- Close button -->
      <button class="close" type="button" @click="closeDetail">
        {{ t('ticket.close_ticket') || '收起票据' }}
      </button>
    </div>
  </Teleport>
</template>

<script setup>
import { useDialog } from '@/composables/useDialog.js';
const { showConfirm, showAlert } = useDialog();

import { computed, ref, watch, onMounted, onUnmounted } from 'vue';
import { Trash2 } from 'lucide-vue-next';
import QrcodeVue from 'qrcode.vue';
import { useI18n } from 'vue-i18n';
import {
  ticketMetadata,
  ticketKind,
  ticketBarcode,
  ticketGroup,
  ticketCardTitle,
  ticketCardSubtitle,
  ticketFormattedDate,
  ticketFormattedTime
} from '@/tickets/wallet.js';

const props = defineProps({
  node: {
    type: Object,
    required: true
  },
  index: {
    type: Number,
    default: 0
  }
});

const { t } = useI18n();

const showDetail = ref(false);
const isEditing = ref(false);
const editForm = ref({});
const isAddedToCalendar = ref(false);

const SVG_PATHS = {
  flight: '<path d="M12 2c.55 0 1 .45 1 1v7l7.5 4v2L13 14v5l2 1.5V22l-3-1-3 1v-1.5l2-1.5v-5l-7.5 2v-2L11 10V3c0-.55.45-1 1-1Z"/>',
  rail: '<rect x="5" y="3" width="14" height="16" rx="3"/><path d="M5 10h14M8 22l2-3m6 0 2 3M9 6h1m4 0h1"/>',
  film: '<rect x="3" y="5" width="18" height="15" rx="2"/><path d="M3 10h18M8 5l3 5m3-5 3 5"/>',
  museum: '<path d="M3 9l9-5 9 5M4 10h16M6 10v9m4-9v9m4-9v9m4-9v9M3 20h18"/>'
};

const kind = computed(() => ticketKind(props.node));
const svgPath = computed(() => SVG_PATHS[kind.value] || SVG_PATHS.flight);

const cardTitle = computed(() => ticketCardTitle(props.node));
const cardSubtitle = computed(() => ticketCardSubtitle(props.node));
const cardDate = computed(() => ticketFormattedDate(props.node) || t('ticket.undated') || '待定');
const cardTime = computed(() => ticketFormattedTime(props.node) || '—');

const isExpired = computed(() => ticketGroup(props.node) === 'expired');
const statusLabel = computed(() => {
  if (isExpired.value) return t('ticket.status_expired') || '已过期';
  return t('ticket.status_upcoming') || '即将使用';
});

const metadata = computed(() => ticketMetadata(props.node));
const barcodeData = computed(() => ticketBarcode(props.node));
const isQrBarcode = computed(() => !metadata.value.barcode_type || ['qr', 'qr_code', 'qrcode'].includes(String(metadata.value.barcode_type).toLowerCase()));

const cardInfoDetail = computed(() => {
  const parts = [];
  if (metadata.value.flight_info?.flight_number) parts.push(metadata.value.flight_info.flight_number);
  if (metadata.value.flight_info?.origin_terminal) parts.push(metadata.value.flight_info.origin_terminal);
  if (metadata.value.venue) parts.push(metadata.value.venue);
  if (metadata.value.seat_info || metadata.value.flight_info?.seat) parts.push(metadata.value.seat_info || metadata.value.flight_info?.seat);
  return parts.length > 0 ? parts.join(' · ') : cardSubtitle.value;
});

const openDetail = () => {
  showDetail.value = true;
};

const closeDetail = () => {
  showDetail.value = false;
  isEditing.value = false;
};

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

const formatTimeInput = (field) => {
  let val = editForm.value[field] || '';
  val = val.replace(/\D/g, '');
  if (val.length > 4) val = val.substring(0, 4);
  if (val.length >= 3) {
    val = val.substring(0, 2) + ':' + val.substring(2, 4);
  }
  editForm.value[field] = val;
};

const checkCalendar = async () => {
  if (window.appAPI && window.appAPI.listEvents) {
    try {
      const allEvents = await window.appAPI.listEvents();
      isAddedToCalendar.value = allEvents.some(e => e.linked_ticket_id === props.node.id);
    } catch (e) {
      console.error('checkCalendar err', e);
    }
  }
};

const startEdit = () => {
  editForm.value = {
    title: cardTitle.value,
    origin: metadata.value.flight_info?.origin || '',
    destination: metadata.value.flight_info?.destination || '',
    flight_number: metadata.value.flight_info?.flight_number || '',
    seat: metadata.value.flight_info?.seat || metadata.value.seat_info || '',
    venue: metadata.value.venue || '',
    date: metadata.value.start_time ? metadata.value.start_time.split(' ')[0] : (metadata.value.date || ''),
    time: (metadata.value.start_time && metadata.value.start_time.includes(' ')) ? metadata.value.start_time.split(' ')[1].substring(0, 5) : (metadata.value.time || ''),
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

    const newMetadata = { ...metadata.value, flight_info: { ...(metadata.value.flight_info || {}) } };
    newMetadata.start_time = newStartTime;
    newMetadata.venue = editForm.value.venue;
    if (editForm.value.seat) {
      newMetadata.seat_info = editForm.value.seat;
      newMetadata.flight_info.seat = editForm.value.seat;
    }
    if (editForm.value.flight_number) {
      newMetadata.flight_info.flight_number = editForm.value.flight_number;
    }
    if (editForm.value.origin) {
      newMetadata.flight_info.origin = editForm.value.origin;
    }
    if (editForm.value.destination) {
      newMetadata.flight_info.destination = editForm.value.destination;
    }

    const newTitle = editForm.value.title || props.node.label;
    const result = await window.appAPI.kgUpdateTicket(props.node.id, newTitle, newMetadata);
    if (result?.error || result?.ok === false) throw new Error(result.error || 'Ticket update failed');

    isEditing.value = false;
    window.dispatchEvent(new CustomEvent('ticket-created'));
  } catch (e) {
    console.error('Failed to save edit', e);
  }
};

const deleteTicket = async () => {
  if (await showConfirm('确定要删除此票据吗？')) {
    try {
      await window.appAPI.kgDeleteNode(props.node.id);
      showDetail.value = false;
      window.dispatchEvent(new CustomEvent('ticket-created'));
    } catch (e) {
      console.error('Failed to delete ticket', e);
      await showAlert('删除失败');
    }
  }
};

const handleTicketOpen = (e) => {
  if (e.detail === props.node.id) {
    showDetail.value = true;
  }
};

watch(showDetail, (newVal) => {
  if (newVal) {
    checkCalendar();
  }
});

onMounted(() => {
  window.addEventListener('ticket-card-open', handleTicketOpen);
});

onUnmounted(() => {
  window.removeEventListener('ticket-card-open', handleTicketOpen);
});
</script>

<style scoped>
/* ── Deck Pass Card (1:1 with mockup) ── */
.pass {
  position: relative;
  width: 100%;
  height: 238px;
  border: 1px solid #ffffff42;
  border-radius: 22px;
  padding: 14px 16px;
  color: #fff;
  text-align: left;
  box-shadow: 0 7px 17px #10243228;
  cursor: pointer;
  transition: transform .18s ease, box-shadow .18s ease;
  overflow: hidden;
  touch-action: manipulation;
  box-sizing: border-box;
  display: block;
  font-family: inherit;
}
.pass:hover {
  transform: translateY(-3px);
  box-shadow: 0 11px 20px #10243235;
}
.pass:focus-visible {
  outline: 3px solid var(--brand, #90bafa);
  outline-offset: 2px;
}
.pass.flight { background: var(--flight, #6593bc); }
.pass.rail { background: var(--rail, #5eaa96); }
.pass.film { background: var(--film, #b57f9d); }
.pass.museum { background: var(--museum, #d29b76); }
.pass.general { background: var(--flight, #6593bc); }

/* Expired: desaturate from original category color! */
.pass.is-expired {
  filter: grayscale(.63) saturate(.77) brightness(.97);
}
.pass.is-expired:hover {
  filter: grayscale(.48) saturate(.84) brightness(.98);
}

.head {
  display: flex;
  align-items: center;
  gap: 11px;
  height: 51px;
}
.icon {
  display: grid;
  place-items: center;
  flex: none;
  width: 37px;
  height: 37px;
  border-radius: 50%;
  background: #ffffff2e;
}
.icon svg {
  width: 20px;
  height: 20px;
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.icon svg.flight-symbol {
  fill: currentColor;
  stroke: none;
}
.identity {
  min-width: 0;
  flex: 1;
}
.identity strong {
  display: block;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  font-size: 15px;
  line-height: 1.25;
  font-weight: 700;
  color: #fff;
}
.identity small {
  display: block;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  font-size: 11px;
  opacity: .8;
  margin-top: 2px;
  color: #fff;
}
.when {
  align-self: flex-start;
  padding-top: 5px;
  font-size: 11px;
  white-space: nowrap;
  opacity: .85;
  color: #fff;
}

.body-title {
  margin-top: 31px;
  font-size: 23px;
  line-height: 1.2;
  font-weight: 800;
  letter-spacing: -.04em;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  color: #fff;
}
.body-sub {
  margin-top: 9px;
  font-size: 13px;
  opacity: .88;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  color: #fff;
}
.foot {
  display: flex;
  justify-content: space-between;
  border-top: 1px solid #ffffff79;
  margin-top: 24px;
  padding-top: 11px;
  font-size: 12px;
  font-weight: 700;
  color: #fff;
}

/* ── Modal View (1:1 with mockup) ── */
.veil {
  position: fixed;
  z-index: 9999;
  inset: 0;
  background: #13212dbf;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 20px;
  overflow-y: auto;
  box-sizing: border-box;
}
.detail {
  position: relative;
  width: min(100%, 390px);
  min-height: 414px;
  border-radius: 23px;
  padding: 22px;
  color: #fff;
  box-shadow: 0 18px 38px #0e1c2e66;
  border: 1px solid #ffffff42;
  box-sizing: border-box;
  text-align: left;
}
.detail.flight { background: var(--flight, #6593bc); }
.detail.rail { background: var(--rail, #5eaa96); }
.detail.film { background: var(--film, #b57f9d); }
.detail.museum { background: var(--museum, #d29b76); }
.detail.general { background: var(--flight, #6593bc); }
.detail.expired-detail { filter: grayscale(.55) saturate(.8); }

.detail h3 {
  font-size: 25px;
  line-height: 1.18;
  letter-spacing: -.04em;
  margin: 32px 0 8px;
  font-weight: 800;
  color: #fff;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.detail p {
  margin: 0;
  opacity: .85;
  font-size: 14px;
  color: #fff;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.detail dl {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 18px;
  margin: 28px 0 0;
  padding-top: 15px;
  border-top: 1px solid #ffffff80;
}
.detail dt {
  font-size: 11px;
  opacity: .75;
  color: #fff;
}
.detail dd {
  margin: 3px 0 0;
  font-size: 14px;
  font-weight: 700;
  color: #fff;
}

.detail-qr-section {
  display: flex;
  flex-direction: column;
  align-items: center;
  margin-top: 24px;
  padding-top: 18px;
  border-top: 1px solid #ffffff42;
}
.detail-qr-box {
  background: #ffffff;
  padding: 12px;
  border-radius: 12px;
  display: inline-flex;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}
.detail-qr-caption {
  font-size: 11px;
  margin-top: 8px;
  opacity: 0.8;
  letter-spacing: 0.5px;
}
.detail-barcode-text {
  margin-top: 18px;
  padding: 8px 12px;
  background: rgba(0, 0, 0, 0.2);
  border-radius: 8px;
  font-size: 12px;
  font-family: monospace;
  text-align: center;
  word-break: break-all;
}

.detail-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  margin-top: 22px;
  padding-top: 14px;
  border-top: 1px solid #ffffff30;
}
.detail-act-btn {
  background: rgba(255, 255, 255, 0.22);
  border: 1px solid rgba(255, 255, 255, 0.35);
  color: #fff;
  border-radius: 14px;
  padding: 6px 14px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  transition: background .15s ease;
}
.detail-act-btn:hover {
  background: rgba(255, 255, 255, 0.35);
}
.detail-del-btn {
  background: rgba(220, 38, 38, 0.35);
  border-color: rgba(220, 38, 38, 0.5);
}
.detail-del-btn:hover {
  background: rgba(220, 38, 38, 0.6);
}

.close {
  border: 0;
  border-radius: 18px;
  background: #ffffff;
  color: #1a2832;
  padding: 9px 28px;
  font-size: 14px;
  font-weight: 700;
  cursor: pointer;
  margin-top: 20px;
  box-shadow: 0 4px 14px rgba(0,0,0,0.25);
  transition: transform .15s ease;
}
.close:hover {
  transform: translateY(-1px);
}

/* Edit form */
.detail-edit-form {
  margin-top: 20px;
  padding-top: 16px;
  border-top: 1px solid #ffffff40;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.edit-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.edit-field label {
  font-size: 11px;
  opacity: .8;
}
.edit-input {
  background: rgba(0, 0, 0, 0.25);
  border: 1px solid rgba(255, 255, 255, 0.3);
  border-radius: 8px;
  padding: 6px 10px;
  color: #fff;
  font-size: 13px;
  outline: none;
}
.edit-input:focus {
  border-color: #fff;
}
.edit-grid-2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.edit-buttons {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 8px;
}
.edit-submit-btn {
  background: #fff;
  color: #1a2832;
  border: none;
  border-radius: 12px;
  padding: 6px 16px;
  font-size: 13px;
  font-weight: 700;
  cursor: pointer;
}
.edit-cancel-btn {
  background: rgba(255, 255, 255, 0.2);
  color: #fff;
  border: 1px solid rgba(255, 255, 255, 0.3);
  border-radius: 12px;
  padding: 6px 14px;
  font-size: 13px;
  cursor: pointer;
}
</style>
