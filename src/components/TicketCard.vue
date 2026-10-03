<template>
  <button
    type="button"
    class="pass new-pass"
    :class="[kind, { 'is-expired': isExpired }]"
    :style="{ zIndex: index + 1 }"
    @click="openDetail"
    :aria-label="`${t('ticket.open') || '查看'} ${cardTitle}，${cardDateWithWeekday}`"
  >
    <!-- 72px 2-Row Symmetrical Header (100% visible when stacked in 82px step) -->
    <div class="new-pass-head">
      <div class="new-pass-head-row1">
        <div class="new-pass-id-group">
          <span class="new-icon-badge" :class="kind">
            <svg :class="{ 'flight-symbol': kind === 'flight' }" viewBox="0 0 24 24" aria-hidden="true" v-html="svgPath"></svg>
          </span>
          <span class="new-pass-title">{{ cardTitle }}</span>
        </div>
        <span class="new-pass-date">{{ cardDateWithWeekday }}</span>
      </div>
      <div class="new-pass-head-row2">
        <span class="new-pass-sub">{{ richSubtitle }}</span>
        <span class="new-pass-tag" :class="{ 'is-exp': isExpired }">
          <span class="status-dot-mini"></span>
          {{ subStatusLabel }}
        </span>
      </div>
    </div>

    <!-- Route Timeline Track -->
    <div class="new-pass-route">
      <div class="route-row">
        <span class="route-station">{{ routeOrigin }}</span>
        <span class="route-station">{{ routeDestination }}</span>
      </div>
      <div class="route-times">
        <span class="route-time">{{ cardTime }}</span>
        <div class="route-track">
          <span class="track-dot"></span>
          <span class="track-line"></span>
          <span class="track-duration-pill">⏱ {{ routeDuration }}</span>
          <span class="track-line"></span>
          <span class="track-dot"></span>
        </div>
        <span class="route-time">{{ routeArrivalTime }}</span>
      </div>
      <div class="route-status-row">
        <span class="status-pill-green" :class="{ 'is-exp-text': isExpired }">
          {{ isExpired ? ($t('ticket.completed') || '已结束') : ($t('ticket.on_time') || '准点运行') }}
        </span>
      </div>
    </div>

    <!-- Foot Row -->
    <div class="new-pass-foot">
      <div>
        <span>{{ $t('ticket.seat') || '座位' }}: <strong>{{ seatDisplay }}</strong></span>
        <span class="foot-sep" v-if="coachDisplay">|</span>
        <span v-if="coachDisplay">{{ coachDisplay }}</span>
      </div>
      <span class="new-pass-foot-sub">{{ platformDisplay }}</span>
    </div>
  </button>

  <!-- Detail Modal -->
  <Teleport to="body">
    <div v-if="showDetail" class="veil" @click.self="closeDetail">
      <div class="ins-modal-card" :class="[kind, { 'is-expired-modal': isExpired }]">

        <!-- Upper Section: Pure White Surface -->
        <div class="ins-upper">
          <!-- Symmetrical 2-Row Header: Left Title+Sub, Right Date+Status -->
          <div class="ins-header">
            <div class="ins-header-left">
              <span class="new-icon-badge modal-badge" :class="kind">
                <svg :class="{ 'flight-symbol': kind === 'flight' }" viewBox="0 0 24 24" aria-hidden="true" v-html="svgPath"></svg>
              </span>
              <div class="ins-header-text">
                <div class="ins-title">{{ cardTitle }}</div>
                <div class="ins-sub">{{ richSubtitle }}</div>
              </div>
            </div>
            <div class="ins-header-right">
              <div class="ins-date">{{ cardDateWithWeekday }}</div>
              <div class="ins-status-badge" :class="{ 'is-exp': isExpired }">
                <span class="status-dot-mini"></span>
                {{ subStatusLabel }}
              </div>
            </div>
          </div>

          <!-- Journey Stations -->
          <div class="ins-stations-row">
            <span>{{ routeOrigin }}</span>
            <span>{{ routeDestination }}</span>
          </div>

          <!-- Times & Track -->
          <div class="ins-times-row">
            <span class="ins-large-time">{{ cardTime }}</span>
            <div class="ins-track-bar">
              <span class="ins-track-dot"></span>
              <span class="ins-track-dashed"></span>
              <span class="ins-duration-capsule">⏱ {{ routeDuration }}</span>
              <span class="ins-track-dashed"></span>
              <span class="ins-track-dot"></span>
            </div>
            <span class="ins-large-time">{{ routeArrivalTime }}</span>
          </div>

          <!-- Status Track (Single Clean Indicator Dot) -->
          <div class="ins-on-time-status" :class="{ 'is-expired-status': isExpired }">
            {{ isExpired ? ($t('ticket.completed') || '已结束') : ($t('ticket.on_time') || '准点运行') }}
          </div>

          <!-- Passenger & Seat Grid -->
          <div class="ins-meta-row">
            <div class="ins-meta-col">
              <small>{{ $t('ticket.passenger') || '乘客' }}</small>
              <strong>{{ passengerDisplay }}</strong>
            </div>
            <div class="ins-seat-grid">
              <div class="ins-meta-col">
                <small>{{ $t('ticket.seat') || '座位' }}</small>
                <strong>{{ seatDisplay }}</strong>
              </div>
              <div class="ins-meta-col">
                <small>{{ spaceLabel }}</small>
                <strong>{{ coachDisplay }}</strong>
              </div>
            </div>
          </div>
        </div>

        <!-- Lower Section: Action & QR Bay (Clean Straight Edge, No Rainbow Bar) -->
        <div class="ins-lower">
          <div class="ins-action-col">
            <div class="platform-pill" v-if="platformDisplay">{{ platformDisplay }}</div>
            <div class="ins-boarding-title">{{ subStatusLabel }}!</div>
            <div class="ins-countdown">
              <span v-if="isExpired">{{ $t('ticket.status') || '状态' }} <b>{{ $t('ticket.completed') || '已结束' }}</b></span>
              <span v-else>{{ $t('ticket.status') || '状态' }} <b>{{ statusLabel }}</b></span>
            </div>
          </div>

          <div class="ins-qr-col">
            <div class="ins-qr-frame">
              <qrcode-vue v-if="barcodeData && isQrBarcode" :value="barcodeData" :size="84" level="M" />
              <div v-else-if="barcodeData" class="ins-barcode-text">{{ barcodeData }}</div>
              <svg v-else viewBox="0 0 100 100" fill="#0f172a" class="qr-placeholder" v-html="qrPlaceholderSvg"></svg>
            </div>
            <span class="ins-qr-tip">{{ barcodeData ? ($t('ticket.tap_qr') || '点击查看大图二维码') : ($t('ticket.info') || '票据核验码') }}</span>
          </div>
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
          <div class="edit-grid-2" v-if="kind === 'flight' || kind === 'rail'">
            <div class="edit-field">
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
          <button class="detail-act-btn detail-del-btn" @click="deleteTicket" :title="t('common.delete') || '删除票据'">
            <Trash2 :size="14" />
          </button>
          <button class="detail-act-btn detail-edit-btn" @click="startEdit">
            {{ t('common.edit') || '编辑' }}
          </button>
        </div>
      </div>

      <!-- Close button -->
      <button class="modal-close-btn" type="button" @click="closeDetail">
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
  ticketFormattedDateWithWeekday,
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
  museum: '<path d="M3 9l9-5 9 5M4 10h16M6 10v9m4-9v9m4-9v9m4-9v9M3 20h18"/>',
  general: '<rect x="5" y="3" width="14" height="16" rx="3"/><path d="M5 10h14M8 22l2-3m6 0 2 3M9 6h1m4 0h1"/>'
};

