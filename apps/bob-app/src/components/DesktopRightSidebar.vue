<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  schedules: {
    type: Array,
    default: () => [],
  },
  inbox: {
    type: Object,
    default: () => null,
  },
});

const emit = defineEmits(['navigate', 'open-quick-note']);

const { t, locale } = useI18n();

const todayLabel = computed(() => {
  const d = new Date();
  if (locale.value && locale.value.startsWith('zh')) {
    return `${t('navToday')} · ${d.getMonth() + 1}月${d.getDate()}日`;
  }
  return `${t('navToday')} · ${d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })}`;
});

const todayScheduleItems = computed(() => {
  if (!props.schedules || !props.schedules.length) {
    return [];
  }
  return props.schedules.slice(0, 5).map(s => ({
    id: s.id,
    time: s.due_at ? s.due_at.slice(11, 16) : (s.time || '--:--'),
    title: s.title || s.content,
    auto: Boolean(s.cron || s.auto),
  }));
});

const todayTotalCount = computed(() => (props.schedules || []).length);
const todayCompletedCount = computed(() => {
  return (props.schedules || []).filter(s => s.status === 'completed' || s.status === 'done').length;
});

const pendingList = computed(() => {
  const list = [];
  const draft = props.inbox?.triageDraft?.batch ? props.inbox.triageDraft : props.inbox?.triageDraft?.value;
  if (draft?.batch?.suggestions?.length) {
    const suggs = draft.batch.suggestions.filter(s => !s._applied);
    if (suggs.length > 0) {
      list.push({
        id: 'triage-batch',
        icon: 'inbox',
        iconColor: 'var(--text-secondary)',
        title: `${t('inboxDailyReviewBadge')} ${suggs.length} ${t('itemsUnit')}`,
        tag: t('chipTriage'),
        tagClass: 'gray',
        action: 'inbox',
      });
    }
  }
  return list;
});

const unsortedSparks = computed(() => {
  const items = Array.isArray(props.inbox?.filteredItems)
    ? props.inbox.filteredItems
    : props.inbox?.filteredItems?.value;
  if (items && items.length) {
    const unsorted = items.filter(
      item => item.triage_status === 'unsorted' || !item.triage_status
    );
    if (unsorted.length > 0) {
      return unsorted.slice(0, 3).map(s => ({
        id: s.id,
        title: s.content || s.title || '...',
      }));
    }
  }
  return [];
});

const unsortedCountNumber = computed(() => {
  if (typeof props.inbox?.unsortedCount === 'number') {
    return props.inbox.unsortedCount;
  }
  if (props.inbox?.unsortedCount?.value !== undefined) {
    return props.inbox.unsortedCount.value;
  }
  return unsortedSparks.value.length;
});

function handleCardClick(drawer) {
  emit('navigate', drawer);
}

function handleRowClick(item) {
  if (item.action) {
    emit('navigate', item.action);
  }
}
</script>

<template>
  <aside class="desktop-right-sidebar" aria-label="Desktop Context Panel">
    <!-- Card 1: 今天 -->
    <div class="card desktop-context-card" @click="handleCardClick('schedule')">
      <div class="panel-title">
        <span>{{ todayLabel }}</span>
        <small>{{ todayCompletedCount }} / {{ todayTotalCount }} {{ t('todosUnit') }}</small>
      </div>
      <div v-if="!todayScheduleItems.length" class="empty-context-hint">
        {{ t('noScheduleToday') }}
      </div>
      <div v-for="item in todayScheduleItems" :key="item.id" class="mini-row">
        <span class="t">{{ item.time }}</span>
        <span class="x" :title="item.title">{{ item.title }}</span>
        <span v-if="item.auto" class="chip gray">{{ t('autoChip') }}</span>
      </div>
    </div>

    <!-- Card 2: 待确认 -->
    <div class="card desktop-context-card">
      <div class="panel-title">
        <span>{{ t('pendingConfirmationsTitle') }}</span>
        <small>{{ pendingList.length }} {{ t('itemsUnit') }}</small>
      </div>
      <div v-if="!pendingList.length" class="empty-context-hint">
        {{ t('noPendingConfirmations') }}
      </div>
      <div v-for="item in pendingList" :key="item.id" class="mini-row" @click.stop="handleRowClick(item)">
        <svg class="ic xs mini-icon" :style="{ color: item.iconColor }" viewBox="0 0 24 24" aria-hidden="true">
          <path v-if="item.icon === 'shield'" d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" fill="none" stroke="currentColor" stroke-width="2"/>
          <path v-else-if="item.icon === 'note'" d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6M16 13H8M16 17H8M10 9H8" fill="none" stroke="currentColor" stroke-width="2"/>
          <path v-else-if="item.icon === 'clock'" d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm1 11h-5V7h2v4h3z" fill="none" stroke="currentColor" stroke-width="2"/>
          <polyline v-else points="22 12 16 12 14 15 10 15 8 12 2 12" fill="none" stroke="currentColor" stroke-width="2"/>
        </svg>
        <span class="x" :title="item.title">{{ item.title }}</span>
        <span class="chip" :class="item.tagClass">{{ item.tag }}</span>
      </div>
    </div>

    <!-- Card 3: 收集箱 -->
    <div class="card desktop-context-card" @click="handleCardClick('inbox')">
      <div class="panel-title">
        <span>{{ t('inboxTitle') }}</span>
        <small>{{ unsortedCountNumber }} {{ t('inboxUnsortedSmall') }}</small>
      </div>
      <div v-if="!unsortedSparks.length" class="empty-context-hint">
        {{ t('inboxEmpty') }}
      </div>
      <div v-for="spark in unsortedSparks" :key="spark.id" class="mini-row">
        <span class="x" :title="spark.title">{{ spark.title }}</span>
      </div>
      <div class="muted small inbox-shortcut-hint" @click.stop="emit('open-quick-note')">
        {{ t('inboxShortcutPrefix') }} <span class="kbd">{{ t('inboxShortcutKey') }}</span> {{ t('inboxShortcutSuffix') }}
      </div>
    </div>
  </aside>
</template>

<style scoped>
.empty-context-hint {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  padding: 0.35rem 0.2rem;
  font-style: italic;
}
</style>
