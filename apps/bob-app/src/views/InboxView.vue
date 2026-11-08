<script setup>
import { inject, ref, computed, onMounted } from 'vue';

const shell = inject('bobAppShell', {});
const t = shell.t || (k => k);
const inbox = shell.inbox;

// Fully unwrapped top-level reactive accessors
const inboxItems = computed(() => inbox?.inboxItems?.value || []);
const unsortedCount = computed(() => inbox?.unsortedCount?.value ?? 0);
const sortedCount = computed(() => inbox?.sortedCount?.value ?? 0);
const laterCount = computed(() => inbox?.laterCount?.value ?? 0);
const filteredItems = computed(() => inbox?.filteredItems?.value || []);
const triageDraft = computed(() => inbox?.triageDraft?.value || null);
const triageDraftBusy = computed(() => inbox?.triageDraftBusy?.value || false);
const triageStatusNotice = computed(() => inbox?.triageStatusNotice?.value || '');

const activeProjects = computed(() => {
  const projects = shell.catalog?.value?.projects || shell.projectList?.value || [];
  return projects.filter(p => p.id && !p.id.startsWith('assistant:') && p.source_kind !== 'legacy_import');
});

const pendingSuggestions = computed(() => {
  if (!triageDraft.value?.batch?.suggestions) return [];
  return triageDraft.value.batch.suggestions.filter(s => !s._applied);
});

const captureInput = computed({
  get: () => inbox?.captureInput?.value || '',
  set: (v) => { if (inbox?.captureInput) inbox.captureInput.value = v; },
});
const captureBusy = computed(() => inbox?.captureBusy?.value || false);

const inboxFilter = computed({
  get: () => inbox?.inboxFilter?.value || 'all',
  set: (v) => { if (inbox?.inboxFilter) inbox.inboxFilter.value = v; },
});

const editingIndex = ref(-1);
const customDestDraft = ref({ kind: 'note', label: '' });

function getSparkText(item) {
  return item.content || item.content_ref || item.contentRef || '';
}

function getSparkTitle(item) {
  const text = getSparkText(item);
  const firstLine = text.trim().split('\n')[0] || '';
  return firstLine.slice(0, 120);
}

function getSparkBody(item) {
  const text = getSparkText(item);
  const lines = text.trim().split('\n');
  if (lines.length > 1) {
    return lines.slice(1).join(' ').trim().slice(0, 200);
  }
  return '';
}

function formatTimestamp(ts) {
  if (!ts) return '';
  const d = new Date(ts);
  const now = new Date();
  const isToday = d.toDateString() === now.toDateString();
  const timeStr = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (isToday) return timeStr;
  return `${d.getMonth() + 1}/${d.getDate()} ${timeStr}`;
}

function getSuggestionForItem(itemId) {
  if (!triageDraft.value?.batch?.suggestions) return null;
  const idx = triageDraft.value.batch.suggestions.findIndex(
    s => Array.isArray(s.spark_ids) && s.spark_ids.includes(itemId)
  );
  if (idx === -1) return null;
  return {
    suggestion: triageDraft.value.batch.suggestions[idx],
    index: idx,
  };
}

function startEditSuggestion(index, sugg) {
  editingIndex.value = index;
  customDestDraft.value = {
    kind: sugg.destination,
    label: sugg.destination === 'project_todo' ? (sugg.project_title || sugg.project_id) : (sugg.title || ''),
  };
}

function saveCustomEdit(index) {
  if (!triageDraft.value?.batch?.suggestions) return;
  const sugg = triageDraft.value.batch.suggestions[index];
  if (!sugg) return;
  sugg.destination = customDestDraft.value.kind;
  if (customDestDraft.value.kind === 'project_todo') {
    sugg.project_title = customDestDraft.value.label;
  } else {
    sugg.title = customDestDraft.value.label;
  }
  editingIndex.value = -1;
}

function changeDestination(sugg, newKind) {
  sugg.destination = newKind;
  if (newKind === 'project_todo' && !sugg.project_id && activeProjects.value.length) {
    sugg.project_id = activeProjects.value[0].id;
    sugg.project_title = activeProjects.value[0].title;
  }
}

