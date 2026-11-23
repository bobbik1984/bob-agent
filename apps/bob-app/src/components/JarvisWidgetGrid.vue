<script setup>
import { computed, inject, ref, unref, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { todayItems } from '../today-items.mjs';
import FeatureWidgetFrame from './FeatureWidgetFrame.vue';
import FeatureTrialBanner from './FeatureTrialBanner.vue';

const shell = inject('bobAppShell', {});
const { t } = useI18n();

const jarvisHome = computed(() => shell.jarvisHome);
const personalOps = computed(() => shell.personalOps);
const inbox = computed(() => shell.inbox);
const modelHub = computed(() => shell.modelHub);
const switchPrimaryDrawer = shell.switchPrimaryDrawer;
const openQuickNoteOverlay = shell.openQuickNoteOverlay;
const focusComposer = shell.focusComposer;
const updateDraft = shell.updateDraft;

const sparkInput = ref('');
onMounted(() => shell.loadPersonalOps?.());

const pendingApprovalCount = computed(
  () => jarvisHome.value?.pendingApprovalCount?.value ?? 0,
);

const todaySchedules = computed(() => {
  const list = unref(personalOps.value)?.schedules || [];
  return todayItems(list).slice(0, 3);
});

const unsortedInboxCount = computed(() => {
  const list = inbox.value?.captures || [];
  return list.filter(c => !c.triaged && !c.archived).length;
});

const connectedNodesCount = computed(() => {
  const nodes = modelHub.value?.state?.remote_nodes || modelHub.value?.value?.state?.remote_nodes || [];
  return nodes.filter(n => n.enabled !== false).length;
});

function getWidgetTitle(id) {
  switch (id) {
    case 'pending':
      return t('jarvisWidgetPendingTitle');
    case 'today':
      return t('jarvisWidgetTodayTitle');
    case 'inbox':
      return t('jarvisWidgetInboxTitle');
    case 'routines':
      return t('jarvisWidgetRoutinesTitle');
    case 'quotas':
      return t('jarvisWidgetQuotasTitle');
    case 'nodes':
      return t('jarvisWidgetNodesTitle');
    default:
      return id;
  }
}

async function submitSpark() {
  const text = sparkInput.value.trim();
  if (!text) return;
  if (typeof inbox.value?.captureSpark === 'function') {
    await inbox.value.captureSpark(text);
  } else if (typeof openQuickNoteOverlay === 'function') {
    openQuickNoteOverlay(text);
  }
  sparkInput.value = '';
}

function startCreateRoutine() {
  const prompt = t('jarvisCreateRoutinePrompt');
  if (shell.draft) {
    shell.draft.value = prompt;
  }
  if (typeof updateDraft === 'function') {
    updateDraft({ target: { value: prompt } });
  }
  if (typeof focusComposer === 'function') {
    focusComposer();
  }
}

function handleReviewPending() {
  if (jarvisHome.value) {
    jarvisHome.value.widgetSheetOpen.value = true;
  }
  const pendingSection = document.querySelector('.pending-approvals-stream, .widget-pending');
  if (pendingSection && typeof pendingSection.scrollIntoView === 'function') {
    pendingSection.scrollIntoView({ behavior: 'smooth' });
  }
}
</script>

<template>
  <section class="jarvis-widget-section" aria-label="Jarvis Widgets">
    <!-- Persistent 3-Minute Trial Bar -->
    <FeatureTrialBanner />

    <!-- Header bar with calm title & customize toggle -->
    <header class="widget-section-header">
      <h2 class="widget-section-title">{{ t('jarvisWidgetsTitle') }}</h2>
      <button
        v-if="jarvisHome"
        type="button"
        class="widget-customize-btn"
        :class="{ active: jarvisHome.editingWidgets.value }"
        :aria-expanded="jarvisHome.editingWidgets.value"
        @click="jarvisHome.editingWidgets.value = !jarvisHome.editingWidgets.value"
      >
        <svg
          class="widget-header-svg"
          viewBox="0 0 24 24"
          width="15"
          height="15"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M12 20h9" />
          <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
        </svg>
        <span>{{ t(jarvisHome.editingWidgets.value ? 'jarvisWidgetsDone' : 'jarvisCustomizeWidgets') }}</span>
      </button>
    </header>

    <!-- Customization Panel when editingWidgets is active -->
    <div
      v-if="jarvisHome && jarvisHome.editingWidgets.value"
      class="widget-customization-panel"
    >
      <div class="customization-list">
        <template v-for="w in jarvisHome.widgets.value" :key="w.id">
          <!-- Standard L1 or active Pro widget item -->
          <div
            v-if="w.tier !== 'pro' || jarvisHome.hasProCluster.value"
            class="customization-item"
          >
            <div class="item-meta">
              <span class="item-name">{{ getWidgetTitle(w.id) }}</span>
              <span v-if="w.tier === 'pro'" class="item-tier-badge">Pro</span>
            </div>
            <div class="item-actions">
              <button
                type="button"
                class="reorder-btn"
                :title="t('jarvisMoveUp')"
                :aria-label="t('jarvisMoveUp')"
                @click="jarvisHome.moveWidget(w.id, -1)"
              >
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="m18 15-6-6-6 6" />
                </svg>
              </button>
              <button
                type="button"
                class="reorder-btn"
                :title="t('jarvisMoveDown')"
                :aria-label="t('jarvisMoveDown')"
                @click="jarvisHome.moveWidget(w.id, 1)"
              >
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="m6 9 6 6 6-6" />
                </svg>
              </button>
              <label class="widget-switch">
                <input
                  type="checkbox"
                  class="visually-hidden"
                  :checked="w.enabled"
                  @change="jarvisHome.toggleWidgetEnabled(w.id)"
                />
                <span class="switch-track" aria-hidden="true">
                  <span class="switch-thumb"></span>
                </span>
              </label>
            </div>
          </div>
        </template>

        <!-- Custom Feature Widgets in customization list -->
        <template v-if="jarvisHome.installedFeatures?.value?.length">
          <div class="customization-group-header">
            <span class="group-title">{{ t('featureInstalledTitle') }}</span>
          </div>
          <div
            v-for="feat in jarvisHome.installedFeatures.value"
            :key="'cust-' + feat.name"
            class="customization-item"
          >
            <div class="item-meta">
              <span class="item-name">{{ feat.title || feat.name }}</span>
              <span class="item-tier-badge">v{{ feat.version || 1 }}</span>
              <span class="item-tier-badge custom-badge">{{ t('featureCustomBadge') }}</span>
            </div>
            <div class="item-actions">
              <button
                type="button"
                class="feature-rollback-btn"
                :title="t('featureRollbackBtn')"
                :aria-label="t('featureRollbackBtn')"
                @click="jarvisHome.rollbackFeature(feat.name)"
              >
                <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                  <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
                  <path d="M3 3v5h5" />
                </svg>
                <span>{{ t('featureRollbackBtn') }}</span>
              </button>
              <label class="widget-switch">
                <input
                  type="checkbox"
                  class="visually-hidden"
                  :checked="feat.enabled !== false"
                  @change="jarvisHome.toggleFeature(feat.name, feat.enabled === false)"
                />
                <span class="switch-track" aria-hidden="true">
                  <span class="switch-thumb"></span>
                </span>
              </label>
            </div>
          </div>
        </template>
      </div>

      <!-- Advanced / Developer Widgets collapsible when no Pro cluster is active -->
      <details
        v-if="!jarvisHome.hasProCluster.value"
        class="advanced-widgets-disclosure"
      >
        <summary class="advanced-summary">
          <span>{{ t('jarvisAdvancedWidgets') }}</span>
          <svg class="disclosure-chevron" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
            <path d="m9 6 6 6-6 6" />
          </svg>
        </summary>
        <div class="advanced-body">
          <div
            v-for="w in jarvisHome.widgets.value.filter(item => item.tier === 'pro')"
            :key="w.id"
            class="customization-item"
          >
            <div class="item-meta">
              <span class="item-name">{{ getWidgetTitle(w.id) }}</span>
              <span class="item-tier-badge">Pro</span>
            </div>
            <div class="item-actions">
              <label class="widget-switch">
                <input
                  type="checkbox"
                  class="visually-hidden"
                  :checked="w.enabled"
                  @change="jarvisHome.toggleWidgetEnabled(w.id)"
                />
                <span class="switch-track" aria-hidden="true">
                  <span class="switch-thumb"></span>
                </span>
              </label>
            </div>
          </div>
        </div>
      </details>

      <footer class="customization-footer">
        <button
          type="button"
          class="ghost-btn"
          @click="jarvisHome.resetWidgets"
        >
          {{ t('jarvisResetWidgets') }}
        </button>
        <button
          type="button"
          class="primary-btn"
          @click="jarvisHome.editingWidgets.value = false"
        >
          {{ t('jarvisWidgetsDone') }}
        </button>
      </footer>
    </div>

    <!-- Active Widget Grid -->
    <div v-if="jarvisHome" class="jarvis-widget-grid">
      <template v-for="w in jarvisHome.visibleWidgets.value" :key="w.id">
        <!-- Widget: Pending Approvals & Takeovers -->
        <article
          v-if="w.id === 'pending'"
          class="widget-card widget-pending"
        >
          <header class="widget-card-header">
            <div class="widget-icon-chip chip-warning" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
              </svg>
            </div>
            <h3 class="widget-card-title">{{ t('jarvisWidgetPendingTitle') }}</h3>
            <span v-if="pendingApprovalCount > 0" class="widget-badge badge-warning">
              {{ pendingApprovalCount }}
            </span>
          </header>
          <div class="widget-card-body">
            <p class="widget-desc">
              {{ t('jarvisWidgetPendingDesc', { count: pendingApprovalCount }) }}
            </p>
          </div>
          <footer class="widget-card-footer">
            <button
              type="button"
              class="widget-action-link"
              @click="handleReviewPending"
            >
              {{ t('jarvisWidgetPendingReview') }}
            </button>
          </footer>
        </article>

        <!-- Widget: Today's Schedule & Todos -->
        <article
          v-else-if="w.id === 'today'"
          class="widget-card widget-today"
        >
          <header class="widget-card-header">
            <div class="widget-icon-chip chip-accent" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="3" y="4" width="18" height="18" rx="2" ry="2" />
                <line x1="16" y1="2" x2="16" y2="6" />
                <line x1="8" y1="2" x2="8" y2="6" />
                <line x1="3" y1="10" x2="21" y2="10" />
              </svg>
            </div>
            <h3 class="widget-card-title">{{ t('jarvisWidgetTodayTitle') }}</h3>
          </header>
          <div class="widget-card-body">
            <div v-if="todaySchedules.length > 0" class="widget-schedule-list">
              <div
                v-for="item in todaySchedules"
                :key="item.id"
                class="schedule-row"
              >
                <span class="schedule-bullet" aria-hidden="true"></span>
                <span class="schedule-title">{{ item.title }}</span>
              </div>
            </div>
            <p v-else class="widget-empty-hint">
              {{ t('jarvisWidgetTodayEmpty') }}
            </p>
          </div>
          <footer class="widget-card-footer">
            <button
              type="button"
              class="widget-action-link"
              @click="switchPrimaryDrawer && switchPrimaryDrawer('schedule')"
            >
              {{ t('jarvisWidgetTodayOpen') }}
            </button>
          </footer>
        </article>

        <!-- Widget: Quick Spark Inbox -->
        <article
          v-else-if="w.id === 'inbox'"
          class="widget-card widget-inbox"
        >
          <header class="widget-card-header">
            <div class="widget-icon-chip chip-accent" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="2">
                <path d="m12 3-1.912 5.813a2 2 0 0 1-1.275 1.275L3 12l5.813 1.912a2 2 0 0 1 1.275 1.275L12 21l1.912-5.813a2 2 0 0 1 1.275-1.275L21 12l-5.813-1.912a2 2 0 0 1-1.275-1.275L12 3Z" />
              </svg>
            </div>
            <h3 class="widget-card-title">{{ t('jarvisWidgetInboxTitle') }}</h3>
            <span v-if="unsortedInboxCount > 0" class="widget-badge badge-neutral">
              {{ t('jarvisWidgetInboxUnsorted', { count: unsortedInboxCount }) }}
            </span>
          </header>
          <div class="widget-card-body">
            <form class="spark-input-form" @submit.prevent="submitSpark">
              <input
                v-model="sparkInput"
                type="text"
                class="spark-input"
                :placeholder="t('jarvisQuickSparkPlaceholder')"
              />
              <button
                type="submit"
                class="spark-submit-btn"
                :disabled="!sparkInput.trim()"
                :aria-label="t('send')"
              >
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M5 12h14M12 5l7 7-7 7" />
                </svg>
              </button>
            </form>
          </div>
        </article>

        <!-- Widget: Daily Routines & Briefs -->
        <article
          v-else-if="w.id === 'routines'"
          class="widget-card widget-routines"
        >
          <header class="widget-card-header">
            <div class="widget-icon-chip chip-accent" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="23 4 23 10 17 10" />
                <polyline points="1 20 1 14 7 14" />
                <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
              </svg>
            </div>
            <h3 class="widget-card-title">{{ t('jarvisWidgetRoutinesTitle') }}</h3>
          </header>
          <div class="widget-card-body">
            <p class="widget-desc">{{ t('jarvisWidgetRoutinesSummary') }}</p>
          </div>
          <footer class="widget-card-footer">
            <button
              type="button"
              class="widget-action-link"
              @click="startCreateRoutine"
            >
              {{ t('jarvisWidgetCreateRoutine') }}
            </button>
          </footer>
        </article>

        <!-- Widget: Subscription Quotas & Monthly Budget (Pro) -->
        <article
          v-else-if="w.id === 'quotas'"
          class="widget-card widget-quotas"
        >
          <header class="widget-card-header">
            <div class="widget-icon-chip chip-accent" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M12 2v20M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
              </svg>
            </div>
            <h3 class="widget-card-title">{{ t('jarvisWidgetQuotasTitle') }}</h3>
          </header>
          <div class="widget-card-body">
            <div class="budget-progress-block">
              <div class="budget-meta-row">
                <span class="budget-label">{{ t('jarvisMonthlyBudget') }}</span>
                <span class="budget-values">
                  ${{ Number(jarvisHome?.budgetState?.value?.spent_usd || 0).toFixed(2) }} /
                  ${{ Number(jarvisHome?.budgetState?.value?.monthly_cap_usd || 10).toFixed(2) }}
                </span>
              </div>
              <div class="progress-bar-track" role="meter">
                <div
                  class="progress-bar-fill"
                  :style="{
                    width:
                      Math.min(
                        100,
                        Math.round(
                          ((jarvisHome?.budgetState?.value?.spent_usd || 0) /
                            (jarvisHome?.budgetState?.value?.monthly_cap_usd || 10)) *
                            100,
                        ),
                      ) + '%',
                  }"
                ></div>
              </div>
            </div>
          </div>
        </article>

        <!-- Widget: Active Nodes & Native Sessions (Pro) -->
        <article
          v-else-if="w.id === 'nodes'"
          class="widget-card widget-nodes"
        >
          <header class="widget-card-header">
            <div class="widget-icon-chip chip-accent" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="2" y="2" width="20" height="8" rx="2" ry="2" />
                <rect x="2" y="14" width="20" height="8" rx="2" ry="2" />
                <line x1="6" y1="6" x2="6.01" y2="6" />
                <line x1="6" y1="18" x2="6.01" y2="18" />
              </svg>
            </div>
            <h3 class="widget-card-title">{{ t('jarvisWidgetNodesTitle') }}</h3>
          </header>
          <div class="widget-card-body">
            <p class="widget-desc">
              {{ t('jarvisWidgetNodesCount', { count: connectedNodesCount }) }}
            </p>
          </div>
          <footer class="widget-card-footer">
            <button
              type="button"
              class="widget-action-link"
              @click="switchPrimaryDrawer && switchPrimaryDrawer('work')"
            >
              {{ t('jarvisWidgetNodesOpen') }}
            </button>
          </footer>
        </article>
      </template>

      <!-- Custom Feature Widgets (Zero-Credential Sandboxed FeatureWidgetFrame) -->
      <FeatureWidgetFrame
        v-for="feat in (jarvisHome?.customFeatureWidgets?.value || [])"
        :key="feat.name"
        :feature="feat"
      />

      <!-- Slot for custom feature widgets (WP-J3 FeatureWidgetFrame) -->
      <slot name="custom-widgets" />
    </div>
  </section>
