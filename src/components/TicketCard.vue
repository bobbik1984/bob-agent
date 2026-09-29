<template>
  <div
    class="wallet-pass-card"
    :class="[
      themeClass,
      { 'is-expired': isExpired, 'is-stacked': isStacked }
    ]"
    :style="stackStyle"
    @click="openDetail"
  >
    <!-- Ticket Stub Notch (Classic Passbook cutout for movie/events) -->
    <div v-if="isMovie || isExhibition" class="notch notch-left"></div>
    <div v-if="isMovie || isExhibition" class="notch notch-right"></div>

    <!-- Pass Header Band -->
    <div class="pass-header">
      <div class="pass-badge">
        <component :is="categoryIcon" class="pass-icon" />
        <span class="pass-type-label">{{ categoryLabel }}</span>
        <span v-if="flightOrTrainNumber" class="pass-number-tag">{{ flightOrTrainNumber }}</span>
      </div>

      <div class="pass-status-pill" :class="ticketStatusClass">
        <span class="status-dot"></span>
        <span class="status-text">{{ displayStatus }}</span>
      </div>
    </div>

    <!-- Pass Route / Main Event Section -->
    <div class="pass-main">
      <template v-if="isTravel">
        <div class="pass-route-row">
          <div class="route-city origin">
            <span class="city-code">{{ originLabel || '出发' }}</span>
            <span v-if="metadata.flight_info?.origin_terminal" class="city-terminal">{{ metadata.flight_info.origin_terminal }}</span>
          </div>

          <div class="route-transit">
            <component :is="categoryIcon" class="transit-icon" />
            <div class="transit-line">
              <span class="transit-arrow">➔</span>
            </div>
          </div>

          <div class="route-city destination">
            <span class="city-code">{{ destinationLabel || '到达' }}</span>
            <span v-if="metadata.flight_info?.destination_terminal" class="city-terminal">{{ metadata.flight_info.destination_terminal }}</span>
          </div>
        </div>
      </template>

      <template v-else>
        <div class="pass-event-row">
          <div class="event-title" :title="node.label">{{ node.label }}</div>
          <div v-if="metadata.venue" class="event-venue" :title="metadata.venue">
            <MapPin :size="12" style="flex-shrink: 0;" />
            <span>{{ metadata.venue }}</span>
          </div>
        </div>
      </template>
    </div>

    <!-- Pass Key Info Footer (Always visible in stack preview!) -->
    <div class="pass-footer">
      <div class="info-cell" v-if="departureDateStr">
        <span class="info-label">{{ isTravel ? '出发日期' : '日期' }}</span>
        <span class="info-value">{{ departureDateStr }}</span>
      </div>

      <div class="info-cell" v-if="departureTimeStr">
        <span class="info-label">{{ isTravel ? '出发时间' : '时间' }}</span>
        <span class="info-value time-highlight">{{ departureTimeStr }}</span>
      </div>

      <div class="info-cell" v-if="seatLabel">
        <span class="info-label">座位</span>
        <span class="info-value seat-tag">{{ seatLabel }}</span>
      </div>

      <div class="info-cell passenger-cell" v-if="passengerName">
        <span class="info-label">乘车/机人</span>
        <span class="info-value">{{ passengerName }}</span>
      </div>

      <div class="pass-mini-barcode" v-if="metadata.barcode_data" title="包含扫码凭证">
        <QrCode :size="15" />
      </div>
    </div>
  </div>

  <!-- Detail Modal (Boarding Pass / Ticket Inspector) -->
  <Teleport to="body">
    <div v-if="showDetail" class="bp-modal-overlay" @click.self="showDetail = false">
      <div class="boarding-pass-modern-card" :class="themeClass">
        <div class="bp-modern-header">
          <span class="bp-modern-icon"><component :is="categoryIcon" style="width:16px;height:16px;" /></span>
          <span>{{ modalHeaderTitle }}</span>
        </div>

        <div class="bp-route-row" v-if="isTravel">
          <div class="bp-airport-group is-origin">
            <input v-if="isEditing" v-model="editForm.origin" class="bp-edit-input bp-airport-code" style="width: 85px; text-align: center;" />
            <span v-else class="bp-airport-code">{{ originLabel || '出发' }}</span>
            <input v-if="isEditing" v-model="editForm.originTerminal" class="bp-edit-input" placeholder="航站楼" style="width: 85px; font-size: 1.05em; text-align: center; height: 26px; box-sizing: border-box;" />
            <div v-else class="bp-terminal" style="height: 26px; line-height: 26px;">{{ metadata.flight_info?.origin_terminal || '' }}</div>

            <input type="text" v-if="isEditing" v-model="editForm.date" @input="formatDateInput('date')" class="bp-edit-input" placeholder="YYYY-MM-DD" maxlength="10" style="width: 115px; font-size: 0.85em; text-align: center; padding: 2px; height: 22px; box-sizing: border-box;" />
            <div v-else style="font-size: 0.85em; opacity: 0.7; height: 22px; line-height: 22px; text-align: center;">{{ departureDateStr }}</div>

            <input type="text" v-if="isEditing" v-model="editForm.time" @input="formatTimeInput('time')" class="bp-edit-input" placeholder="HH:MM" maxlength="5" style="width: 90px; font-size: 1.1em; font-weight: 500; text-align: center; padding: 2px; height: 26px; box-sizing: border-box;" />
            <div v-else style="font-size: 1.2em; font-weight: 600; height: 26px; line-height: 26px; text-align: center;">{{ departureTimeStr }}</div>
          </div>

          <div class="bp-route-center">
            <component :is="categoryIcon" style="width: 24px; height: 24px;" />
          </div>

          <div class="bp-airport-group is-destination">
            <input v-if="isEditing" v-model="editForm.destination" class="bp-edit-input bp-airport-code" style="width: 85px; text-align: center;" />
            <span v-else class="bp-airport-code">{{ destinationLabel || '到达' }}</span>
            <input v-if="isEditing" v-model="editForm.destinationTerminal" class="bp-edit-input" placeholder="航站楼" style="width: 85px; font-size: 1.05em; text-align: center; height: 26px; box-sizing: border-box;" />
            <div v-else class="bp-terminal" style="height: 26px; line-height: 26px;">{{ metadata.flight_info?.destination_terminal || '' }}</div>

            <input type="text" v-if="isEditing" v-model="editForm.endDate" @input="formatDateInput('endDate')" class="bp-edit-input" placeholder="YYYY-MM-DD" maxlength="10" style="width: 115px; font-size: 0.85em; text-align: center; padding: 2px; height: 22px; box-sizing: border-box;" />
            <div v-else style="font-size: 0.85em; opacity: 0.7; height: 22px; line-height: 22px; text-align: center;">{{ arrivalDateStr }}</div>

            <input type="text" v-if="isEditing" v-model="editForm.endTime" @input="formatTimeInput('endTime')" class="bp-edit-input" placeholder="HH:MM" maxlength="5" style="width: 90px; font-size: 1.1em; font-weight: 500; text-align: center; padding: 2px; height: 26px; box-sizing: border-box;" />
            <div v-else style="font-size: 1.2em; font-weight: 600; height: 26px; line-height: 26px; text-align: center;">{{ arrivalTimeStr }}</div>
          </div>
        </div>
        <div class="bp-route-row" v-else style="display: flex; flex-direction: column; gap: 8px;">
          <input v-if="isEditing" v-model="editForm.title" class="bp-edit-input bp-airport-code" style="width: 100%;" />
          <span v-else class="bp-airport-code" style="font-size:1.4em; white-space: normal; height: auto; line-height: 1.3;">{{ node.label }}</span>
        </div>

        <div class="bp-modern-divider"></div>

        <div class="bp-detail-grid">
          <div class="bp-modern-field" v-if="isTravel || passengerName || isEditing">
            <div class="bp-modern-label">{{ $t('ticket.passenger') || '乘车/乘机人' }}</div>
            <input v-if="isEditing" v-model="editForm.passenger_name" class="bp-edit-input" />
            <div v-else class="bp-modern-value">{{ passengerName || '—' }}</div>
          </div>
          <div class="bp-modern-field" v-if="isTravel">
            <div class="bp-modern-label">{{ isFlight ? ($t('ticket.flight') || '航班号') : ($t('ticket.train') || '车次') }}</div>
            <input v-if="isEditing" v-model="editForm.flight_number" class="bp-edit-input" />
            <div v-else class="bp-modern-value">{{ flightOrTrainNumber || '—' }}</div>
          </div>
          <div class="bp-modern-field" v-if="!isTravel">
            <div class="bp-modern-label">{{ $t('ticket.date') || '日期' }}</div>
            <input type="text" v-if="isEditing" v-model="editForm.date" @input="formatDateInput('date')" class="bp-edit-input" placeholder="YYYY-MM-DD" maxlength="10" />
            <div v-else class="bp-modern-value">{{ departureDateStr || '—' }}</div>
          </div>
          <div class="bp-modern-field" v-if="!isTravel">
            <div class="bp-modern-label">{{ $t('ticket.time') || '时间' }}</div>
            <input type="text" v-if="isEditing" v-model="editForm.time" @input="formatTimeInput('time')" class="bp-edit-input" placeholder="HH:MM" maxlength="5" />
            <div v-else class="bp-modern-value">{{ departureTimeStr || '—' }}</div>
          </div>
          <div class="bp-modern-field">
            <div class="bp-modern-label">{{ $t('ticket.seat') || '座位' }}</div>
            <input v-if="isEditing" v-model="editForm.seat" class="bp-edit-input" />
            <div v-else class="bp-modern-value">{{ seatLabel || '—' }}</div>
          </div>
          <div class="bp-modern-field" v-if="isTravel">
            <div class="bp-modern-label">{{ $t('ticket.pnr') || '票号/预约' }}</div>
            <input v-if="isEditing" v-model="editForm.pnr" class="bp-edit-input" />
            <div v-else class="bp-modern-value">{{ metadata.flight_info?.pnr || '—' }}</div>
          </div>
          <div class="bp-modern-field" v-if="!isTravel" style="grid-column: span 3; align-items: flex-start;">
            <div class="bp-modern-label">{{ $t('ticket.venue') || '场馆/地点' }}</div>
            <input v-if="isEditing" v-model="editForm.venue" class="bp-edit-input" style="text-align: left;" />
            <div v-else class="bp-modern-value" style="text-align: left;">{{ metadata.venue || '—' }}</div>
          </div>
        </div>

        <!-- QR Code Section: Fixed High-Contrast White Background for Scanners -->
        <div class="bp-modern-qr-section" v-if="metadata.barcode_data">
          <div class="bp-modern-qr-wrapper">
            <qrcode-vue :value="metadata.barcode_data" :size="180" level="M" />
          </div>
          <div class="bp-qr-hint">出示扫码乘车 / 乘机 / 核销</div>
          <div v-if="metadata.barcode_data.length <= 40" class="bp-qr-code-text">{{ metadata.barcode_data }}</div>
        </div>

        <div class="bp-modern-actions">
          <button class="bp-modern-btn bp-modern-btn-danger" @click="deleteTicket" title="删除票据">
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
const { showConfirm, showAlert } = useDialog();

