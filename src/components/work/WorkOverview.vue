<template>
  <div class="mobile-overview">
    <div class="overview-row"><span>{{ t('work.mobile_current_phase') }}</span><strong>{{ aggregate.project.currentPhase || t('work.phase_unset') }}</strong></div>
    <div class="overview-row"><span>{{ t('work.mobile_next_step') }}</span><strong>{{ nextTask?.title || t('work.empty_tasks') }}</strong></div>
    <div class="overview-row"><span>{{ t('work.mobile_recent_progress') }}</span><strong>{{ latestEvent ? eventLabel(latestEvent) : t('work.empty_activity') }}</strong><time v-if="latestEvent">{{ formatTime(latestEvent.createdAt) }}</time></div>
  </div>
</template>

<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { getOpenTasks } from '../../work/work-view-state.js';
const props = defineProps({ aggregate: { type: Object, required: true } });
const { t, locale } = useI18n();
const nextTask = computed(() => getOpenTasks(props.aggregate)[0]);
const latestEvent = computed(() => props.aggregate.recentEvents?.[0]);
function eventLabel(event) {
  const type = event?.eventType || '';
  if (type === 'project.created') return t('work.event_project_created');
  if (type.endsWith('.created')) return t('work.event_item_created');
  if (type.endsWith('.status_changed')) return t('work.event_status_changed');
  if (type === 'relation.created') return t('work.event_relation_created');
  if (type === 'external_link.recorded') return t('work.event_external_link_recorded');
  if (type.startsWith('change_review.')) return t(`work.event_${type.replace('.', '_')}`, type);
  return type || t('work.empty_activity');
}
function formatTime(timestamp) { return timestamp ? new Intl.DateTimeFormat(locale.value, { month: 'short', day: 'numeric' }).format(new Date(timestamp)) : ''; }
</script>

<style scoped>
.mobile-overview { display: grid; }
.overview-row { display: grid; grid-template-columns: minmax(72px, .34fr) minmax(0, 1fr) auto; gap: 10px; align-items: baseline; min-height: 39px; border-top: 1px solid var(--border-subtle); padding: 9px 2px; }
.overview-row:first-child { border-top: 0; }
.overview-row > span, .overview-row time { color: var(--text-muted); font-size: 10px; }
.overview-row strong { min-width: 0; color: var(--text-secondary); font-size: 12px; line-height: 1.45; }
.overview-row time { white-space: nowrap; }
</style>
