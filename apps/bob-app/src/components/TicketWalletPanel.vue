<script setup>
import { computed, reactive, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import CaptureImage from './CaptureImage.vue';
import CustomSelect from './CustomSelect.vue';

const props = defineProps({
  notes: {
    type: Array,
    default: () => [],
  },
  busy: {
    type: Boolean,
    default: false,
  },
  recognizing: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits(['save-ticket', 'delete-ticket', 'recognize-ticket-file']);

const { t } = useI18n();
const fileInputRef = ref(null);
const showManualForm = ref(false);
const activeDetailId = ref(null);

const categoryOptions = computed(() => [
  { value: 'flight', label: t('ticketCategoryFlight') },
  { value: 'train', label: t('ticketCategoryTrain') },
  { value: 'movie', label: t('ticketCategoryMovie') },
  { value: 'exhibition', label: t('ticketCategoryExhibition') },
  { value: 'concert', label: t('ticketCategoryConcert') },
  { value: 'general', label: t('ticketCategoryGeneral') },
]);

const form = reactive({
  id: '',
  category: 'train',
  title: '',
  number: '',
  origin: '',
  destination: '',
  venue: '',
  startTime: '',
  endTime: '',
  seat: '',
  coach: '',
  gate: '',
  passenger: '',
  pickupCode: '',
  bookingId: '',
  mediaId: '',
});

function parseTicketNote(note) {
  if (!note) return null;
  const tags = Array.isArray(note.tags) ? note.tags : [];
  const isTagged = tags.some(tag => tag === 'ticket' || String(tag).startsWith('ticket:'));
  const rawBody = note.body_markdown || note.body || '';
  let parsed = null;
  try {
    parsed = JSON.parse(rawBody);
  } catch {
    parsed = null;
  }
  if (!isTagged && (!parsed || parsed.kind !== 'ticket')) {
    return null;
  }
  const meta = parsed?.ticket && typeof parsed.ticket === 'object' ? parsed.ticket : parsed || {};
  const tagCat = tags
    .find(tag => String(tag).startsWith('ticket:'))
    ?.slice('ticket:'.length);
  const category = meta.category || parsed?.category || tagCat || 'general';
  const startAtMs = Number(parsed?.start_at_ms || 0) || (meta.start_time ? Date.parse(meta.start_time.replace(' ', 'T')) : 0) || 0;
  const endAtMs = Number(parsed?.end_at_ms || 0) || (meta.end_time ? Date.parse(meta.end_time.replace(' ', 'T')) : 0) || 0;

  return {
    id: note.id,
    title: note.title || meta.title || 'Ticket',
    category,
    startAtMs: Number.isFinite(startAtMs) ? startAtMs : 0,
    endAtMs: Number.isFinite(endAtMs) ? endAtMs : 0,
    updatedAt: note.updated_at || 0,
    meta: {
      origin: meta.origin || meta.flight_info?.origin || '',
      destination: meta.destination || meta.flight_info?.destination || '',
      venue: meta.venue || '',
      number: meta.number || meta.flight_info?.flight_number || '',
      start_time: meta.start_time || '',
      end_time: meta.end_time || '',
      seat: meta.seat || meta.seat_info || '',
      coach: meta.coach || meta.hall || '',
      gate: meta.gate || meta.platform || '',
      passenger: meta.passenger || meta.passenger_name || '',
      pickup_code: meta.pickup_code || '',
      booking_id: meta.booking_id || '',
      media_id: meta.media_id || '',
    },
    rawNote: note,
  };
}

const tickets = computed(() =>
  (props.notes || [])
    .map(parseTicketNote)
    .filter(Boolean)
    .sort((a, b) => (b.startAtMs || b.updatedAt) - (a.startAtMs || a.updatedAt)),
);

const walletGroups = computed(() => {
  const now = Date.now();
  const expiryCutoff = now - 4 * 3600 * 1000;
  const upcoming = [];
  const undated = [];
  const expired = [];
  for (const item of tickets.value) {
    if (!item.startAtMs) {
      undated.push(item);
    } else if (item.startAtMs >= expiryCutoff) {
      upcoming.push(item);
    } else {
      expired.push(item);
    }
  }
  upcoming.sort((a, b) => a.startAtMs - b.startAtMs);
  return { upcoming, undated, expired };
});

function categoryLabel(cat) {
  switch (cat) {
    case 'flight':
      return t('ticketCategoryFlight');
    case 'train':
    case 'rail':
      return t('ticketCategoryTrain');
    case 'movie':
    case 'film':
      return t('ticketCategoryMovie');
    case 'exhibition':
    case 'museum':
      return t('ticketCategoryExhibition');
    case 'concert':
      return t('ticketCategoryConcert');
    default:
      return t('ticketCategoryGeneral');
  }
}

function formatTicketTime(ms, rawStr) {
  if (rawStr) return rawStr;
  if (!ms) return '';
  try {
    const d = new Date(ms);
    const yyyy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, '0');
    const dd = String(d.getDate()).padStart(2, '0');
    const hh = String(d.getHours()).padStart(2, '0');
    const mi = String(d.getMinutes()).padStart(2, '0');
    return `${yyyy}-${mm}-${dd} ${hh}:${mi}`;
  } catch {
    return '';
  }
}

function formatClockOnly(ms, rawStr) {
  if (rawStr && rawStr.includes(' ')) {
    return rawStr.split(' ')[1].slice(0, 5);
  }
  if (!ms) return '--:--';
  try {
    return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
  } catch {
    return '--:--';
  }
}

function formatDateOnly(ms, rawStr) {
  if (rawStr && rawStr.includes(' ')) {
    return rawStr.split(' ')[0];
  }
  if (!ms) return t('ticketStatusUndated');
  try {
    const d = new Date(ms);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  } catch {
    return '';
  }
}

function openCreateForm() {
  Object.assign(form, {
    id: `ticket-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    category: 'train',
    title: '',
    number: '',
    origin: '',
    destination: '',
    venue: '',
    startTime: '',
    endTime: '',
    seat: '',
    coach: '',
    gate: '',
    passenger: '',
    pickupCode: '',
    bookingId: '',
    mediaId: '',
  });
  showManualForm.value = true;
}

function openEditForm(item) {
  const toLocalInput = (ms, raw) => {
    if (raw && raw.includes(' ')) return raw.replace(' ', 'T').slice(0, 16);
    if (!ms) return '';
    const d = new Date(ms);
    return new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
  };
  Object.assign(form, {
    id: item.id,
    category: item.category || 'general',
    title: item.title || '',
    number: item.meta.number || '',
    origin: item.meta.origin || '',
    destination: item.meta.destination || '',
    venue: item.meta.venue || '',
    startTime: toLocalInput(item.startAtMs, item.meta.start_time),
    endTime: toLocalInput(item.endAtMs, item.meta.end_time),
    seat: item.meta.seat || '',
    coach: item.meta.coach || '',
    gate: item.meta.gate || '',
    passenger: item.meta.passenger || '',
    pickupCode: item.meta.pickup_code || '',
    bookingId: item.meta.booking_id || '',
    mediaId: item.meta.media_id || '',
  });
  showManualForm.value = true;
}

function submitTicketForm() {
  const autoTitle =
    form.title.trim() ||
    (form.origin.trim() && form.destination.trim()
      ? `${form.origin.trim()} - ${form.destination.trim()}`
      : form.venue.trim() || form.number.trim());
  if (!autoTitle) return;

  const normalizeDt = v => (v ? v.replace('T', ' ') : '');
  const startStr = normalizeDt(form.startTime);
  const endStr = normalizeDt(form.endTime);
  const startAt = form.startTime ? Date.parse(form.startTime) : null;
  const endAt = form.endTime ? Date.parse(form.endTime) : null;

  const summaryParts = [];
  if (form.number.trim()) summaryParts.push(form.number.trim());
  if (form.seat.trim()) summaryParts.push(`${t('ticketSeatLabel')}: ${form.seat.trim()}`);
  if (form.gate.trim()) summaryParts.push(`${t('ticketGateLabel')}: ${form.gate.trim()}`);

  emit('save-ticket', {
    id: form.id || `ticket-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    title: autoTitle,
    category: form.category || 'general',
    startAt: Number.isFinite(startAt) ? startAt : null,
    endAt: Number.isFinite(endAt) ? endAt : null,
    notesSummary: summaryParts.join(' · '),
    ticketMeta: {
      category: form.category || 'general',
      origin: form.origin.trim() || null,
      destination: form.destination.trim() || null,
      venue: form.venue.trim() || null,
      number: form.number.trim() || null,
      start_time: startStr || null,
      end_time: endStr || null,
      seat: form.seat.trim() || null,
      coach: form.coach.trim() || null,
      gate: form.gate.trim() || null,
      passenger: form.passenger.trim() || null,
      pickup_code: form.pickupCode.trim() || null,
      booking_id: form.bookingId.trim() || null,
      media_id: form.mediaId || null,
    },
  });
  showManualForm.value = false;
}

function handleFileSelect(event) {
  const file = event.target?.files?.[0];
  if (event.target) event.target.value = '';
  if (!file) return;
  emit('recognize-ticket-file', file);
}

function toggleDetail(id) {
  activeDetailId.value = activeDetailId.value === id ? null : id;
}
</script>

<template>
  <div class="ticket-wallet-panel">
    <div class="ticket-wallet-toolbar hub-card">
      <input
        ref="fileInputRef"
        type="file"
        class="capture-file-input"
        accept="image/png,image/jpeg,image/webp"
        @change="handleFileSelect"
      />
      <button
        type="button"
        class="primary-action-btn"
        :disabled="busy || recognizing"
        @click="fileInputRef?.click()"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 6h4l2-3h4l2 3h4v15H4z" />
          <circle cx="12" cy="13" r="4" />
        </svg>
        <span>{{ t(recognizing ? 'ticketRecognizingBtn' : 'ticketUploadAndRecognizeBtn') }}</span>
      </button>
      <button
        type="button"
        class="mini-ghost-btn"
        :disabled="busy || recognizing"
        @click="showManualForm ? (showManualForm = false) : openCreateForm()"
      >
        + {{ t('ticketManualCreateBtn') }}
      </button>
    </div>

    <form
      v-if="showManualForm"
      class="ticket-manual-form hub-card"
      @submit.prevent="submitTicketForm"
    >
      <div class="ticket-form-grid">
        <CustomSelect
          v-model="form.category"
          :options="categoryOptions"
          class="ticket-category-select"
        />
        <input
          v-model="form.title"
          type="text"
          :placeholder="t('ticketTitlePlaceholder')"
          required
        />
        <input
          v-model="form.number"
          type="text"
          :placeholder="t('ticketNumberLabel')"
        />
        <input
          v-model="form.origin"
          type="text"
          :placeholder="t('ticketOriginLabel')"
        />
        <input
          v-model="form.destination"
          type="text"
          :placeholder="t('ticketDestinationLabel')"
        />
        <input
          v-model="form.venue"
          type="text"
          :placeholder="t('ticketVenueLabel')"
        />
        <input
          v-model="form.startTime"
          type="datetime-local"
          :aria-label="t('ticketStartTimeLabel')"
        />
        <input
          v-model="form.endTime"
          type="datetime-local"
          :aria-label="t('ticketEndTimeLabel')"
        />
        <input
          v-model="form.seat"
          type="text"
          :placeholder="t('ticketSeatLabel')"
        />
        <input
          v-model="form.coach"
          type="text"
          :placeholder="t('ticketCoachLabel')"
        />
        <input
          v-model="form.gate"
          type="text"
          :placeholder="t('ticketGateLabel')"
        />
        <input
          v-model="form.passenger"
          type="text"
          :placeholder="t('ticketPassengerLabel')"
        />
        <input
          v-model="form.pickupCode"
          type="text"
          :placeholder="t('ticketPickupCodeLabel')"
        />
      </div>
      <div class="ticket-form-actions">
        <button type="button" class="mini-ghost-btn" @click="showManualForm = false">
          {{ t('scheduleClose') }}
        </button>
        <button type="submit" class="primary-action-btn" :disabled="busy">
          {{ t('ticketSaveBtn') }}
        </button>
      </div>
    </form>

    <div v-if="!tickets.length" class="view-empty-card hub-card">
      <p>{{ t('ticketEmptyHint') }}</p>
    </div>

    <div v-else class="ticket-groups-container">
      <section
        v-for="group in [
          { key: 'upcoming', label: t('ticketGroupUpcoming'), list: walletGroups.upcoming, expired: false },
          { key: 'undated', label: t('ticketGroupUndated'), list: walletGroups.undated, expired: false },
          { key: 'expired', label: t('ticketGroupExpired'), list: walletGroups.expired, expired: true },
        ].filter(g => g.list.length > 0)"
        :key="group.key"
        class="ticket-group-section"
      >
        <header class="ticket-group-header">
          <strong>{{ group.label }}</strong>
          <span class="subtab-count">{{ group.list.length }}</span>
        </header>

        <div class="ticket-cards-grid">
          <article
            v-for="item in group.list"
            :key="item.id"
            class="boarding-pass-card hub-card"
            :class="[item.category, { expired: group.expired }]"
          >
            <div class="pass-top-bar" @click="toggleDetail(item.id)">
              <div class="pass-title-group">
                <span class="pass-cat-badge">{{ categoryLabel(item.category) }}</span>
                <strong class="pass-title">{{ item.title }}</strong>
                <span v-if="item.meta.number" class="pass-number-pill">{{ item.meta.number }}</span>
              </div>
              <div class="pass-date-group">
                <span class="pass-date-text">{{ formatDateOnly(item.startAtMs, item.meta.start_time) }}</span>
                <span class="schedule-status-badge" :class="group.expired ? 'cancelled' : 'scheduled'">
                  {{ group.expired ? t('ticketStatusExpired') : item.startAtMs ? t('ticketStatusUpcoming') : t('ticketStatusUndated') }}
                </span>
              </div>
            </div>

            <div
              v-if="item.meta.origin || item.meta.destination"
              class="pass-route-band"
              @click="toggleDetail(item.id)"
            >
              <div class="pass-station-col">
                <span class="pass-station-name">{{ item.meta.origin || '--' }}</span>
                <strong class="pass-clock">{{ formatClockOnly(item.startAtMs, item.meta.start_time) }}</strong>
              </div>
              <div class="pass-route-arrow">
                <span class="pass-route-line"></span>
                <span v-if="item.meta.number" class="pass-route-code">{{ item.meta.number }}</span>
                <span class="pass-route-line"></span>
              </div>
              <div class="pass-station-col align-end">
                <span class="pass-station-name">{{ item.meta.destination || '--' }}</span>
                <strong class="pass-clock">{{ item.meta.end_time ? formatClockOnly(item.endAtMs, item.meta.end_time) : '--:--' }}</strong>
              </div>
            </div>

            <div v-else-if="item.meta.venue" class="pass-venue-band" @click="toggleDetail(item.id)">
              <span>{{ t('ticketVenueLabel') }}: <strong>{{ item.meta.venue }}</strong></span>
              <span v-if="item.startAtMs || item.meta.start_time">
                {{ formatTicketTime(item.startAtMs, item.meta.start_time) }}
              </span>
            </div>

            <div class="pass-meta-strip">
              <span v-if="item.meta.seat">
                {{ t('ticketSeatLabel') }}: <strong>{{ item.meta.seat }}</strong>
              </span>
              <span v-if="item.meta.coach">
                {{ t('ticketCoachLabel') }}: <strong>{{ item.meta.coach }}</strong>
              </span>
              <span v-if="item.meta.gate">
                {{ t('ticketGateLabel') }}: <strong>{{ item.meta.gate }}</strong>
              </span>
              <span v-if="item.meta.passenger">
                {{ t('ticketPassengerLabel') }}: <strong>{{ item.meta.passenger }}</strong>
              </span>
              <span v-if="item.meta.pickup_code">
                {{ t('ticketPickupCodeLabel') }}: <strong>{{ item.meta.pickup_code }}</strong>
              </span>
            </div>

            <div v-if="activeDetailId === item.id && item.meta.media_id" class="pass-media-preview">
              <CaptureImage :media-id="item.meta.media_id" />
            </div>

            <div class="pass-actions-row">
              <button type="button" class="mini-ghost-btn" @click="openEditForm(item)">
                {{ t('ticketEditBtn') }}
              </button>
              <button type="button" class="mini-ghost-btn" @click="emit('delete-ticket', item)">
                {{ t('ticketDeleteBtn') }}
              </button>
            </div>
          </article>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.ticket-wallet-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.ticket-wallet-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 10px 14px;
}
.ticket-manual-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px;
}
.ticket-form-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 8px;
}
.ticket-form-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.ticket-groups-container {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.ticket-group-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.ticket-group-header {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.85rem;
  color: var(--text-secondary);
}
.ticket-cards-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 12px;
}
.boarding-pass-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px;
  border-left: 3px solid var(--user-accent, var(--accent-primary));
  transition: opacity 0.2s ease;
}
.boarding-pass-card.expired {
  opacity: 0.68;
  border-left-color: var(--text-tertiary);
}
.pass-top-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  cursor: pointer;
}
.pass-title-group {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
}
.pass-cat-badge {
  font-size: 0.72rem;
  padding: 2px 7px;
  border-radius: 999px;
  background: var(--bg-tertiary);
  color: var(--user-accent, var(--accent-primary));
  font-weight: 600;
}
.pass-title {
  font-size: 0.95rem;
  color: var(--text-primary);
}
.pass-number-pill {
  font-size: 0.75rem;
  padding: 1px 6px;
  border-radius: 6px;
  background: var(--bg-secondary);
  color: var(--text-secondary);
}
.pass-date-group {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 0.78rem;
  color: var(--text-secondary);
}
.pass-route-band {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 10px;
  background: var(--bg-secondary);
  cursor: pointer;
}
.pass-station-col {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.pass-station-col.align-end {
  align-items: flex-end;
}
.pass-station-name {
  font-size: 0.8rem;
  color: var(--text-secondary);
}
.pass-clock {
  font-size: 1.15rem;
  font-variant-numeric: tabular-nums;
  color: var(--text-primary);
}
.pass-route-arrow {
  display: flex;
  align-items: center;
  flex: 1;
  gap: 6px;
  color: var(--text-tertiary);
  font-size: 0.75rem;
}
.pass-route-line {
  flex: 1;
  height: 1px;
  border-bottom: 1px dashed var(--border-subtle);
}
.pass-venue-band {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 12px;
  border-radius: 8px;
  background: var(--bg-secondary);
  font-size: 0.84rem;
  color: var(--text-secondary);
  cursor: pointer;
}
.pass-meta-strip {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 14px;
  font-size: 0.8rem;
  color: var(--text-secondary);
}
.pass-meta-strip strong {
  color: var(--text-primary);
}
.pass-actions-row {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding-top: 6px;
  border-top: 1px dashed var(--border-subtle);
}
</style>