</template>

<style scoped>
.jarvis-widget-section {
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
  width: 100%;
  max-width: 900px;
  margin: var(--gap-md) auto;
  box-sizing: border-box;
}

.widget-section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--gap-2xs);
}

.widget-section-title {
  margin: 0;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--text-secondary);
  letter-spacing: 0.02em;
  text-transform: uppercase;
}

.widget-customize-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--gap-3xs);
  background: transparent;
  border: 1px solid transparent;
  color: var(--text-tertiary);
  font-size: var(--text-xs);
  cursor: pointer;
  padding: 0.25rem 0.5rem;
  border-radius: var(--radius-pill);
  transition: color 0.15s ease, background 0.15s ease;
}

.widget-customize-btn:hover,
.widget-customize-btn.active {
  color: var(--text-primary);
  background: var(--bg-secondary);
  border-color: var(--line);
}

/* Customization Panel */
.widget-customization-panel {
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  border-radius: var(--radius-card);
  padding: var(--gap-md);
  margin-bottom: var(--gap-xs);
}

.customization-list {
  display: flex;
  flex-direction: column;
  gap: var(--gap-xs);
}

.customization-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--gap-xs) var(--gap-sm);
  background: var(--bg-primary);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
}

.item-meta {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
}

.item-name {
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--text-primary);
}

