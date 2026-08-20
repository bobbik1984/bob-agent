<template>
  <section class="today-card" :class="{ 'is-compact': compact }" :aria-busy="loading">
    <header class="today-card-header">
      <div class="today-card-heading">
        <ListChecks :size="16" />
        <span>{{ t('today.title') }}</span>
      </div>
      <span v-if="changedCount" class="today-change-count">
        {{ t('today.changed', { count: changedCount }) }}
      </span>
    </header>

    <div v-if="loading && !snapshot" class="today-loading" aria-live="polite">
      <span class="today-loading-line"></span>
      <span class="today-loading-line short"></span>
    </div>

    <div v-else-if="errorCode && !snapshot" class="today-empty" role="status">
      <CircleAlert :size="16" />
      <span>{{ t('today.unavailable') }}</span>
      <button type="button" class="today-text-button" @click="$emit('refresh')">{{ t('today.retry') }}</button>
    </div>

    <template v-else-if="snapshot">
      <button
        v-if="snapshot.focusItem"
        type="button"
        class="today-focus"
        @click="emitAction(snapshot.focusItem)"
      >
        <span class="today-item-kicker">{{ kindLabel(snapshot.focusItem.kind) }}</span>
        <strong>{{ itemTitle(snapshot.focusItem) }}</strong>
        <span v-if="itemSummary(snapshot.focusItem)" class="today-item-summary">{{ itemSummary(snapshot.focusItem) }}</span>
        <span v-if="snapshot.focusItem.action?.kind !== 'none'" class="today-item-action">
          {{ actionLabel(snapshot.focusItem.action?.kind) }}
          <ChevronRight :size="14" />
        </span>
      </button>

      <div v-if="snapshot.attentionItems?.length" class="today-attention-list">
        <button
          v-for="item in snapshot.attentionItems"
          :key="item.itemId"
          type="button"
          class="today-attention-item"
          @click="emitAction(item)"
        >
          <span class="today-attention-dot"></span>
          <span class="today-attention-copy">
            <strong>{{ itemTitle(item) }}</strong>
            <span v-if="itemSummary(item)">{{ itemSummary(item) }}</span>
          </span>
          <ChevronRight v-if="item.action?.kind !== 'none'" :size="14" />
        </button>
      </div>

      <div v-if="!snapshot.focusItem && !snapshot.attentionItems?.length" class="today-empty">
        <CircleCheck :size="16" />
        <span>{{ t('today.clear') }}</span>
      </div>

      <footer class="today-card-footer">
        <div class="today-counts" :aria-label="t('today.summary')">
          <span v-if="snapshot.sectionCounts?.today">{{ t('today.count_today', { count: snapshot.sectionCounts.today }) }}</span>
          <span v-if="snapshot.sectionCounts?.inProgress">{{ t('today.count_progress', { count: snapshot.sectionCounts.inProgress }) }}</span>
          <span v-if="snapshot.sectionCounts?.changes">{{ t('today.count_changes', { count: snapshot.sectionCounts.changes }) }}</span>
          <span v-if="snapshot.sectionCounts?.insights">{{ t('today.count_insights', { count: snapshot.sectionCounts.insights }) }}</span>
          <span v-if="!hasCounts">{{ t('today.no_more') }}</span>
        </div>
        <button v-if="canExpand" type="button" class="today-text-button" @click="$emit('expand')">
          {{ t('today.details') }}
        </button>
      </footer>

      <div v-if="snapshot.status !== 'fresh'" class="today-source-note">
        <CloudOff :size="13" />
        <span>{{ t(snapshot.status === 'stale' ? 'today.stale' : 'today.partial') }}</span>
      </div>
    </template>
  </section>
</template>

<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { ChevronRight, CircleAlert, CircleCheck, CloudOff, ListChecks } from 'lucide-vue-next';

const props = defineProps({
  snapshot: { type: Object, default: null },
  loading: { type: Boolean, default: false },
  errorCode: { type: String, default: '' },
  compact: { type: Boolean, default: false },
});
const emit = defineEmits(['action', 'expand', 'refresh']);
const { t, te } = useI18n();

const changedCount = computed(() => props.snapshot?.changedSinceLastSeen?.length || 0);
const hasCounts = computed(() => {
  const counts = props.snapshot?.sectionCounts || {};
  return ['today', 'inProgress', 'changes', 'insights'].some((key) => Number(counts[key]) > 0);
});
const canExpand = computed(() => (props.snapshot?.detailItems?.length || 0) > 0);

