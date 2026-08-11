<template>
  <section class="work-project-accordion" :class="{ expanded }">
    <button class="work-project-trigger" type="button" :aria-expanded="expanded" :aria-controls="contentId" @click="$emit('toggle')">
      <span class="project-status-dot" :class="{ active: project.status === 'active' }" aria-hidden="true"></span>
      <span class="project-title">{{ project.title }}</span>
      <span class="project-count">{{ countLabel }}</span>
      <ChevronDown class="project-chevron" :size="17" aria-hidden="true" />
    </button>
    <div v-if="expanded" :id="contentId" class="work-project-content">
      <div v-if="loading" class="project-feedback" aria-live="polite"><LoaderCircle class="spin" :size="17" /><span>{{ t('work.mobile_loading') }}</span></div>
      <div v-else-if="error" class="project-feedback error" role="alert">
        <CircleAlert :size="17" /><span>{{ t('work.mobile_load_failed') }}</span>
        <button class="btn btn-secondary btn-compact" type="button" @click.stop="$emit('retry')">{{ t('work.mobile_retry') }}</button>
      </div>
      <slot v-else />
    </div>
  </section>
</template>

<script setup>
import { computed } from 'vue';
import { ChevronDown, CircleAlert, LoaderCircle } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
const props = defineProps({ project: { type: Object, required: true }, expanded: { type: Boolean, default: false }, countLabel: { type: String, default: '' }, loading: { type: Boolean, default: false }, error: { type: String, default: '' } });
defineEmits(['toggle', 'retry']);
const { t } = useI18n();
const contentId = computed(() => `mobile-work-project-${props.project.id}`);
</script>

<style scoped>
.work-project-accordion { border-bottom: 1px solid var(--border-subtle); }
.work-project-trigger { display: grid; grid-template-columns: 8px minmax(0, 1fr) auto 18px; align-items: center; gap: 10px; width: 100%; min-height: 54px; border: 0; padding: 10px 2px; color: var(--text-primary); background: transparent; font: inherit; text-align: left; cursor: pointer; }
.project-status-dot { width: 7px; height: 7px; box-sizing: border-box; border: 1.5px solid var(--text-muted); border-radius: 50%; background: transparent; }
.project-status-dot.active { border-color: var(--user-accent, var(--accent-primary)); background: var(--user-accent, var(--accent-primary)); }
.project-title { overflow: hidden; font-size: 14px; font-weight: 620; text-overflow: ellipsis; white-space: nowrap; }
.project-count { color: var(--text-muted); font-size: 11px; white-space: nowrap; }
.project-chevron { color: var(--text-muted); transition: transform 160ms ease; }
.expanded .project-chevron { transform: rotate(180deg); }
.work-project-content { padding: 0 0 16px 18px; }
.project-feedback { display: flex; align-items: center; gap: 8px; min-height: 46px; color: var(--text-tertiary); font-size: 12px; }
.project-feedback span { flex: 1; }
.project-feedback.error { color: var(--text-secondary); }
.spin { animation: spin .8s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
