<script setup>
import { computed, inject, onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';

const shell = inject('bobAppShell', {});
const { t } = useI18n();

const jarvisHome = computed(() => shell.jarvisHome);
const trialFeatures = computed(() => jarvisHome.value?.trialFeatures?.value || []);

const now = ref(Date.now());
let timer = null;
const autoRolledBack = new Set();

function getRemainingSeconds(feat) {
  if (!feat.trial_expires_at) return 180;
  return Math.max(0, Math.ceil((feat.trial_expires_at - now.value) / 1000));
}

function formatRemaining(feat) {
  const secs = getRemainingSeconds(feat);
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}:${s < 10 ? '0' : ''}${s}`;
}

async function checkAutoRollback() {
  const list = trialFeatures.value;
  for (let i = 0; i < list.length; i++) {
    const feat = list[i];
    if (feat.trial_expires_at && getRemainingSeconds(feat) === 0 && !autoRolledBack.has(feat.name)) {
      autoRolledBack.add(feat.name);
      try {
        if (typeof jarvisHome.value?.rollbackFeature === 'function') {
          await jarvisHome.value.rollbackFeature(feat.name);
        } else if (typeof jarvisHome.value?.refreshFeatures === 'function') {
          await jarvisHome.value.refreshFeatures();
        }
      } catch {}
    }
  }
}

onMounted(() => {
  if (typeof setInterval !== 'undefined') {
    timer = setInterval(() => {
      now.value = Date.now();
      checkAutoRollback();
    }, 1000);
  }
});

onUnmounted(() => {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
});
</script>

<template>
  <div
    v-if="trialFeatures.length > 0"
    class="feature-trial-banner"
    role="region"
    :aria-label="t('featureTrialBannerTitle')"
  >
    <div
      v-for="feat in trialFeatures"
      :key="feat.name"
      class="trial-banner-item"
    >
      <div class="trial-meta-lead">
        <div class="trial-icon-chip" aria-hidden="true">
          <svg
            viewBox="0 0 24 24"
            width="16"
            height="16"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
          </svg>
        </div>
        <div class="trial-info-wrap">
          <div class="trial-title-row">
            <span class="trial-title">{{ feat.title || feat.name }}</span>
            <span class="trial-version-badge">v{{ feat.version || 1 }}</span>
            <span class="trial-tag">{{ t('featureTrialBannerTitle') }}</span>
          </div>
          <div class="trial-countdown-row">
            <svg
              class="trial-clock-icon"
              viewBox="0 0 24 24"
              width="12"
              height="12"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <circle cx="12" cy="12" r="10" />
              <polyline points="12 6 12 12 16 14" />
            </svg>
            <span class="trial-countdown-text">
              {{ t('featureTrialRemaining') }}: <strong class="trial-timer">{{ formatRemaining(feat) }}</strong>
            </span>
          </div>
        </div>
      </div>

      <div class="trial-actions">
        <button
          type="button"
          class="trial-keep-btn"
          :title="t('featureTrialKeep')"
          :aria-label="t('featureTrialKeep')"
          @click="jarvisHome && jarvisHome.keepFeature(feat.name)"
        >
          <svg
            viewBox="0 0 24 24"
            width="13"
            height="13"
            fill="none"
            stroke="currentColor"
            stroke-width="2.2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <polyline points="20 6 9 17 4 12" />
          </svg>
          <span>{{ t('featureTrialKeep') }}</span>
        </button>
        <button
          type="button"
          class="trial-undo-btn"
          :title="t('featureTrialUndo')"
          :aria-label="t('featureTrialUndo')"
          @click="jarvisHome && jarvisHome.rollbackFeature(feat.name)"
        >
          <svg
            viewBox="0 0 24 24"
            width="13"
            height="13"
            fill="none"
            stroke="currentColor"
            stroke-width="2.2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
            <path d="M3 3v5h5" />
          </svg>
          <span>{{ t('featureTrialUndo') }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.feature-trial-banner {
  display: flex;
  flex-direction: column;
  gap: var(--gap-xs);
  width: 100%;
  box-sizing: border-box;
}

.trial-banner-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-sm);
  padding: var(--gap-xs) var(--gap-sm);
  border-radius: var(--radius-md);
  background: var(--surface);
  border: 1px solid var(--line);
  box-sizing: border-box;
}

.trial-meta-lead {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
  min-width: 0;
}

.trial-icon-chip {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-sm);
  background: var(--bg-secondary);
  color: var(--accent-primary);
  flex-shrink: 0;
}

.trial-info-wrap {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.trial-title-row {
  display: flex;
  align-items: center;
  gap: var(--gap-2xs);
  flex-wrap: wrap;
}

.trial-title {
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.trial-version-badge {
  display: inline-flex;
  align-items: center;
  padding: 1px var(--gap-3xs);
  font-size: var(--text-2xs);
  font-weight: 600;
  border-radius: var(--radius-xs);
  background: var(--bg-primary);
  color: var(--accent-primary);
  border: 1px solid var(--line);
}

.trial-tag {
  display: inline-flex;
  align-items: center;
  font-size: var(--text-2xs);
  font-weight: 500;
  color: var(--color-warning);
}

.trial-countdown-row {
  display: flex;
  align-items: center;
  gap: var(--gap-3xs);
  font-size: var(--text-2xs);
  color: var(--text-secondary);
}

.trial-clock-icon {
  flex-shrink: 0;
  color: var(--color-warning);
}

.trial-countdown-text {
  font-variant-numeric: tabular-nums;
}

.trial-timer {
  font-weight: 700;
  color: var(--accent-primary);
}

.trial-actions {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
  flex-shrink: 0;
}

.trial-keep-btn,
.trial-undo-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--gap-3xs);
  padding: var(--gap-3xs) var(--gap-xs);
  font-size: var(--text-xs);
  font-weight: 500;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all var(--transition-fast, 0.15s ease);
}

.trial-keep-btn {
  background: var(--accent-primary);
  color: var(--bg-root, var(--surface));
  border: 1px solid transparent;
}

.trial-keep-btn:hover {
  filter: brightness(1.1);
}

.trial-undo-btn {
  background: var(--bg-secondary);
  color: var(--text-secondary);
  border: 1px solid var(--line);
}

.trial-undo-btn:hover {
  background: var(--surface);
  color: var(--color-error);
  border-color: var(--color-error);
}

@media (max-width: 600px) {
  .trial-banner-item {
    flex-direction: column;
    align-items: stretch;
    gap: var(--gap-xs);
  }

  .trial-actions {
    justify-content: flex-end;
  }
}
</style>