function translated(key, args) {
  return key && te(key) ? t(key, args || {}) : '';
}

function itemTitle(item) {
  return item.title || translated(item.titleKey, item.messageArgs) || t('today.untitled');
}

function itemSummary(item) {
  return item.summary || translated(item.summaryKey, item.messageArgs);
}

function kindLabel(kind) {
  const key = `today.kind.${kind || 'progress'}`;
  return te(key) ? t(key) : t('today.kind.progress');
}

function actionLabel(kind) {
  const key = `today.action.${kind || 'open_details'}`;
  return te(key) ? t(key) : t('today.action.open_details');
}

function emitAction(item) {
  if (item.action?.kind !== 'none') emit('action', item);
}
</script>

<style scoped>
.today-card {
  width: 100%;
  color: var(--text-primary);
  background: var(--bg-primary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  overflow: hidden;
}

.today-card-header,
.today-card-footer,
.today-item-action,
.today-empty,
.today-source-note,
.today-card-heading {
  display: flex;
  align-items: center;
}

.today-card-header {
  min-height: 42px;
  padding: 0 var(--space-4);
  justify-content: space-between;
  border-bottom: 1px solid var(--border-subtle);
}

.today-card-heading {
  gap: var(--space-2);
  font-size: var(--text-sm);
  font-weight: 600;
}

.today-change-count,
.today-item-kicker {
  color: var(--text-tertiary);
  font-size: var(--text-xs);
}

.today-focus {
  width: 100%;
  padding: var(--space-5);
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
  text-align: left;
  color: inherit;
  background: transparent;
  border: 0;
  cursor: pointer;
}

.today-focus:hover,
.today-attention-item:hover {
  background: var(--bg-hover);
}

.today-focus strong {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 16px;
  font-weight: 600;
}

.today-item-summary,
.today-attention-copy span {
  color: var(--text-secondary);
  font-size: var(--text-sm);
  line-height: 1.5;
}

.today-item-action {
  gap: 2px;
  margin-top: var(--space-1);
  color: var(--text-secondary);
  font-size: var(--text-xs);
}

.today-attention-list {
  border-top: 1px solid var(--border-subtle);
}

.today-attention-item {
  width: 100%;
  min-height: 50px;
  padding: var(--space-3) var(--space-4);
  display: grid;
  grid-template-columns: 6px minmax(0, 1fr) 14px;
  gap: var(--space-3);
  align-items: center;
  color: inherit;
  background: transparent;
  border: 0;
  border-bottom: 1px solid var(--border-subtle);
  text-align: left;
  cursor: pointer;
}

.today-attention-dot {
  width: 5px;
  height: 5px;
  border-radius: var(--radius-full);
  background: var(--text-secondary);
}

.today-attention-copy {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.today-attention-copy strong,
.today-attention-copy span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.today-attention-copy strong {
  font-size: var(--text-sm);
  font-weight: 500;
}

.today-card-footer {
  min-height: 40px;
  padding: 0 var(--space-4);
  justify-content: space-between;
  gap: var(--space-3);
}

.today-counts {
  min-width: 0;
  display: flex;
  gap: var(--space-3);
  overflow: hidden;
  color: var(--text-tertiary);
  font-size: var(--text-xs);
  white-space: nowrap;
}

.today-text-button {
  flex-shrink: 0;
  padding: 4px 0;
  color: var(--text-secondary);
  background: transparent;
  border: 0;
  font-size: var(--text-xs);
  cursor: pointer;
}

.today-text-button:hover {
  color: var(--text-primary);
}

.today-empty {
  min-height: 76px;
  justify-content: center;
  gap: var(--space-2);
  color: var(--text-secondary);
  font-size: var(--text-sm);
}

.today-source-note {
  gap: 6px;
  padding: 8px var(--space-4);
  color: var(--text-tertiary);
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-subtle);
  font-size: var(--text-xs);
}

.today-loading {
  padding: var(--space-6) var(--space-5);
}

.today-loading-line {
  display: block;
  width: 72%;
  height: 8px;
  margin-bottom: var(--space-3);
  border-radius: var(--radius-full);
  background: var(--border-subtle);
  animation: today-pulse 1.4s ease-in-out infinite;
}

.today-loading-line.short { width: 46%; }

.is-compact .today-focus { padding: var(--space-4); }
.is-compact .today-card-header { min-height: 38px; }

@keyframes today-pulse {
  50% { opacity: 0.45; }
}

@media (prefers-reduced-motion: reduce) {
  .today-loading-line { animation: none; }
}
</style>