const qrPlaceholderSvg = `
<rect x="0" y="0" width="30" height="30" rx="4" fill="none" stroke="#0f172a" stroke-width="6"/>
<rect x="8" y="8" width="14" height="14" rx="2"/>
<rect x="70" y="0" width="30" height="30" rx="4" fill="none" stroke="#0f172a" stroke-width="6"/>
<rect x="78" y="8" width="14" height="14" rx="2"/>
<rect x="0" y="70" width="30" height="30" rx="4" fill="none" stroke="#0f172a" stroke-width="6"/>
<rect x="8" y="78" width="14" height="14" rx="2"/>
<rect x="42" y="10" width="8" height="8"/>
<rect x="42" y="30" width="12" height="6"/>
<rect x="10" y="44" width="8" height="14"/>
<rect x="30" y="44" width="16" height="8"/>
<rect x="60" y="44" width="14" height="8"/>
<rect x="82" y="44" width="10" height="14"/>
<rect x="40" y="65" width="10" height="12"/>
<rect x="60" y="65" width="14" height="14"/>
<rect x="80" y="70" width="12" height="8"/>
<rect x="75" y="85" width="16" height="8"/>
`;

const kind = computed(() => ticketKind(props.node));
const svgPath = computed(() => SVG_PATHS[kind.value] || SVG_PATHS.flight);