function onSuggestionProjectChange(sugg, event) {
  const projId = event.target.value;
  const proj = activeProjects.value.find(p => p.id === projId);
  sugg.project_id = projId;
  sugg.project_title = proj ? proj.title : projId;
}

function getChipClass(kind) {
  if (kind === 'calendar') return 'chip blue';
  if (kind === 'project_todo') return 'chip green';
  if (kind === 'note') return 'chip violet';
  if (kind === 'keep') return 'chip gray';
  return 'chip gray';
}

function getDestBadgeLabel(target) {
  if (!target) return '';
  if (target.destination === 'project_todo' || target.destination_kind === 'project_todo') {
    return target.project_title || target.project_id || target.destination_label || t('destProjectTodo');
  }
  if (target.destination === 'calendar' || target.destination_kind === 'calendar') {
    const time = target.start_time || target.destination_label;
    return time ? `${t('destCalendar')} · ${time}` : t('destCalendar');
  }
  if (target.destination === 'note' || target.destination_kind === 'note') return t('destNote');
  if (target.destination === 'keep' || target.destination_kind === 'keep') return t('destKeep');
  if (target.destination === 'merged' || target.destination_kind === 'merged') return t('destMerged');
  return target.destination_label || target.destination || target.destination_kind || '';
}

function getSuggestionSelectValue(sugg) {
  if (!sugg) return '';
  if (sugg.destination === 'project_todo') {
    return `project:${sugg.project_id || (activeProjects.value[0]?.id || '')}`;
  }
  return sugg.destination;
}

function handleSuggestionSelectChange(sugg, event) {
  const val = event.target.value;
  if (val.startsWith('project:')) {
    const projId = val.slice(8);
    const proj = activeProjects.value.find(p => p.id === projId);
    sugg.destination = 'project_todo';
    sugg.project_id = projId;
    sugg.project_title = proj ? proj.title : projId;
  } else {
    sugg.destination = val;
  }
}

function getDestKindLabel(kind) {
  if (kind === 'calendar') return t('destCalendar');
  if (kind === 'project_todo') return t('destProjectTodo');
  if (kind === 'note') return t('destNote');
  if (kind === 'keep') return t('destKeep');
  if (kind === 'merged') return t('destMerged');
  return kind;
}

function handleCaptureKeydown(e) {
  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
    e.preventDefault();
    submitCapture();
  }
}

function submitCapture() {
  if (inbox?.submitZeroDecisionCapture) {
    inbox.submitZeroDecisionCapture();
  }
}

function triggerTriageNow() {
  if (inbox?.loadTriageDraft) {
    inbox.loadTriageDraft(true);
  }
}

function dismissDraft() {
  if (inbox?.dismissTriageDraft) {
    inbox.dismissTriageDraft();
  }
}

function applySuggestion(idx) {
  if (inbox?.applyTriageSuggestion) {
    inbox.applyTriageSuggestion(idx);
  }
}

function applyAll() {
  if (inbox?.applyAllTriageSuggestions) {
    inbox.applyAllTriageSuggestions();
  }
}

function handleManualAssign(item, event) {
  const rawVal = event.target.value;
  if (!rawVal) return;
  try {
    const dest = JSON.parse(rawVal);
    if (inbox?.manuallySetItemDestination) {
      inbox.manuallySetItemDestination(item.id, dest);
    }
  } catch (err) {
    console.error('Failed to parse manual assign destination:', err);
  }
  event.target.value = '';
}

function handleReopenItem(item) {
  if (inbox?.manuallySetItemDestination) {
    inbox.manuallySetItemDestination(item.id, null);
  }
}

onMounted(() => {
  if (inbox?.loadInbox) {
    inbox.loadInbox();
  }
  if (inbox?.loadTriageDraft) {
    inbox.loadTriageDraft(false);
  }
});
</script>