.item-tier-badge {
  font-size: var(--text-2xs);
  padding: 0.1rem 0.35rem;
  border-radius: var(--radius-pill);
  background: var(--accent-soft);
  color: var(--accent-primary);
  font-weight: 600;
}

.item-actions {
  display: flex;
  align-items: center;
  gap: var(--gap-2xs);
}

.reorder-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.6rem;
  height: 1.6rem;
  border-radius: var(--radius-sm);
  background: transparent;
  border: 1px solid var(--line);
  color: var(--text-secondary);
  cursor: pointer;
}

.reorder-btn:hover {
  background: var(--bg-secondary);
  color: var(--text-primary);
}

/* Switch */
.widget-switch {
  position: relative;
  display: inline-block;
  width: 2.2rem;
  height: 1.3rem;
  cursor: pointer;
}

.switch-track {
  position: absolute;
  inset: 0;
  background: var(--bg-tertiary);
  border: 1px solid var(--line);
  border-radius: var(--radius-pill);
  transition: background 0.2s ease, border-color 0.2s ease;
}

.switch-thumb {
  position: absolute;
  top: 1.5px;
  left: 2px;
  width: 0.95rem;
  height: 0.95rem;
  border-radius: var(--radius-pill);
  background: var(--text-primary);
  transition: transform 0.2s ease, background 0.2s ease;
}