import { computed, ref, watch, onMounted, onUnmounted } from 'vue';
import { Plane, Film, Ticket, Calendar, Train, CreditCard, Music, Trash2, MapPin, QrCode } from 'lucide-vue-next';
import QrcodeVue from 'qrcode.vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  node: {
    type: Object,
    required: true
  },
  isStacked: {
    type: Boolean,
    default: false
  },
  index: {
    type: Number,
    default: 0
  },
  total: {
    type: Number,
    default: 1
  }
});

const { t } = useI18n();

const showDetail = ref(false);
const isEditing = ref(false);
const editForm = ref({});
const isAddedToCalendar = ref(false);

const openDetail = () => {
  showDetail.value = true;
};

// ── Metadata & Intelligent Categorization ───────────────────────
const metadata = computed(() => {
  if (typeof props.node.metadata === 'string') {
    try {
      return JSON.parse(props.node.metadata);
    } catch (e) {
      console.error("TicketCard JSON parse error:", e);
      return {};
    }
  }
  return props.node.metadata || {};
});

const detectedCategory = computed(() => {
  const cat = (metadata.value.category || '').toLowerCase();
  if (cat === 'flight' || cat === 'air') return 'flight';
  if (cat === 'train' || cat === 'rail') return 'train';
  if (cat === 'movie' || cat === 'film' || cat === 'cinema') return 'movie';
  if (cat === 'concert' || cat === 'music' || cat === 'show') return 'concert';
  if (cat === 'exhibition' || cat === 'museum' || cat === 'expo') return 'exhibition';
  if (cat === 'membership' || cat === 'card') return 'membership';

  // Smart heuristic inference from title/summary
  const text = `${props.node.label || ''} ${props.node.summary || ''} ${metadata.value.venue || ''}`;
  if (/(\b[GDCKZTX]\d{1,4}\b)|(\([GDCKZTX\d]+\))|高铁|动车|列车|火车站|车次/i.test(text)) {
    return 'train';
  }
  if (/电影|影城|影院|放映|激光厅|IMAX/i.test(text)) {
    return 'movie';
  }
  if (/音乐|演唱会|大剧院|演出|话剧|音乐节/i.test(text)) {
    return 'concert';
  }
  if (/展|博物馆|美术馆|博览会|门票/i.test(text)) {
    return 'exhibition';
  }
  if (/(\b[A-Z]{2}\s?\d{3,4}\b)|(\b[A-Z][0-9]\s?\d{3,4}\b)|(\b[0-9][A-Z]\s?\d{3,4}\b)|([A-Z]{3}\s*[-–➔✈]\s*[A-Z]{3})|航班|机票|登机牌/i.test(text)) {
    return 'flight';
  }
  return 'ticket';
});