const cardTitle = computed(() => ticketCardTitle(props.node));
const cardSubtitle = computed(() => ticketCardSubtitle(props.node));
const cardDate = computed(() => ticketFormattedDate(props.node) || t('ticket.undated') || '待定');
const cardDateWithWeekday = computed(() => ticketFormattedDateWithWeekday(props.node) || t('ticket.undated') || '待定');
const cardTime = computed(() => ticketFormattedTime(props.node) || '—');

const isExpired = computed(() => ticketGroup(props.node) === 'expired');
const statusLabel = computed(() => {
  if (isExpired.value) return t('ticket.status_expired') || '已过期';
  return t('ticket.status_upcoming') || '即将使用';
});

const subStatusLabel = computed(() => {
  if (isExpired.value) return t('ticket.completed') || '已结束';
  if (kind.value === 'flight') return t('ticket.starting') || '开始登机';
  if (kind.value === 'rail') return t('ticket.boarding_now') || '正在检票';
  if (kind.value === 'film') return t('ticket.can_enter') || '可入场';
  return t('ticket.status_upcoming') || '待使用';
});

const metadata = computed(() => ticketMetadata(props.node));
const barcodeData = computed(() => ticketBarcode(props.node));
const isQrBarcode = computed(() => !metadata.value.barcode_type || ['qr', 'qr_code', 'qrcode'].includes(String(metadata.value.barcode_type).toLowerCase()));

const routeOrigin = computed(() => {
  if (metadata.value.flight_info?.origin) return metadata.value.flight_info.origin;
  if (metadata.value.venue && metadata.value.venue.includes('-')) return metadata.value.venue.split('-')[0].trim();
  if (cardTitle.value.includes('→')) return cardTitle.value.split('→')[0].trim();
  return metadata.value.venue || cardTitle.value;
});

const routeDestination = computed(() => {
  if (metadata.value.flight_info?.destination) return metadata.value.flight_info.destination;
  if (metadata.value.venue && metadata.value.venue.includes('-')) return metadata.value.venue.split('-')[1].trim();
  if (cardTitle.value.includes('→')) return cardTitle.value.split('→')[1].trim();
  if (kind.value === 'film') {
    return metadata.value.end_time ? '散场 ' + (metadata.value.end_time.includes(' ') ? metadata.value.end_time.split(' ')[1].substring(0, 5) : '') : '散场';
  }
  if (kind.value === 'museum') {
    return metadata.value.end_time ? '闭馆 ' + (metadata.value.end_time.includes(' ') ? metadata.value.end_time.split(' ')[1].substring(0, 5) : '') : '闭馆';
  }
  return '到达';
});

