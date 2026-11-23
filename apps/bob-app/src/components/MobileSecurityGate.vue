<script setup>
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  securityLock: {
    type: Object,
    required: true,
  },
});

const { t } = useI18n();
const autoPrompted = ref(false);

const errorMessage = computed(() => {
  const code = props.securityLock?.errorCode?.value;
  if (!code) return '';
  if (code === 'SECURITY_LOCK_NOT_ENROLLED') return t('securityLockErrNotEnrolled');
  if (code === 'SECURITY_LOCK_CANCELLED') return t('securityLockErrCancelled');
  return t('securityLockErrFailed');
});

function promptMeta() {
  return {
    title: t('securityLockPromptTitle'),
    subtitle: t('securityLockPromptSubtitle'),
  };
}

function enableFirstLaunch() {
  void props.securityLock.chooseFirstLaunch(true, promptMeta());
}

function skipFirstLaunch() {
  void props.securityLock.chooseFirstLaunch(false);
}

function triggerUnlock() {
  void props.securityLock.unlock(promptMeta());
}

watch(
  () => props.securityLock?.needsUnlock?.value,
  needed => {
    if (needed && !autoPrompted.value) {
      autoPrompted.value = true;
      triggerUnlock();
    }
  },
  { immediate: true },
);
</script>

<template>
  <div
    v-if="securityLock?.isBlocked?.value"
    class="mobile-security-gate"
    role="dialog"
    aria-modal="true"
    :aria-label="t(securityLock.needsDecision.value ? 'securityLockSetupTitle' : 'securityLockLockedTitle')"
  >
    <section class="security-gate-card">
      <div class="security-gate-badge" aria-hidden="true">
        <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
          <rect x="4" y="11" width="16" height="10" rx="2.5" />
          <path d="M8 11V7.5a4 4 0 0 1 8 0V11" />
        </svg>
      </div>

      <template v-if="securityLock.needsDecision.value">
        <h2 class="security-gate-title">{{ t('securityLockSetupTitle') }}</h2>
        <p class="security-gate-desc">{{ t('securityLockSetupDesc') }}</p>
        <p v-if="!securityLock.state.value.enrolled" class="security-gate-hint">
          {{ t('securityLockNotEnrolledHint') }}
        </p>
        <div class="security-gate-actions">
          <button
            type="button"
            class="security-gate-btn-primary"
            :disabled="securityLock.busy.value"
            @click="enableFirstLaunch"
          >
            {{ t('securityLockEnableBtn') }}
          </button>
          <button
            type="button"
            class="security-gate-btn-secondary"
            :disabled="securityLock.busy.value"
            @click="skipFirstLaunch"
          >
            {{ t('securityLockSkipBtn') }}
          </button>
        </div>
      </template>

      <template v-else>
        <h2 class="security-gate-title">{{ t('securityLockLockedTitle') }}</h2>
        <p class="security-gate-desc">{{ t('securityLockLockedDesc') }}</p>
        <div class="security-gate-actions">
          <button
            type="button"
            class="security-gate-btn-primary"
            :disabled="securityLock.busy.value"
            @click="triggerUnlock"
          >
            {{ t('securityLockUnlockBtn') }}
          </button>
        </div>
      </template>

      <p v-if="errorMessage" role="alert" class="security-gate-error">
        {{ errorMessage }}
      </p>
    </section>
  </div>
</template>

<style scoped>
.mobile-security-gate {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1.5rem;
  background: var(--bg-root);
}

.security-gate-card {
  width: 100%;
  max-width: 24rem;
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: 0.875rem;
  padding: 1.75rem 1.5rem;
  border-radius: 1rem;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
}

.security-gate-badge {
  width: 3.5rem;
  height: 3.5rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 9999px;
  background: var(--accent-soft);
  color: var(--accent-primary);
}

.security-gate-title {
  margin: 0;
  font-size: 1.125rem;
  font-weight: 600;
  color: var(--text-primary);
}

.security-gate-desc {
  margin: 0;
  font-size: 0.875rem;
  line-height: 1.5;
  color: var(--text-secondary);
}

.security-gate-hint {
  margin: 0;
  padding: 0.625rem 0.75rem;
  border-radius: 0.5rem;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  font-size: 0.8125rem;
  color: var(--text-secondary);
}

.security-gate-actions {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 0.625rem;
  margin-top: 0.5rem;
}

.security-gate-btn-primary {
  width: 100%;
  padding: 0.75rem 1rem;
  border-radius: 0.625rem;
  border: 1px solid var(--accent-primary);
  background: var(--accent-primary);
  color: var(--text-on-accent);
  font-size: 0.9375rem;
  font-weight: 600;
  cursor: pointer;
}

.security-gate-btn-primary:disabled,
.security-gate-btn-secondary:disabled {
  opacity: 0.6;
  cursor: default;
}

.security-gate-btn-secondary {
  width: 100%;
  padding: 0.6875rem 1rem;
  border-radius: 0.625rem;
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-primary);
  font-size: 0.875rem;
  font-weight: 500;
  cursor: pointer;
}

.security-gate-error {
  margin: 0;
  font-size: 0.8125rem;
  color: var(--color-error);
}
</style>