const isFlight = computed(() => detectedCategory.value === 'flight');
const isTrain = computed(() => detectedCategory.value === 'train');
const isTravel = computed(() => isFlight.value || isTrain.value);
const isMovie = computed(() => detectedCategory.value === 'movie');
const isConcert = computed(() => detectedCategory.value === 'concert');
const isExhibition = computed(() => detectedCategory.value === 'exhibition');

const themeClass = computed(() => {
  switch (detectedCategory.value) {
    case 'flight': return 'theme-flight';
    case 'train': return 'theme-train';
    case 'movie': return 'theme-movie';
    case 'concert': return 'theme-concert';
    case 'exhibition': return 'theme-exhibition';
    default: return 'theme-general';
  }
});

const categoryIcon = computed(() => {
  switch (detectedCategory.value) {
    case 'flight': return Plane;
    case 'train': return Train;
    case 'movie': return Film;
    case 'concert': return Music;
    case 'exhibition': return Ticket;
    case 'membership': return CreditCard;
    default: return Ticket;
  }
});

const categoryLabel = computed(() => {
  switch (detectedCategory.value) {
    case 'flight': return '航班机票';
    case 'train': return '高铁动车';
    case 'movie': return '电影客票';
    case 'concert': return '演出门票';
    case 'exhibition': return '展览门票';
    case 'membership': return '会员卡券';
    default: return '通行凭证';
  }
});