.widget-switch input:checked + .switch-track {
  background: var(--accent-primary);
  border-color: var(--accent-primary);
}

.widget-switch input:checked + .switch-track .switch-thumb {
  transform: translateX(0.9rem);
  background: var(--text-on-accent);
}

.advanced-widgets-disclosure {
  border-radius: var(--radius-sm);
  border: 1px dashed var(--line);
  padding: var(--gap-xs);
  background: var(--bg-primary);
}

.advanced-summary {
  font-size: var(--text-xs);
  font-weight: 500;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--gap-2xs) var(--gap-xs);
  user-select: none;
}

.advanced-summary:hover {
  color: var(--text-primary);
}

.advanced-body {
  display: flex;
  flex-direction: column;
  gap: var(--gap-xs);
  padding-top: var(--gap-xs);
}

.customization-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--gap-sm);
  padding-top: var(--gap-xs);
  border-top: 1px solid var(--line);
}

.ghost-btn {
  background: transparent;
  border: 1px solid var(--line);
  color: var(--text-secondary);
  border-radius: var(--radius-btn);
  padding: 0.35rem 0.75rem;
  font-size: var(--text-xs);
  cursor: pointer;
}

.ghost-btn:hover {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.primary-btn {
  background: var(--accent-primary);
  border: 1px solid var(--accent-primary);
  color: var(--text-on-accent);
  border-radius: var(--radius-btn);
  padding: 0.35rem 0.85rem;
  font-size: var(--text-xs);
  font-weight: 500;
  cursor: pointer;
}

.primary-btn:hover {
  opacity: 0.92;
}

/* Grid Layout */
.jarvis-widget-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: var(--gap-md);
  width: 100%;
}