<template>
  <div class="inbox-view" role="region" :aria-label="t('inboxTitle')">
    <!-- Top Header & Filter Bar -->
    <header class="inbox-header-bar">
      <div class="inbox-title-group">
        <h2>{{ t('inboxTitle') }}</h2>
        <small>{{ t('inboxUnsortedCount', { count: unsortedCount }) }}</small>
      </div>

      <div class="inbox-filter-pills" role="tablist">
        <button
          type="button"
          class="filter-pill"
          :class="{ active: inboxFilter === 'all' }"
          @click="inboxFilter = 'all'"
        >
          {{ t('filterAll') }} ({{ inboxItems.length }})
        </button>
        <button
          type="button"
          class="filter-pill"
          :class="{ active: inboxFilter === 'unsorted' }"
          @click="inboxFilter = 'unsorted'"
        >
          {{ t('filterUnsorted') }} ({{ unsortedCount }})
        </button>
        <button
          type="button"
          class="filter-pill"
          :class="{ active: inboxFilter === 'later' }"
          @click="inboxFilter = 'later'"
        >
          {{ t('filterLater') }} ({{ laterCount }})
        </button>
        <button
          type="button"
          class="filter-pill"
          :class="{ active: inboxFilter === 'sorted' }"
          @click="inboxFilter = 'sorted'"
        >
          {{ t('filterSorted') }} ({{ sortedCount }})
        </button>
      </div>

      <button
        type="button"
        class="btn ghost tiny"
        :disabled="triageDraftBusy || !unsortedCount"
        @click="triggerTriageNow"
      >
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <path d="m12 3 1.912 5.886a2 2 0 0 0 1.272 1.272L21 12l-5.886 1.912a2 2 0 0 0-1.272 1.272L12 21l-1.912-5.886a2 2 0 0 0 1.272-1.272L3 12l5.886-1.912a2 2 0 0 0 1.272-1.272L12 3z"/>
        </svg>
        <span>{{ t('inboxTriageNowBtn') }}</span>
      </button>
    </header>

    <!-- Main Scrollable Body (Single Unified Stream) -->
    <div class="inbox-scroll-body">
      <!-- Zero-decision Quick Capture Box (Always at top) -->
      <section class="inbox-capture-box" aria-label="Zero-decision Capture">
        <textarea
          v-model="captureInput"
          class="inbox-capture-input"
          :placeholder="t('inboxCapturePlaceholder')"
          :disabled="captureBusy"
          rows="2"
          @keydown="handleCaptureKeydown"
        ></textarea>
        <div class="inbox-capture-bottom">
          <div class="inbox-capture-tools">
            <span class="inbox-capture-hint">{{ t('inboxCaptureHint') }}</span>
          </div>
          <button
            type="button"
            class="btn primary tiny"
            :disabled="captureBusy || !captureInput.trim()"
            @click="submitCapture"
          >
            {{ t('inboxCaptureSaveBtn') }}
          </button>
        </div>
      </section>

      <!-- Compact Daily Review Banner (Only when active AI suggestions exist) -->
      <section
        v-if="pendingSuggestions.length > 0"
        class="daily-review-banner"
        aria-label="Daily Review Summary"
      >
        <div class="daily-review-banner-main">
          <div class="daily-review-badge">
            <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
              <path d="m12 3 1.912 5.886a2 2 0 0 0 1.272 1.272L21 12l-5.886 1.912a2 2 0 0 0-1.272 1.272L12 21l-1.912-5.886a2 2 0 0 0 1.272-1.272L3 12l5.886-1.912a2 2 0 0 0 1.272-1.272L12 3z"/>
            </svg>
            <span>{{ t('inboxDailyReviewBadge') }}</span>
            <span class="daily-review-count-pill">{{ pendingSuggestions.length }}</span>
          </div>
          <div class="daily-review-headline">
            {{ t('inboxDailyReviewHeadline', { count: pendingSuggestions.length }) }}
          </div>
        </div>
        <div class="daily-review-banner-actions">
          <button
            type="button"
            class="btn ghost tiny"
            :disabled="triageDraftBusy"
            @click="dismissDraft"
          >
            {{ t('inboxDismissTodayBtn') }}
          </button>
          <button
            type="button"
            class="btn primary tiny"
            :disabled="triageDraftBusy"
            @click="applyAll"
          >
            {{ t('inboxConfirmAllBtn') }}
          </button>
        </div>
      </section>

      <!-- Unified Sparks List (Each item displayed only once) -->
      <section class="spark-list" aria-label="Sparks List">
        <!-- Empty State -->
        <div v-if="!filteredItems.length" class="card empty inbox-empty-state">
          <div class="inbox-empty-icon" aria-hidden="true">
            <svg viewBox="0 0 24 24"><polyline points="22 12 16 12 14 15 10 15 8 12 2 12" /><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" /></svg>
          </div>
          <h4>{{ t('inboxTitle') }}</h4>
          <p class="inbox-empty-desc">
            {{ t('inboxZeroDecisionDesc') || t('inboxEmptyFiltered') }}
          </p>
        </div>

        <!-- Spark Item Cards -->
        <article
          v-for="item in filteredItems"
          :key="item.id"
          class="spark-card"
          :class="{
            'is-sorted': item.triage_status === 'sorted',
            'has-suggestion': getSuggestionForItem(item.id) && !getSuggestionForItem(item.id).suggestion._applied && item.triage_status !== 'sorted'
          }"
        >
          <!-- Top Row: Spark Title + Destination Tag -->
          <div class="spark-card-header">
            <strong class="spark-card-title">{{ getSparkTitle(item) }}</strong>

            <div class="spark-card-tag-wrap">
              <!-- Sorted: Confirmed Tag -->
              <span v-if="item.triage_status === 'sorted'" :class="getChipClass(item.destination_kind)">
                {{ getDestBadgeLabel(item) }}
              </span>

              <!-- Has unapplied AI suggestion -->
              <span
                v-else-if="getSuggestionForItem(item.id) && !getSuggestionForItem(item.id).suggestion._applied"
                :class="getChipClass(getSuggestionForItem(item.id).suggestion.destination)"
              >
                {{ getDestBadgeLabel(getSuggestionForItem(item.id).suggestion) }}
              </span>

              <!-- Unsorted without suggestion -->
              <span v-else class="chip gray">{{ t('inboxStatusUnsorted') }}</span>
            </div>
          </div>

          <!-- Full Width Spark Body -->
          <p v-if="getSparkBody(item)" class="spark-card-body">{{ getSparkBody(item) }}</p>

          <!-- AI Reasoning Sentence (if suggested and not yet applied) -->
          <div
            v-if="getSuggestionForItem(item.id) && !getSuggestionForItem(item.id).suggestion._applied && item.triage_status !== 'sorted' && getSuggestionForItem(item.id).suggestion.reason"
            class="sugg-reason"
          >
            {{ getSuggestionForItem(item.id).suggestion.reason }}
          </div>

          <!-- Inline Edit Form (when adjusting) -->
          <div v-if="editingIndex === getSuggestionForItem(item.id)?.index" class="sugg-edit-row">
            <input
              v-model="customDestDraft.label"
              class="filter-pill"
              style="border-radius: var(--radius-sm); flex: 1; min-width: 120px;"
              :placeholder="t('inboxCustomLabelPlaceholder')"
            />
            <button type="button" class="btn primary tiny" @click="saveCustomEdit(getSuggestionForItem(item.id).index)">
              {{ t('save') }}
            </button>
            <button type="button" class="btn ghost tiny" @click="editingIndex = -1">
              {{ t('cancel') }}
            </button>
          </div>

          <!-- Bottom Footer Row -->
          <!-- State A: Active Suggestion -->
          <div
            v-if="getSuggestionForItem(item.id) && !getSuggestionForItem(item.id).suggestion._applied && item.triage_status !== 'sorted' && editingIndex !== getSuggestionForItem(item.id)?.index"
            class="spark-card-footer sugg-bottom-toolbar"
          >
            <div class="sugg-picker-group">
              <span class="sugg-picker-label">{{ t('inboxSwitchTo') }}:</span>
              <select
                class="sugg-dest-select"
                :value="getSuggestionSelectValue(getSuggestionForItem(item.id).suggestion)"
                @change="handleSuggestionSelectChange(getSuggestionForItem(item.id).suggestion, $event)"
              >
                <optgroup :label="t('modeProjectShort')">
                  <option
                    v-for="p in activeProjects"
                    :key="p.id"
                    :value="`project:${p.id}`"
                  >
                    {{ p.title }}
                  </option>
                </optgroup>
                <optgroup :label="t('inboxOtherDest')">
                  <option value="note">{{ t('destNote') }}</option>
                  <option value="calendar">{{ t('destCalendar') }}</option>
                  <option value="keep">{{ t('destKeep') }}</option>
                </optgroup>
              </select>
            </div>

            <div class="sugg-actions-row">
              <button
                type="button"
                class="btn ghost tiny"
                @click="startEditSuggestion(getSuggestionForItem(item.id).index, getSuggestionForItem(item.id).suggestion)"
              >
                {{ t('inboxAdjustBtn') }}
              </button>
              <button
                type="button"
                class="btn primary tiny"
                :disabled="triageDraftBusy"
                @click="applySuggestion(getSuggestionForItem(item.id).index)"
              >
                {{ t('confirm') }}
              </button>
            </div>
          </div>

          <!-- State B: Already Sorted -->
          <div v-else-if="item.triage_status === 'sorted'" class="spark-card-footer spark-card-meta-row">
            <div class="spark-meta-left">
              <span>{{ formatTimestamp(item.created_at) }}</span>
              <span class="review-status-indicator" style="color: var(--color-success);">
                <svg class="inline-svg-xs" viewBox="0 0 24 24"><polyline points="20 6 9 17 4 12"/></svg>
                <span>{{ t('inboxConfirmedTag') }}</span>
              </span>
            </div>
            <div class="spark-meta-right">
              <button
                type="button"
                class="btn ghost tiny"
                @click="handleReopenItem(item)"
              >
                {{ t('inboxReopenBtn') }}
              </button>
            </div>
          </div>

          <!-- State C: Unsorted without suggestion (Manual Project/Destination Assignment) -->
          <div v-else class="spark-card-footer spark-card-meta-row">
            <div class="spark-meta-left">
              <span>{{ formatTimestamp(item.created_at) }}</span>
              <span v-if="item.media_id" class="chip gray" style="font-size: 10px;">
                <svg class="inline-svg-xs" viewBox="0 0 24 24"><rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>
                {{ t('photo') }}
              </span>
            </div>
            <div class="spark-meta-right">
              <div class="manual-assign-wrap">
                <select
                  class="manual-assign-select"
                  @change="handleManualAssign(item, $event)"
                >
                  <option value="" disabled selected>{{ t('inboxAssignToProject') }}</option>
                  <optgroup :label="t('modeProjectShort')">
                    <option
                      v-for="p in activeProjects"
                      :key="p.id"
                      :value="JSON.stringify({ kind: 'project_todo', refId: p.id, label: p.title })"
                    >
                      {{ p.title }}
                    </option>
                  </optgroup>
                  <optgroup :label="t('inboxOtherDest')">
                    <option :value="JSON.stringify({ kind: 'note', label: getSparkTitle(item) })">
                      {{ t('destNote') }}
                    </option>
                    <option :value="JSON.stringify({ kind: 'calendar', label: getSparkTitle(item) })">
                      {{ t('destCalendar') }}
                    </option>
                    <option :value="JSON.stringify({ kind: 'keep', label: 'Keep' })">
                      {{ t('destKeep') }}
                    </option>
                  </optgroup>
                </select>
              </div>
            </div>
          </div>
        </article>
      </section>
    </div>
  </div>
</template>

<style scoped>
.inbox-empty-state {
  text-align: center;
  padding: 3rem 1.5rem;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  border: 1px dashed var(--line);
  background: var(--bg-primary);
  border-radius: var(--radius-card);
}

.inbox-empty-icon {
  width: 44px;
  height: 44px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  background: var(--bg-secondary);
  color: var(--text-tertiary);
  margin-bottom: 0.25rem;
}

.inbox-empty-icon svg {
  width: 24px;
  height: 24px;
  stroke: currentColor;
  stroke-width: 1.8;
  fill: none;
}

.inbox-empty-state h4 {
  font-size: var(--text-md);
  color: var(--text-primary);
  margin: 0;
}

.inbox-empty-desc {
  font-size: var(--text-sm);
  color: var(--text-secondary);
  max-width: 420px;
  line-height: 1.5;
  margin: 0;
}
</style>