const modalHeaderTitle = computed(() => {
  if (isFlight.value) return 'Boarding Pass · 登机牌';
  if (isTrain.value) return 'Train Ticket · 高铁票';
  if (isMovie.value) return 'Movie Pass · 电影票';
  return 'Passbook · 票据凭证';
});

// ── Smart Route & Details ───────────────────────────────────────
const originLabel = computed(() => {
  if (metadata.value.flight_info?.origin) return metadata.value.flight_info.origin;
  if (metadata.value.venue && metadata.value.venue.includes('-')) return metadata.value.venue.split('-')[0].trim();
  const label = props.node.label || '';
  if (label.includes('➔')) return label.split('➔')[0].trim();
  if (label.includes('->')) return label.split('->')[0].trim();
  const match = label.match(/([A-Z]{3})\s*[-–]\s*([A-Z]{3})/i);
  if (match) return match[1].toUpperCase();
  return '';
});

const destinationLabel = computed(() => {
  if (metadata.value.flight_info?.destination) return metadata.value.flight_info.destination;
  if (metadata.value.venue && metadata.value.venue.includes('-')) return metadata.value.venue.split('-')[1].trim();
  const label = props.node.label || '';
  if (label.includes('➔')) {
    return label.split('➔')[1].replace(/\(.*?\)/g, '').trim();
  }
  if (label.includes('->')) {
    return label.split('->')[1].replace(/\(.*?\)/g, '').trim();
  }
  const match = label.match(/([A-Z]{3})\s*[-–]\s*([A-Z]{3})/i);
  if (match) return match[2].toUpperCase();
  return '';
});

const flightOrTrainNumber = computed(() => {
  if (metadata.value.flight_info?.flight_number) return metadata.value.flight_info.flight_number;
  const label = props.node.label || '';
  const trainMatch = label.match(/\(([GDCKZTX\d]+)\)/i) || label.match(/\b([GDCKZTX]\d{1,4})\b/i);
  if (trainMatch) return trainMatch[1];
  const flightMatch = label.match(/\b([A-Z0-9]{2}\s?\d{3,4})\b/i);
  if (flightMatch) return flightMatch[1];
  return '';
});