@media (max-width: 600px) {
  .jarvis-widget-grid {
    grid-template-columns: 1fr;
    gap: var(--gap-sm);
  }
}

/* Widget Card */
.widget-card {
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  border: 1px solid var(--line);
  border-radius: var(--radius-card);
  padding: var(--gap-md);
  box-shadow: var(--shadow);
  gap: var(--gap-xs);
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}

.widget-card:hover {
  border-color: color-mix(in srgb, var(--accent-primary) 35%, var(--line));
}

.widget-card.widget-pending {
  border-left: 3px solid var(--color-warning);
}

.widget-card-header {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
}

.widget-icon-chip {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.85rem;
  height: 1.85rem;
  border-radius: var(--radius-sm);
  flex-shrink: 0;
}

.chip-accent {
  background: var(--accent-soft);
  color: var(--accent-primary);
}

.chip-warning {
  background: var(--warning-soft);
  color: var(--warning-text);
}

.widget-card-title {
  margin: 0;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--text-primary);
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.widget-badge {
  font-size: var(--text-2xs);
  padding: 0.1rem 0.4rem;
  border-radius: var(--radius-pill);
  font-weight: 600;
  flex-shrink: 0;
}

.badge-warning {
  background: var(--warning-soft);
  color: var(--warning-text);
}