const routeArrivalTime = computed(() => {
  if (metadata.value.end_time) {
    return ticketFormattedTime({ metadata: { date: metadata.value.end_time } }) || '—';
  }
  if (cardTime.value !== '—') {
    // If arrival time not explicitly set, calculate fallback offset (e.g. +1h)
    const [h, m] = cardTime.value.split(':').map(Number);
    if (!isNaN(h) && !isNaN(m)) {
      const arrH = (h + 1) % 24;
      return `${String(arrH).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
    }
  }
  return '—';
});

const routeDuration = computed(() => {
  if (metadata.value.duration) return metadata.value.duration;
  if (metadata.value.start_time && metadata.value.end_time) {
    const t1 = Date.parse(metadata.value.start_time.replace(' ', 'T'));
    const t2 = Date.parse(metadata.value.end_time.replace(' ', 'T'));
    if (!isNaN(t1) && !isNaN(t2) && t2 > t1) {
      const mins = Math.round((t2 - t1) / 60000);
      if (mins < 60) return `${mins}m`;
      return `${Math.floor(mins / 60)}h ${mins % 60}m`;
    }
  }
  if (kind.value === 'film') return '120m';
  if (kind.value === 'rail') return '58m';
  if (kind.value === 'flight') return '2h 15m';
  return '下午场';
});

const passengerDisplay = computed(() => {
  return metadata.value.passenger || metadata.value.passenger_name || metadata.value.name || t('ticket.passenger') || '持票人';
});

const seatDisplay = computed(() => {
  return metadata.value.flight_info?.seat || metadata.value.seat_info || metadata.value.seat || '—';
});

const coachDisplay = computed(() => {
  if (metadata.value.flight_info?.cabin) return metadata.value.flight_info.cabin;
  if (metadata.value.flight_info?.coach) return metadata.value.flight_info.coach;
  if (kind.value === 'film') return metadata.value.hall || metadata.value.seat_info?.split(' ')[0] || t('ticket.hall') || '影厅';
  if (kind.value === 'rail') return '车厢';
  if (kind.value === 'museum') return '展厅';
  return '舱位';
});

const platformDisplay = computed(() => {
  if (metadata.value.gate) return '登机口 ' + metadata.value.gate;
  if (metadata.value.platform) return '检票口 ' + metadata.value.platform;
  if (metadata.value.pickup_code) return '取票码: ' + metadata.value.pickup_code;
  if (metadata.value.booking_id) return '预约号: ' + metadata.value.booking_id;
  return metadata.value.venue || '';
});

const spaceLabel = computed(() => {
  if (kind.value === 'film') return t('ticket.hall') || '影厅';
  if (kind.value === 'rail') return '车厢';
  if (kind.value === 'museum') return '展区';
  return t('ticket.coach_space') || '空间';
});

const richSubtitle = computed(() => {
  const parts = [];
  if (kind.value === 'flight') {
    const flightNum = metadata.value.flight_info?.flight_number;
    if (flightNum) parts.push(flightNum);
    if (cardTime.value !== '—') parts.push(cardTime.value + ' 起飞');
    if (seatDisplay.value !== '—') parts.push(seatDisplay.value);
  } else if (kind.value === 'rail') {
    const trainNum = metadata.value.flight_info?.flight_number || (cardSubtitle.value.match(/([GDCKZTX]\d+)/i)?.[1]);
    if (trainNum) parts.push(trainNum);
    if (cardTime.value !== '—') parts.push(cardTime.value + ' 发车');
    if (seatDisplay.value !== '—') parts.push(seatDisplay.value);
  } else if (kind.value === 'film') {
    if (metadata.value.venue) parts.push(metadata.value.venue);
    if (cardTime.value !== '—') parts.push(cardTime.value + ' 放映');
    if (seatDisplay.value !== '—') parts.push(seatDisplay.value);
  } else {
    return cardSubtitle.value;
  }
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
/* ── Deck Pass Card (Modern Segmented Pass Standard) ── */
.new-pass {
  position: relative;
  width: 100%;
  height: 238px;
  border-radius: 22px;
  background: #ffffff;
  border: 1px solid #e2e8f0;
  box-shadow: 0 10px 25px -5px rgba(30, 41, 59, 0.08), 0 4px 10px -3px rgba(30, 41, 59, 0.04);
  cursor: pointer;
  transition: transform .2s cubic-bezier(.34,1.56,.64,1), box-shadow .2s ease;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  text-align: left;
  box-sizing: border-box;
  font-family: inherit;
}
.new-pass:hover {
  transform: translateY(-4px);
  box-shadow: 0 16px 32px -6px rgba(15,23,42,0.12), 0 6px 14px -4px rgba(15,23,42,0.06);
}
.new-pass:focus-visible {
  outline: 3px solid #3b82f6;
  outline-offset: 2px;
}

/* Expired: Pure white opaque base, desaturate inner text and icons */
.new-pass.is-expired {
  background: #ffffff !important;
  opacity: 1 !important;
  border-color: #e2e8f0;
  box-shadow: 0 4px 14px -3px rgba(15, 23, 42, 0.05);
}
.new-pass.is-expired .new-pass-head,
.new-pass.is-expired .new-pass-route,
.new-pass.is-expired .new-pass-foot {
  opacity: 0.42;
  filter: grayscale(1);
  transition: opacity .15s ease, filter .15s ease;
}
.new-pass.is-expired:hover .new-pass-head,
.new-pass.is-expired:hover .new-pass-route,
.new-pass.is-expired:hover .new-pass-foot {
  opacity: 0.72;
  filter: grayscale(0.5);
}
.new-pass.is-expired .status-pill-green {
  color: #94a3b8;
}
.new-pass.is-expired .status-pill-green:before {
  background: #cbd5e1;
}

/* ── Exposed Header: 2 rows inside 72px so all vital info is visible in deck stack ── */
.new-pass-head {
  padding: 9px 14px 7px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  border-bottom: 1px solid #f1f5f9;
  height: 72px;
  box-sizing: border-box;
  background: #ffffff;
}
.new-pass-head-row1 {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
}
.new-pass-id-group {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}
.new-icon-badge {
  width: 32px;
  height: 32px;
  border-radius: 9px;
  display: grid;
  place-items: center;
  flex: none;
}
.new-icon-badge.flight, .new-icon-badge.general { background: #eff6ff; color: #2563eb; }
.new-icon-badge.rail { background: #ecfdf5; color: #059669; }
.new-icon-badge.film { background: #fdf2f8; color: #db2777; }
.new-icon-badge.museum { background: #fef3c7; color: #d97706; }
.new-icon-badge svg {
  width: 18px;
  height: 18px;
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.new-icon-badge svg.flight-symbol {
  fill: currentColor;
  stroke: none;
}
.new-pass-title {
  font-size: 14.5px;
  font-weight: 800;
  color: #0f172a;
  letter-spacing: -0.02em;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 180px;
}
.new-pass-date {
  font-size: 12px;
  font-weight: 700;
  color: #0f172a;
  white-space: nowrap;
  flex: none;
}
.new-pass-head-row2 {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding-left: 41px;
}
.new-pass-sub {
  font-size: 11.5px;
  font-weight: 600;
  color: #64748b;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 180px;
}
.new-pass-tag {
  font-size: 11px;
  font-weight: 700;
  color: #059669;
  display: flex;
  align-items: center;
  gap: 4px;
  flex: none;
}
.status-dot-mini {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #10b981;
  display: inline-block;
  flex: none;
}
.new-pass-tag.is-exp {
  color: #94a3b8;
}
.new-pass-tag.is-exp .status-dot-mini {
  background: #cbd5e1;
}

/* ── Route timeline inside card ── */
.new-pass-route {
  padding: 16px 18px 12px;
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
}
.route-row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
}
.route-station {
  font-size: 12px;
  font-weight: 600;
  color: #64748b;
}
.route-times {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 4px;
}
.route-time {
  font-size: 26px;
  font-weight: 800;
  color: #0f172a;
  letter-spacing: -0.03em;
}
.route-track {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 4px;
  margin: 0 14px;
  position: relative;
}
.track-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #94a3b8;
  flex: none;
}
.track-line {
  flex: 1;
  border-bottom: 2px dashed #cbd5e1;
}
.track-duration-pill {
  padding: 2px 8px;
  background: #f1f5f9;
  border-radius: 12px;
  font-size: 10px;
  font-weight: 700;
  color: #475569;
  display: flex;
  align-items: center;
  gap: 3px;
  white-space: nowrap;
}
.route-status-row {
  display: flex;
  justify-content: center;
  margin-top: 6px;
}
.status-pill-green {
  font-size: 11px;
  font-weight: 700;
  color: #059669;
  display: flex;
  align-items: center;
  gap: 4px;
}
.status-pill-green:before {
  content: "";
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #10b981;
}
.status-pill-green.is-exp-text {
  color: #94a3b8;
}
.status-pill-green.is-exp-text:before {
  background: #cbd5e1;
}

/* ── Card bottom row ── */
.new-pass-foot {
  background: #f8fafc;
  border-top: 1px dashed #e2e8f0;
  padding: 10px 16px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 12px;
  color: #475569;
}
.new-pass-foot strong {
  color: #0f172a;
  font-weight: 700;
}
.foot-sep {
  margin: 0 4px;
  opacity: 0.4;
}
.new-pass-foot-sub {
  font-size: 11px;
  color: #64748b;
}

/* ── Modal Overlay & Card (Modern Segmented Pass Standard) ── */
.veil {
  position: fixed;
  z-index: 9999;
  inset: 0;
  background: rgba(15, 23, 42, 0.72);
  backdrop-filter: blur(4px);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 18px;
  overflow-y: auto;
  box-sizing: border-box;
}

.ins-modal-card {
  position: relative;
  width: min(100%, 354px);
  background: #ffffff;
  border-radius: 22px;
  overflow: hidden;
  box-shadow: 0 25px 60px -15px rgba(0,0,0,0.38), 0 0 0 1px rgba(0,0,0,0.04);
  text-align: left;
  box-sizing: border-box;
  animation: modalPop .22s cubic-bezier(.34,1.4,.64,1) both;
}
@keyframes modalPop {
  from { transform: scale(0.96) translateY(12px); opacity: 0; }
  to { transform: scale(1) translateY(0); opacity: 1; }
}

/* Upper Section: White Clean Sheet */
.ins-upper {
  padding: 20px 22px 18px;
  background: #ffffff;
}

.ins-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}
.ins-header-left {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
  flex: 1;
}
.ins-header-left .new-icon-badge.modal-badge {
  width: 44px;
  height: 44px;
  border-radius: 12px;
  display: grid;
  place-items: center;
  flex: none;
}
.ins-header-left .new-icon-badge.modal-badge svg {
  width: 24px;
  height: 24px;
}
.ins-header-text {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.ins-title {
  font-size: 16.5px;
  font-weight: 800;
  color: #0f172a;
  letter-spacing: -0.02em;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 170px;
  line-height: 1.25;
}
.ins-sub {
  font-size: 11.5px;
  font-weight: 600;
  color: #64748b;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 170px;
  line-height: 1.25;
}

.ins-header-right {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 3px;
  flex: none;
  text-align: right;
}
.ins-date {
  font-size: 13.5px;
  font-weight: 800;
  color: #0f172a;
  line-height: 1.25;
  white-space: nowrap;
}
.ins-status-badge {
  font-size: 11.5px;
  font-weight: 700;
  color: #059669;
  display: flex;
  align-items: center;
  gap: 4px;
  line-height: 1.25;
  white-space: nowrap;
}
.ins-status-badge.is-exp {
  color: #94a3b8;
}
.ins-status-badge .status-dot-mini {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #10b981;
  display: inline-block;
  flex: none;
}
.ins-status-badge.is-exp .status-dot-mini {
  background: #cbd5e1;
}

/* Journey Segment */
.ins-stations-row {
  display: flex;
  justify-content: space-between;
  font-size: 14px;
  font-weight: 600;
  color: #334155;
  margin-bottom: 4px;
}
.ins-times-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.ins-large-time {
  font-size: 28px;
  font-weight: 800;
  color: #0f172a;
  letter-spacing: -0.04em;
  line-height: 1;
}
.ins-track-bar {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 4px;
  margin: 0 12px;
}
.ins-track-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #94a3b8;
  flex: none;
}
.ins-track-dashed {
  flex: 1;
  border-bottom: 2px dashed #cbd5e1;
}
.ins-duration-capsule {
  padding: 3px 9px;
  background: #f1f5f9;
  border-radius: 20px;
  font-size: 11px;
  font-weight: 700;
  color: #475569;
  display: flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
}
.ins-on-time-status {
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 5px;
  margin: 10px 0 16px;
  font-size: 12px;
  font-weight: 700;
  color: #059669;
}
.ins-on-time-status:before {
  content: "";
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #10b981;
}
.ins-on-time-status.is-expired-status {
  color: #94a3b8;
}
.ins-on-time-status.is-expired-status:before {
  background: #cbd5e1;
}

/* Passenger & Seat row */
.ins-meta-row {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  padding-top: 14px;
  border-top: 1px solid #f1f5f9;
}
.ins-meta-col small {
  display: block;
  font-size: 11px;
  font-weight: 600;
  color: #94a3b8;
  margin-bottom: 3px;
  text-transform: capitalize;
}
.ins-meta-col strong {
  display: block;
  font-size: 15px;
  font-weight: 800;
  color: #0f172a;
}
.ins-seat-grid {
  display: flex;
  gap: 24px;
  text-align: right;
}

/* Lower Section: Action & Verification Bay (Clean Flat Edge) */
.ins-lower {
  background: #f8fafc;
  padding: 18px 22px 20px;
  border-top: 1px solid #f1f5f9;
  position: relative;
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.ins-action-col {
  flex: 1;
  padding-right: 12px;
}
.platform-pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 10px;
  background: #dbeafe;
  color: #1e40af;
  border-radius: 8px;
  font-size: 12px;
  font-weight: 800;
  margin-bottom: 8px;
}
.ins-boarding-title {
  font-size: 14px;
  font-weight: 800;
  color: #0f172a;
  margin: 0 0 2px;
}
.ins-countdown {
  font-size: 12px;
  font-weight: 600;
  color: #64748b;
  margin: 0;
}
.ins-countdown b {
  color: #2563eb;
  font-weight: 800;
}

.ins-qr-col {
  flex: none;
  text-align: center;
}
.ins-qr-frame {
  width: 96px;
  height: 96px;
  background: #ffffff;
  border-radius: 14px;
  padding: 6px;
  box-shadow: 0 4px 12px rgba(0,0,0,0.06);
  border: 1px solid #e2e8f0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
}
.ins-barcode-text {
  font-size: 11px;
  word-break: break-all;
  max-width: 84px;
  text-align: center;
  font-family: monospace;
  color: #0f172a;
}
.qr-placeholder {
  width: 100%;
  height: 100%;
}
.ins-qr-tip {
  font-size: 10px;
  font-weight: 600;
  color: #94a3b8;
  margin-top: 4px;
  display: block;
}

/* Modal Bottom Action Bar (Edit / Delete) */
.detail-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 10px 22px 14px;
  background: #f8fafc;
  border-top: 1px solid #f1f5f9;
}
.detail-act-btn {
  background: #ffffff;
  border: 1px solid #cbd5e1;
  color: #334155;
  border-radius: 12px;
  padding: 6px 14px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  transition: all .15s ease;
  box-shadow: 0 1px 3px rgba(0,0,0,0.05);
}
.detail-act-btn:hover {
  background: #f1f5f9;
  color: #0f172a;
}
.detail-del-btn {
  background: #fef2f2;
  border-color: #fecaca;
  color: #dc2626;
}
.detail-del-btn:hover {
  background: #fee2e2;
}

/* Close Button */
.modal-close-btn {
  margin-top: 16px;
  border: 0;
  border-radius: 20px;
  background: #ffffff;
  color: #0f172a;
  padding: 9px 26px;
  font-size: 13px;
  font-weight: 700;
  cursor: pointer;
  box-shadow: 0 4px 14px rgba(0,0,0,0.2);
  transition: transform .15s ease;
}
.modal-close-btn:hover {
  transform: scale(1.04);
}

/* Inline Edit Form */
.detail-edit-form {
  padding: 16px 22px;
  background: #ffffff;
  border-top: 1px solid #f1f5f9;
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
  font-weight: 600;
  color: #64748b;
}
.edit-input {
  background: #f8fafc;
  border: 1px solid #cbd5e1;
  border-radius: 8px;
  padding: 7px 10px;
  color: #0f172a;
  font-size: 13px;
  outline: none;
  transition: border-color .15s;
}
.edit-input:focus {
  border-color: #2563eb;
  background: #ffffff;
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
  margin-top: 6px;
}
.edit-submit-btn {
  background: #2563eb;
  color: #ffffff;
  border: none;
  border-radius: 10px;
  padding: 6px 16px;
  font-size: 13px;
  font-weight: 700;
  cursor: pointer;
}
.edit-cancel-btn {
  background: #f1f5f9;
  color: #475569;
  border: 1px solid #cbd5e1;
  border-radius: 10px;
  padding: 6px 14px;
  font-size: 13px;
  cursor: pointer;
}
</style>