const passengerName = computed(() => {
  return metadata.value.flight_info?.passenger_name || metadata.value.passenger_name || '';
});

const seatLabel = computed(() => {
  if (metadata.value.seat_info) return metadata.value.seat_info;
  if (metadata.value.flight_info?.seat) return metadata.value.flight_info.seat;
  return '';
});

const departureDateStr = computed(() => {
  if (!metadata.value.start_time) return '';
  return metadata.value.start_time.split(' ')[0];
});

const departureTimeStr = computed(() => {
  if (!metadata.value.start_time) return '';
  const parts = metadata.value.start_time.split(' ');
  if (parts.length > 1 && parts[1] !== '00:00:00') {
    return parts[1].substring(0, 5);
  }
  return '';
});

const arrivalDateStr = computed(() => {
  if (!metadata.value.end_time) return '';
  return metadata.value.end_time.split(' ')[0];
});

const arrivalTimeStr = computed(() => {
  if (!metadata.value.end_time) return '';
  const parts = metadata.value.end_time.split(' ');
  if (parts.length > 1 && parts[1] !== '00:00:00') {
    return parts[1].substring(0, 5);
  }
  return '';
});

const isExpired = computed(() => {
  if (!metadata.value.start_time) return false;
  const dtStr = metadata.value.start_time.replace(' ', 'T');
  const startTime = new Date(dtStr).getTime();
  if (isNaN(startTime)) return false;
  return Date.now() > startTime + 24 * 3600 * 1000;
});

const displayStatus = computed(() => {
  if (isExpired.value) return '已过期';
  if (metadata.value.status === 'upcoming') return '有效';
  return metadata.value.status || '有效';
});

const ticketStatusClass = computed(() => {
  return isExpired.value ? 'status-expired' : 'status-active';
});

const stackStyle = computed(() => {
  if (!props.isStacked) return {};
  return {
    zIndex: props.index + 1
  };
});

// ── Edit & Actions ──────────────────────────────────────────────
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
    } catch(e) {
      console.error("checkCalendar err", e);
    }
  }
};

const startEdit = () => {
  editForm.value = {
    title: props.node.label,
    origin: originLabel.value,
    originTerminal: metadata.value.flight_info?.origin_terminal || '',
    destination: destinationLabel.value,
    destinationTerminal: metadata.value.flight_info?.destination_terminal || '',
    date: departureDateStr.value,
    time: departureTimeStr.value,
    endDate: arrivalDateStr.value,
    endTime: arrivalTimeStr.value,
    flight_number: flightOrTrainNumber.value,
    passenger_name: passengerName.value,
    seat: seatLabel.value,
    pnr: metadata.value.flight_info?.pnr || '',
    venue: metadata.value.venue || '',
    barcode_data: metadata.value.barcode_data || ''
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

    let newMetadata = { ...metadata.value };
    newMetadata.start_time = newStartTime;
    newMetadata.end_time = newEndTime;
    newMetadata.venue = editForm.value.venue;
    newMetadata.barcode_data = editForm.value.barcode_data;
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
    await window.appAPI.kgUpdateTicket(props.node.id, newTitle, newMetadata);

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
/* ── Apple Wallet / Passbook Card Stacking Design ── */
.wallet-pass-card {
  position: relative;
  border-radius: 16px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  width: 100%;
  min-height: 140px;
  padding: 16px 18px;
  box-sizing: border-box;
  color: #ffffff;
  cursor: pointer;
  box-shadow: 0 4px 18px rgba(0, 0, 0, 0.16), 0 1px 3px rgba(0, 0, 0, 0.08);
  transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1), box-shadow 0.25s cubic-bezier(0.16, 1, 0.3, 1);
  user-select: none;
  border: 1px solid rgba(255, 255, 255, 0.12);
}

.wallet-pass-card:hover {
  transform: translateY(-4px);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.25);
}

/* Stack mode specific overlapping */
.wallet-pass-card.is-stacked {
  margin-top: -80px;
}
.wallet-pass-card.is-stacked:first-child {
  margin-top: 0;
}
.wallet-pass-card.is-stacked:hover {
  transform: translateY(-24px) scale(1.01);
  z-index: 99 !important;
  box-shadow: 0 18px 40px rgba(0, 0, 0, 0.38);
}