.badge-neutral {
  background: var(--bg-secondary);
  color: var(--text-secondary);
  border: 1px solid var(--line);
}

.widget-card-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: var(--gap-3xs);
}

.widget-desc {
  margin: 0;
  font-size: var(--text-xs);
  color: var(--text-secondary);
  line-height: 1.45;
}

.widget-empty-hint {
  margin: 0;
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  font-style: normal;
}

.widget-card-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  margin-top: auto;
  padding-top: var(--gap-2xs);
}

.widget-action-link {
  background: transparent;
  border: none;
  padding: 0;
  font-size: var(--text-xs);
  font-weight: 500;
  color: var(--accent-primary);
  cursor: pointer;
  transition: opacity 0.15s ease;
}

.widget-action-link:hover {
  text-decoration: underline;
  opacity: 0.9;
}

/* Schedule List in Widget */
.widget-schedule-list {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2xs);
}

.schedule-row {
  display: flex;
  align-items: center;
  gap: var(--gap-2xs);
  font-size: var(--text-xs);
  color: var(--text-primary);
}

.schedule-bullet {
  width: 4px;
  height: 4px;
  border-radius: var(--radius-pill);
  background: var(--accent-primary);
  flex-shrink: 0;
}

.schedule-title {
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Spark Input */
.spark-input-form {
  display: flex;
  align-items: center;
  gap: var(--gap-2xs);
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  padding: 0.2rem 0.35rem;
}

.spark-input {
  flex: 1;
  background: transparent;
  border: none;
  outline: none;
  font-size: var(--text-xs);
  color: var(--text-primary);
  padding: 0.2rem 0.25rem;
  min-width: 0;
}

.spark-submit-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.6rem;
  height: 1.6rem;
  border-radius: var(--radius-sm);
  background: var(--accent-primary);
  border: none;
  color: var(--text-on-accent);
  cursor: pointer;
  flex-shrink: 0;
  transition: opacity 0.15s ease;
}

.spark-submit-btn:disabled {
  opacity: 0.35;
  cursor: default;
}

/* Budget Progress Block */
.budget-progress-block {
  display: flex;
  flex-direction: column;
  gap: var(--gap-3xs);
}

.budget-meta-row {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  font-size: var(--text-2xs);
}

.budget-label {
  color: var(--text-secondary);
}

.budget-values {
  color: var(--text-primary);
  font-weight: 500;
}

.progress-bar-track {
  width: 100%;
  height: 6px;
  border-radius: var(--radius-pill);
  background: var(--bg-tertiary);
  overflow: hidden;
}

.progress-bar-fill {
  height: 100%;
  border-radius: var(--radius-pill);
  background: var(--accent-primary);
  transition: width 0.3s ease;
}

.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

.customization-group-header {
  padding: var(--gap-xs) var(--gap-2xs) var(--gap-3xs);
}

.group-title {
  font-size: var(--text-2xs);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-secondary);
}

.custom-badge {
  background: var(--bg-secondary);
  color: var(--accent-primary);
}

.feature-rollback-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--gap-3xs);
  padding: 3px var(--gap-2xs);
  font-size: var(--text-2xs);
  border-radius: var(--radius-xs);
  background: var(--bg-secondary);
  color: var(--text-secondary);
  border: 1px solid var(--line);
  cursor: pointer;
  transition: all var(--transition-fast, 0.15s ease);
}

.feature-rollback-btn:hover {
  color: var(--color-error);
  border-color: var(--color-error);
}
</style>