/* Category Themes (Rich saturated gradients) */
.theme-flight {
  background: linear-gradient(135deg, #0f2b5c 0%, #1d4ed8 60%, #3b82f6 100%);
}
.theme-train {
  background: linear-gradient(135deg, #064e3b 0%, #047857 60%, #10b981 100%);
}
.theme-movie {
  background: linear-gradient(135deg, #4c0519 0%, #9f1239 60%, #ec4899 100%);
}
.theme-concert {
  background: linear-gradient(135deg, #78350f 0%, #b45309 60%, #f59e0b 100%);
}
.theme-exhibition {
  background: linear-gradient(135deg, #3b0764 0%, #6d28d9 60%, #8b5cf6 100%);
}
.theme-general {
  background: linear-gradient(135deg, #1e293b 0%, #334155 60%, #475569 100%);
}

.wallet-pass-card.is-expired {
  filter: grayscale(0.7) opacity(0.85);
}

/* Perforated ticket notches for movie/exhibitions */
.notch {
  position: absolute;
  top: 50%;
  width: 14px;
  height: 24px;
  background-color: var(--bg-primary);
  border: 1px solid rgba(255, 255, 255, 0.12);
  transform: translateY(-50%);
  z-index: 3;
}
.notch-left {
  left: -8px;
  border-radius: 0 14px 14px 0;
  border-left: none;
}
.notch-right {
  right: -8px;
  border-radius: 14px 0 0 14px;
  border-right: none;
}

/* Pass Header */
.pass-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

.pass-badge {
  display: flex;
  align-items: center;
  gap: 6px;
  background: rgba(0, 0, 0, 0.22);
  backdrop-filter: blur(8px);
  padding: 4px 10px;
  border-radius: 20px;
  border: 1px solid rgba(255, 255, 255, 0.15);
}

.pass-icon {
  width: 14px;
  height: 14px;
}

.pass-type-label {
  font-size: 0.75rem;
  font-weight: 600;
  letter-spacing: 0.5px;
}

.pass-number-tag {
  font-size: 0.75rem;
  font-weight: 700;
  background: rgba(255, 255, 255, 0.2);
  padding: 1px 6px;
  border-radius: 10px;
  letter-spacing: 0.5px;
}

.pass-status-pill {
  display: flex;
  align-items: center;
  gap: 5px;
  background: rgba(0, 0, 0, 0.25);
  padding: 3px 8px;
  border-radius: 12px;
  font-size: 0.7rem;
  font-weight: 500;
}
.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
}
.status-active .status-dot {
  background-color: #34d399;
  box-shadow: 0 0 8px #34d399;
}
.status-expired .status-dot {
  background-color: #9ca3af;
}

/* Pass Main Section */
.pass-main {
  margin: 6px 0 10px;
}

.pass-route-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.route-city {
  display: flex;
  flex-direction: column;
}
.route-city.origin {
  align-items: flex-start;
}
.route-city.destination {
  align-items: flex-end;
}

.city-code {
  font-size: 1.35rem;
  font-weight: 700;
  letter-spacing: 0.5px;
}

.city-terminal {
  font-size: 0.7rem;
  opacity: 0.8;
  margin-top: 1px;
}

.route-transit {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex: 1;
  padding: 0 12px;
}

.transit-icon {
  width: 14px;
  height: 14px;
  opacity: 0.75;
}

.transit-line {
  position: relative;
  width: 100%;
  max-width: 90px;
  height: 1px;
  background: rgba(255, 255, 255, 0.3);
  margin-top: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.transit-arrow {
  font-size: 9px;
  opacity: 0.8;
}

.pass-event-row {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.event-title {
  font-size: 1.15rem;
  font-weight: 700;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.event-venue {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 0.78rem;
  opacity: 0.85;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Pass Footer Info Strip */
.pass-footer {
  display: flex;
  align-items: center;
  gap: 16px;
  padding-top: 8px;
  border-top: 1px dashed rgba(255, 255, 255, 0.2);
}

.info-cell {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.info-label {
  font-size: 0.65rem;
  opacity: 0.7;
  letter-spacing: 0.3px;
  text-transform: uppercase;
}

.info-value {
  font-size: 0.88rem;
  font-weight: 600;
  white-space: nowrap;
}

.time-highlight {
  font-size: 1.05rem;
  font-weight: 700;
}

.seat-tag {
  background: rgba(255, 255, 255, 0.2);
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 0.78rem;
}

.passenger-cell {
  margin-left: auto;
}

.pass-mini-barcode {
  display: flex;
  align-items: center;
  opacity: 0.75;
}

/* ── Detail Modal ── */
.bp-modal-overlay {
  position: fixed;
  top: 0; left: 0; right: 0; bottom: 0;
  background: rgba(0, 0, 0, 0.65);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
  backdrop-filter: blur(6px);
}

.boarding-pass-modern-card {
  background: var(--bg-tertiary);
  color: var(--text-primary);
  border: 1px solid var(--border-default);
  border-radius: 18px;
  padding: 22px;
  width: 90%;
  max-width: 360px;
  box-shadow: 0 20px 50px rgba(0, 0, 0, 0.4);
  animation: modalPop 0.28s cubic-bezier(0.16, 1, 0.3, 1);
  font-family: var(--font-sans, system-ui, sans-serif);
}

@keyframes modalPop {
  from { opacity: 0; transform: scale(0.92); }
  to { opacity: 1; transform: scale(1); }
}

.bp-modern-header {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.85rem;
  font-weight: 600;
  opacity: 0.8;
  margin-bottom: 12px;
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
  margin: 12px 0 18px;
}

.bp-route-center {
  opacity: 0.5;
  margin-top: 6px;
}

.bp-airport-code {
  font-size: 1.55rem;
  font-weight: 700;
  letter-spacing: 1px;
  height: 34px;
  line-height: 34px;
  display: inline-block;
}

.bp-detail-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px 12px;
}

.bp-modern-field {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-height: 44px;
}

.bp-modern-field:nth-child(3n+1) { align-items: flex-start; text-align: left; }
.bp-modern-field:nth-child(3n+2) { align-items: center; text-align: center; }
.bp-modern-field:nth-child(3n) { align-items: flex-end; text-align: right; }

.bp-modern-label {
  font-size: 0.72rem;
  color: var(--text-tertiary);
  letter-spacing: 0.3px;
}

.bp-modern-value {
  font-size: 1.05rem;
  font-weight: 700;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  width: 100%;
}

.bp-modern-divider {
  height: 1px;
  background: var(--border-default);
  margin: 12px 0;
}

/* High Contrast QR Section */
.bp-modern-qr-section {
  margin-top: 18px;
  display: flex;
  flex-direction: column;
  align-items: center;
}

.bp-modern-qr-wrapper {
  background: #ffffff !important;
  color: #000000 !important;
  padding: 14px;
  border-radius: 12px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.15);
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.bp-qr-hint {
  font-size: 0.75rem;
  color: var(--text-secondary);
  margin-top: 8px;
  opacity: 0.8;
}

.bp-qr-code-text {
  font-size: 0.78rem;
  font-family: monospace;
  color: var(--text-tertiary);
  margin-top: 4px;
  letter-spacing: 0.5px;
}

.bp-modern-actions {
  display: flex;
  gap: 10px;
  margin-top: 20px;
}

.bp-modern-btn {
  flex: 1;
  padding: 10px;
  border-radius: var(--radius-default);
  border: none;
  font-weight: 600;
  cursor: pointer;
  transition: opacity 0.2s;
  font-size: 0.9rem;
}

.bp-modern-btn:hover {
  opacity: 0.88;
}

.bp-modern-btn-dismiss {
  background: var(--bg-secondary);
  color: var(--text-secondary);
}

.bp-modern-btn-danger {
  background: var(--color-error, #ef4444);
  color: #ffffff;
  flex: none;
  width: 44px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.bp-modern-btn-primary {
  background: var(--accent-primary, #3b82f6);
  color: #ffffff;
}

.bp-edit-input {
  background: var(--bg-primary);
  border: 1px solid var(--border-default);
  color: var(--text-primary);
  border-radius: var(--radius-default);
  padding: 4px 8px;
  font-size: 1rem;
  width: 100%;
  box-sizing: border-box;
  font-family: inherit;
  height: 28px;
}

.bp-airport-group {
  display: grid;
  grid-template-rows: 34px 28px 26px 28px;
  align-items: center;
}
.bp-airport-group.is-origin { justify-items: start; }
.bp-airport-group.is-destination { justify-items: end; }
.bp-terminal {
  font-size: 1rem;
  font-weight: 500;
  opacity: 0.7;
}
</style>
