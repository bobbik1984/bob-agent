<script>
const FEATURED_PROVIDER_IDS = ['deepseek', 'gemini', 'openai', 'anthropic', 'openrouter'];

export const DEFAULT_PROVIDER_PRESETS = [
  {
    id: 'deepseek',
    name: 'DeepSeek',
    clerkModel: 'deepseek-v4.1-flash-1017',
    makerModel: 'deepseek-v4-pro',
    baseUrl: 'https://api.deepseek.com/v1',
    featured: true,
  },
  {
    id: 'gemini',
    name: 'Google Gemini',
    clerkModel: 'models/gemini-2.5-flash',
    makerModel: 'models/gemini-2.5-pro',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta',
    featured: true,
  },
  {
    id: 'openai',
    name: 'OpenAI',
    clerkModel: 'gpt-4o-mini',
    makerModel: 'gpt-4o',
    baseUrl: 'https://api.openai.com/v1',
    featured: true,
  },
  {
    id: 'anthropic',
    name: 'Anthropic',
    clerkModel: 'claude-haiku-4-5',
    makerModel: 'claude-3-7-sonnet-20250219',
    baseUrl: 'https://api.anthropic.com/v1',
    featured: true,
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    clerkModel: 'deepseek/deepseek-v4.1-flash',
    makerModel: 'anthropic/claude-fable-5.1',
    baseUrl: 'https://openrouter.ai/api/v1',
    featured: true,
  },
  {
    id: 'qwen',
    name: 'Qwen (Bailian)',
    clerkModel: 'qwen3.8-flash',
    makerModel: 'qwen3.8-max',
    baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    featured: false,
  },
  {
    id: 'zhipu',
    name: 'Zhipu GLM',
    clerkModel: 'glm-5.3-flash',
    makerModel: 'glm-5.3',
    baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
    featured: false,
  },
  {
    id: 'kimi',
    name: 'Moonshot Kimi',
    clerkModel: 'kimi-k2.6',
    makerModel: 'kimi-k3.1',
    baseUrl: 'https://api.moonshot.cn/v1',
    featured: false,
  },
  {
    id: 'minimax',
    name: 'MiniMax',
    clerkModel: 'MiniMax-M2.7',
    makerModel: 'MiniMax-M3',
    baseUrl: 'https://api.minimax.chat/v1',
    featured: false,
  },
  {
    id: 'doubao',
    name: 'Doubao (Ark)',
    clerkModel: 'doubao-seed-1-6-flash-250828',
    makerModel: 'doubao-seed-2-0-pro-260215',
    baseUrl: 'https://ark.cn-beijing.volces.com/api/v3',
    featured: false,
  },
];

export function normalizeProviderPresets(rawPresets = []) {
  const byId = new Map();
  for (const def of DEFAULT_PROVIDER_PRESETS) {
    byId.set(def.id, { ...def });
  }
  if (Array.isArray(rawPresets)) {
    for (const item of rawPresets) {
      if (!item || typeof item !== 'object') continue;
      const id = String(item.id || item.providerId || item.provider_id || '').trim().toLowerCase();
      if (!id) continue;
      const resolvedName =
        item.displayName || item.display_name || item.name || item.label || id;
      const existing = byId.get(id) || {
        id,
        name: resolvedName,
        clerkModel: '',
        makerModel: '',
        baseUrl: '',
        featured: FEATURED_PROVIDER_IDS.includes(id),
      };
      byId.set(id, {
        ...existing,
        id,
        name:
          item.displayName ||
          item.display_name ||
          item.name ||
          item.label ||
          existing.name,
        clerkModel:
          item.clerkModel ||
          item.clerk_model ||
          item.fastModel ||
          item.fast_model ||
          existing.clerkModel,
        makerModel:
          item.makerModel ||
          item.maker_model ||
          item.workerModel ||
          item.worker_model ||
          item.deepModel ||
          existing.makerModel,
        baseUrl: item.baseUrl || item.base_url || item.endpoint || existing.baseUrl,
        featured:
          item.featured !== undefined
            ? Boolean(item.featured)
            : item.recommended !== undefined
              ? Boolean(item.recommended)
              : FEATURED_PROVIDER_IDS.includes(id),
      });
    }
  }
  const merged = Array.from(byId.values());
  return merged.sort((a, b) => {
    const aIdx = FEATURED_PROVIDER_IDS.indexOf(a.id);
    const bIdx = FEATURED_PROVIDER_IDS.indexOf(b.id);
    if (aIdx !== -1 && bIdx !== -1) return aIdx - bIdx;
    if (aIdx !== -1) return -1;
    if (bIdx !== -1) return 1;
    return 0;
  });
}

export function normalizePairingCode(raw) {
  return String(raw ?? '')
    .replace(/[\s-]+/g, '')
    .replace(/[^0-9A-Za-z]/g, '')
    .slice(0, 6);
}

export function validateStep1Claim({
  pairingCode,
  password,
  confirmPassword,
  pairingCodeRequired = true,
} = {}) {
  const code = normalizePairingCode(pairingCode);
  if (pairingCodeRequired && code.length !== 6) {
    return { valid: false, errorKey: 'onboardingErrPairingFormat' };
  }
  const pwd = String(password ?? '');
  if (pwd.length < 12) {
    return { valid: false, errorKey: 'onboardingErrPasswordShort' };
  }
  if (pwd !== String(confirmPassword ?? '')) {
    return { valid: false, errorKey: 'onboardingErrPasswordMismatch' };
  }
  return { valid: true, errorKey: null };
}

export function resolveOnboardingClaimErrorKey(err) {
  const code = String(err?.code || err?.message || '');
  if (code.includes('PAIRING_CODE_INVALID')) return 'onboardingErrPairingInvalid';
  if (code.includes('PAIRING_CODE_EXPIRED')) return 'onboardingErrPairingExpired';
  if (code.includes('PAIRING_RATE_LIMITED')) return 'onboardingErrRateLimited';
  if (code.includes('ONBOARDING_ALREADY_CLAIMED')) return 'onboardingErrAlreadyClaimed';
  if (code.includes('PASSWORD_TOO_SHORT')) return 'onboardingErrPasswordShort';
  return 'onboardingErrGeneric';
}

export function resolveAutoPairedModels(presets, providerId) {
  const list = normalizeProviderPresets(presets);
  const found = list.find(p => p.id === providerId) || list[0];
  return {
    providerId: found.id,
    clerkModel: found.clerkModel,
    makerModel: found.makerModel,
    baseUrl: found.baseUrl,
  };
}
</script>

<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import JarvisOrb from './JarvisOrb.vue';
import { getProviderLogo } from '../layout-mode.mjs';
import {
  claimOnboardingNode,
  configureOnboardingProvider,
  completeOnboardingPersona,
  getBobNodeSessionToken,
} from '../http-bridge.mjs';

const props = defineProps({
  initialStatus: {
    type: Object,
    default: () => ({ needsOnboarding: true, presets: [] }),
  },
  theme: {
    type: String,
    default: 'system',
  },
});

const emit = defineEmits(['completed', 'update-theme']);
const { t, locale } = useI18n();

const determineInitialStep = status => {
  if (status?.passwordConfigured && getBobNodeSessionToken()) {
    if (status?.providerConfigured) return 3;
    return 2;
  }
  return 1;
};

const currentStep = ref(determineInitialStep(props.initialStatus));
const busy = ref(false);

// Step 1 state
const codeDigits = ref(['', '', '', '', '', '']);
const slotRefs = ref([]);
const adminPassword = ref('');
const confirmPassword = ref('');
const showAdminPassword = ref(false);
const step1ErrorKey = ref('');
const step1RawError = ref('');
const cooldownSec = ref(Math.max(0, Number(props.initialStatus?.lockedRetryAfterSec || 0)));
let cooldownTimer = null;

// Step 2 state
const presets = computed(() => normalizeProviderPresets(props.initialStatus?.presets || []));
const featuredPresets = computed(() => presets.value.filter(p => p.featured));
const additionalPresets = computed(() => presets.value.filter(p => !p.featured));
const showMoreProviders = ref(false);
const selectedProviderId = ref('deepseek');
const clerkModel = ref('');
const makerModel = ref('');
const baseUrl = ref('');
const apiKey = ref('');
const showApiKey = ref(false);
const connectivityVerified = ref(false);
const providerFailedOnce = ref(false);
const step2ErrorText = ref('');

// Step 3 state
const assistantName = ref('');
const step3ErrorText = ref('');

function applyProviderPreset(providerId) {
  const paired = resolveAutoPairedModels(presets.value, providerId);
  selectedProviderId.value = paired.providerId;
  clerkModel.value = paired.clerkModel;
  makerModel.value = paired.makerModel;
  baseUrl.value = paired.baseUrl;
  connectivityVerified.value = false;
  providerFailedOnce.value = false;
  step2ErrorText.value = '';
}

watch(
  presets,
  list => {
    if (list.length > 0 && !clerkModel.value) {
      applyProviderPreset(selectedProviderId.value || list[0].id);
    }
  },
  { immediate: true },
);

const pairingCodeRequired = computed(() => props.initialStatus?.pairingCodeRequired !== false);
const combinedPairingCode = computed(() => normalizePairingCode(codeDigits.value.join('')));

function startCooldown(seconds) {
  const secs = Math.max(0, Math.ceil(Number(seconds || 0)));
  cooldownSec.value = secs;
  if (cooldownTimer) {
    clearInterval(cooldownTimer);
    cooldownTimer = null;
  }
  if (secs <= 0 || typeof setInterval !== 'function') return;
  cooldownTimer = setInterval(() => {
    if (cooldownSec.value > 1) {
      cooldownSec.value -= 1;
    } else {
      cooldownSec.value = 0;
      clearInterval(cooldownTimer);
      cooldownTimer = null;
    }
  }, 1000);
}

onMounted(() => {
  if (cooldownSec.value > 0) {
    startCooldown(cooldownSec.value);
  }
});

onUnmounted(() => {
  if (cooldownTimer) {
    clearInterval(cooldownTimer);
    cooldownTimer = null;
  }
});

function setSlotRef(el, idx) {
  if (el) {
    slotRefs.value[idx] = el;
  }
}

function handleCodeInput(idx, event) {
  const raw = String(event?.target?.value ?? '');
  const clean = normalizePairingCode(raw);
  step1ErrorKey.value = '';
  step1RawError.value = '';

  if (clean.length > 1) {
    fillCodeFromString(clean, idx);
    return;
  }
  codeDigits.value[idx] = clean;
  if (clean && idx < 5) {
    slotRefs.value[idx + 1]?.focus?.();
  }
}

function handleCodeKeydown(idx, event) {
  if (event.key === 'Backspace' && !codeDigits.value[idx] && idx > 0) {
    codeDigits.value[idx - 1] = '';
    slotRefs.value[idx - 1]?.focus?.();
  } else if (event.key === 'ArrowLeft' && idx > 0) {
    event.preventDefault();
    slotRefs.value[idx - 1]?.focus?.();
  } else if (event.key === 'ArrowRight' && idx < 5) {
    event.preventDefault();
    slotRefs.value[idx + 1]?.focus?.();
  }
}

function fillCodeFromString(raw, startIndex = 0) {
  const clean = normalizePairingCode(raw);
  if (!clean) return;
  const next = [...codeDigits.value];
  let pos = startIndex;
  for (let i = 0; i < clean.length && pos < 6; i += 1, pos += 1) {
    next[pos] = clean[i];
  }
  codeDigits.value = next;
  const focusIdx = Math.min(5, Math.max(0, pos - 1));
  slotRefs.value[focusIdx]?.focus?.();
}

function handleCodePaste(idx, event) {
  const pasted = event?.clipboardData?.getData?.('text') || '';
  if (!pasted) return;
  event.preventDefault();
  step1ErrorKey.value = '';
  fillCodeFromString(pasted, idx === 0 || !codeDigits.value[0] ? 0 : idx);
}

function toggleLocale() {
  locale.value = locale.value === 'zh-CN' ? 'en-US' : 'zh-CN';
}

function cycleTheme() {
  const order = ['system', 'light', 'dark'];
  const idx = order.indexOf(props.theme || 'system');
  const next = order[(idx + 1) % order.length];
  emit('update-theme', next);
}

const themeLabel = computed(() => {
  if (props.theme === 'light') return t('onboardingThemeLight');
  if (props.theme === 'dark') return t('onboardingThemeDark');
  return t('onboardingThemeSystem');
});

async function submitStep1() {
  if (busy.value || cooldownSec.value > 0) return;
  step1ErrorKey.value = '';
  step1RawError.value = '';

  const check = validateStep1Claim({
    pairingCode: combinedPairingCode.value,
    password: adminPassword.value,
    confirmPassword: confirmPassword.value,
    pairingCodeRequired: pairingCodeRequired.value,
  });
  if (!check.valid) {
    step1ErrorKey.value = check.errorKey;
    return;
  }

  busy.value = true;
  try {
    await claimOnboardingNode({
      pairingCode: combinedPairingCode.value,
      password: adminPassword.value,
      clientName: 'Bob Web Onboarding',
    });
    currentStep.value = 2;
  } catch (err) {
    const key = resolveOnboardingClaimErrorKey(err);
    step1ErrorKey.value = key;
    if (err?.retryAfterSec) {
      startCooldown(err.retryAfterSec);
    }
    if (key === 'onboardingErrGeneric' && err?.detail) {
      step1RawError.value = err.detail;
    }
  } finally {
    busy.value = false;
  }
}

async function submitStep2(skipConnectivityTest = false) {
  if (busy.value) return;
  step2ErrorText.value = '';
  connectivityVerified.value = false;

  if (!apiKey.value.trim()) {
    step2ErrorText.value = t('onboardingErrApiKeyRequired');
    return;
  }

  busy.value = true;
  try {
    await configureOnboardingProvider({
      providerId: selectedProviderId.value,
      apiKey: apiKey.value.trim(),
      baseUrl: baseUrl.value.trim(),
      clerkModel: clerkModel.value.trim(),
      makerModel: makerModel.value.trim(),
      skipConnectivityTest: Boolean(skipConnectivityTest),
    });
    connectivityVerified.value = !skipConnectivityTest;
    providerFailedOnce.value = false;
    currentStep.value = 3;
  } catch (err) {
    providerFailedOnce.value = true;
    const reason = err?.detail || err?.code || String(err?.message || '');
    step2ErrorText.value = t('onboardingErrProviderProbe', {
      reason: reason || 'CONNECTION_FAILED',
    });
  } finally {
    busy.value = false;
  }
}

async function handleFinishPersona(skip = false) {
  if (busy.value) return;
  step3ErrorText.value = '';
  busy.value = true;
  try {
    const chosenName = skip ? '' : assistantName.value.trim();
    await completeOnboardingPersona({ assistantName: chosenName });
    emit('completed', { assistantName: chosenName });
  } catch (err) {
    step3ErrorText.value = err?.detail || t('onboardingErrGeneric');
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="onboarding-viewport" role="region" :aria-label="t('onboardingBrandSubtitle')">
    <div class="onboarding-card">
      <!-- Top Brand & Utility Bar -->
      <header class="onboarding-topbar">
        <div class="onboarding-brand">
          <JarvisOrb
            size="compact"
            :mood="busy ? 'thinking' : 'idle'"
            :interactive="false"
          />
          <div class="onboarding-brand-copy">
            <strong class="onboarding-brand-title">Bob</strong>
            <span class="onboarding-brand-sub">{{ t('onboardingBrandSubtitle') }}</span>
          </div>
        </div>

        <div class="onboarding-top-actions">
          <button
            type="button"
            class="onboarding-util-btn"
            :aria-label="t('onboardingLangToggle')"
            :title="t('onboardingLangToggle')"
            @click="toggleLocale"
          >
            <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="12" cy="12" r="10" />
              <path d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
            </svg>
            <span>{{ locale === 'zh-CN' ? 'EN' : '中文' }}</span>
          </button>

          <button
            type="button"
            class="onboarding-util-btn"
            :aria-label="themeLabel"
            :title="themeLabel"
            @click="cycleTheme"
          >
            <svg v-if="theme === 'light'" class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="12" cy="12" r="5" />
              <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
            </svg>
            <svg v-else-if="theme === 'dark'" class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
            </svg>
            <svg v-else class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
              <rect x="2" y="3" width="20" height="14" rx="2" />
              <path d="M8 21h8M12 17v4" />
            </svg>
            <span>{{ themeLabel }}</span>
          </button>
        </div>
      </header>

      <!-- 3-Step Progress Stepper -->
      <nav class="onboarding-stepper-nav" :aria-label="t('onboardingStepIndicator', { current: currentStep, total: 3 })">
        <ol class="onboarding-stepper">
          <li
            v-for="(labelKey, idx) in ['onboardingStep1Label', 'onboardingStep2Label', 'onboardingStep3Label']"
            :key="labelKey"
            class="stepper-item"
            :class="{
              active: currentStep === idx + 1,
              completed: currentStep > idx + 1,
            }"
          >
            <span class="stepper-circle">
              <svg
                v-if="currentStep > idx + 1"
                class="check-svg"
                viewBox="0 0 24 24"
                aria-hidden="true"
              >
                <polyline points="20 6 9 17 4 12" />
              </svg>
              <template v-else>{{ idx + 1 }}</template>
            </span>
            <span class="stepper-label">{{ t(labelKey) }}</span>
          </li>
        </ol>
      </nav>

      <!-- Step 1: Claim & Protect Node -->
      <form
        v-if="currentStep === 1"
        class="onboarding-step-body"
        @submit.prevent="submitStep1"
      >
        <div class="step-heading">
          <h1>{{ t('onboardingStep1Title') }}</h1>
          <p>{{ t('onboardingStep1Desc') }}</p>
        </div>

        <div v-if="pairingCodeRequired" class="field-group">
          <label class="field-label" for="pairing-slot-0">{{ t('onboardingPairingCodeLabel') }}</label>
          <div class="pairing-code-slots" role="group" :aria-label="t('onboardingPairingCodeLabel')">
            <input
              v-for="(digit, idx) in codeDigits"
              :id="`pairing-slot-${idx}`"
              :key="idx"
              :ref="el => setSlotRef(el, idx)"
              type="text"
              inputmode="numeric"
              maxlength="6"
              autocomplete="one-time-code"
              class="pairing-slot-input"
              :value="digit"
              :disabled="busy || cooldownSec > 0"
              @input="handleCodeInput(idx, $event)"
              @keydown="handleCodeKeydown(idx, $event)"
              @paste="handleCodePaste(idx, $event)"
            />
          </div>
          <p class="field-hint">
            <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
              <polyline points="4 17 10 11 4 5" />
              <line x1="12" y1="19" x2="20" y2="19" />
            </svg>
            <span>{{ t('onboardingPairingCodeHint') }}</span>
          </p>
        </div>

        <div class="field-group">
          <label class="field-label" for="onboarding-admin-password">
            {{ t('onboardingAdminPasswordLabel') }}
          </label>
          <div class="password-input-wrap">
            <input
              id="onboarding-admin-password"
              v-model="adminPassword"
              :type="showAdminPassword ? 'text' : 'password'"
              class="onboarding-text-input"
              autocomplete="new-password"
              :placeholder="t('onboardingAdminPasswordPlaceholder')"
              :disabled="busy"
              @input="step1ErrorKey = ''"
            />
            <button
              type="button"
              class="password-eye-btn"
              :aria-label="t(showAdminPassword ? 'onboardingHidePassword' : 'onboardingShowPassword')"
              @click="showAdminPassword = !showAdminPassword"
            >
              <svg v-if="showAdminPassword" class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
                <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24" />
                <line x1="1" y1="1" x2="23" y2="23" />
              </svg>
              <svg v-else class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
                <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                <circle cx="12" cy="12" r="3" />
              </svg>
            </button>
          </div>
        </div>

        <div class="field-group">
          <label class="field-label" for="onboarding-confirm-password">
            {{ t('onboardingConfirmPasswordLabel') }}
          </label>
          <input
            id="onboarding-confirm-password"
            v-model="confirmPassword"
            :type="showAdminPassword ? 'text' : 'password'"
            class="onboarding-text-input"
            autocomplete="new-password"
            :placeholder="t('onboardingConfirmPasswordPlaceholder')"
            :disabled="busy"
            @input="step1ErrorKey = ''"
          />
        </div>

        <div
          v-if="cooldownSec > 0 || step1ErrorKey"
          class="onboarding-alert alert-error"
          role="alert"
        >
          <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
          <span>
            {{
              cooldownSec > 0
                ? t('onboardingErrRateLimited', { seconds: cooldownSec })
                : step1RawError || t(step1ErrorKey)
            }}
          </span>
        </div>

        <footer class="onboarding-footer-actions">
          <button
            type="submit"
            class="onboarding-primary-cta"
            :disabled="busy || cooldownSec > 0"
          >
            <svg v-if="busy" class="inline-svg onboarding-spin" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M21 12a9 9 0 1 1-6.219-8.56" />
            </svg>
            <span>{{ t(busy ? 'onboardingClaimingBtn' : 'onboardingClaimBtn') }}</span>
          </button>
        </footer>
      </form>

      <!-- Step 2: Choose Provider & Auto-Pair Clerk + Maker -->
      <form
        v-else-if="currentStep === 2"
        class="onboarding-step-body"
        @submit.prevent="submitStep2(false)"
      >
        <div class="step-heading">
          <h1>{{ t('onboardingStep2Title') }}</h1>
          <p>{{ t('onboardingStep2Desc') }}</p>
        </div>

        <div class="provider-cards-grid" role="radiogroup" :aria-label="t('onboardingStep2Title')">
          <button
            v-for="prov in featuredPresets"
            :key="prov.id"
            type="button"
            role="radio"
            :aria-checked="selectedProviderId === prov.id"
            class="provider-card-btn"
            :class="{ selected: selectedProviderId === prov.id }"
            @click="applyProviderPreset(prov.id)"
          >
            <img :src="getProviderLogo(prov.id)" alt="" class="provider-card-logo" />
            <span class="provider-card-name">{{ prov.name }}</span>
            <svg
              v-if="selectedProviderId === prov.id"
              class="check-svg provider-check"
              viewBox="0 0 24 24"
              aria-hidden="true"
            >
              <polyline points="20 6 9 17 4 12" />
            </svg>
          </button>
        </div>

        <div v-if="additionalPresets.length > 0" class="more-providers-block">
          <button
            type="button"
            class="more-providers-toggle"
            :aria-expanded="showMoreProviders"
            @click="showMoreProviders = !showMoreProviders"
          >
            <span>{{ t(showMoreProviders ? 'onboardingLessProviders' : 'onboardingMoreProviders') }}</span>
            <svg class="chevron-svg" viewBox="0 0 24 24" aria-hidden="true">
              <path :d="showMoreProviders ? 'm18 15-6-6-6 6' : 'm6 9 6 6 6-6'" />
            </svg>
          </button>

          <div v-if="showMoreProviders" class="provider-cards-grid additional-grid">
            <button
              v-for="prov in additionalPresets"
              :key="prov.id"
              type="button"
              role="radio"
              :aria-checked="selectedProviderId === prov.id"
              class="provider-card-btn"
              :class="{ selected: selectedProviderId === prov.id }"
              @click="applyProviderPreset(prov.id)"
            >
              <img :src="getProviderLogo(prov.id)" alt="" class="provider-card-logo" />
              <span class="provider-card-name">{{ prov.name }}</span>
              <svg
                v-if="selectedProviderId === prov.id"
                class="check-svg provider-check"
                viewBox="0 0 24 24"
                aria-hidden="true"
              >
                <polyline points="20 6 9 17 4 12" />
              </svg>
            </button>
          </div>
        </div>

        <!-- Auto-Paired Clerk + Maker Summary Strip -->
        <div class="role-pair-strip">
          <div class="role-pair-item">
            <span class="role-pair-tag">{{ t('onboardingRoleClerkBadge') }}</span>
            <code class="role-pair-model">{{ clerkModel }}</code>
          </div>
          <div class="role-pair-item">
            <span class="role-pair-tag maker-tag">{{ t('onboardingRoleMakerBadge') }}</span>
            <code class="role-pair-model">{{ makerModel }}</code>
          </div>
        </div>

        <div class="field-group">
          <label class="field-label" for="onboarding-api-key">
            {{ t('onboardingApiKeyLabel') }}
          </label>
          <div class="password-input-wrap">
            <input
              id="onboarding-api-key"
              v-model="apiKey"
              :type="showApiKey ? 'text' : 'password'"
              class="onboarding-text-input"
              autocomplete="off"
              :placeholder="t('onboardingApiKeyPlaceholder')"
              :disabled="busy"
              @input="step2ErrorText = ''"
            />
            <button
              type="button"
              class="password-eye-btn"
              :aria-label="t(showApiKey ? 'onboardingHidePassword' : 'onboardingShowPassword')"
              @click="showApiKey = !showApiKey"
            >
              <svg v-if="showApiKey" class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
                <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24" />
                <line x1="1" y1="1" x2="23" y2="23" />
              </svg>
              <svg v-else class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
                <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                <circle cx="12" cy="12" r="3" />
              </svg>
            </button>
          </div>
        </div>

        <details class="onboarding-advanced-drawer">
          <summary class="advanced-drawer-summary">
            <span>{{ t('onboardingAdvancedModelsSummary') }}</span>
            <svg class="chevron-svg" viewBox="0 0 24 24" aria-hidden="true">
              <path d="m6 9 6 6 6-6" />
            </svg>
          </summary>
          <div class="advanced-drawer-fields">
            <div class="field-group">
              <label class="field-label" for="onboarding-clerk-model">
                {{ t('onboardingClerkModelLabel') }}
              </label>
              <input
                id="onboarding-clerk-model"
                v-model="clerkModel"
                type="text"
                class="onboarding-text-input"
                :disabled="busy"
              />
            </div>
            <div class="field-group">
              <label class="field-label" for="onboarding-maker-model">
                {{ t('onboardingMakerModelLabel') }}
              </label>
              <input
                id="onboarding-maker-model"
                v-model="makerModel"
                type="text"
                class="onboarding-text-input"
                :disabled="busy"
              />
            </div>
            <div class="field-group">
              <label class="field-label" for="onboarding-base-url">
                {{ t('onboardingBaseUrlLabel') }}
              </label>
              <input
                id="onboarding-base-url"
                v-model="baseUrl"
                type="url"
                class="onboarding-text-input"
                :placeholder="t('onboardingBaseUrlPlaceholder')"
                :disabled="busy"
              />
            </div>
          </div>
        </details>

        <div v-if="connectivityVerified" class="onboarding-alert alert-success" role="status">
          <svg class="check-svg" viewBox="0 0 24 24" aria-hidden="true">
            <polyline points="20 6 9 17 4 12" />
          </svg>
          <span>{{ t('onboardingConnectedBadge') }}</span>
        </div>

        <div v-if="step2ErrorText" class="onboarding-alert alert-error" role="alert">
          <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
          <span>{{ step2ErrorText }}</span>
        </div>

        <footer class="onboarding-footer-actions">
          <button
            v-if="providerFailedOnce"
            type="button"
            class="onboarding-secondary-cta"
            :disabled="busy"
            @click="submitStep2(true)"
          >
            {{ t('onboardingSaveSkipTestBtn') }}
          </button>
          <button
            type="submit"
            class="onboarding-primary-cta"
            :disabled="busy"
          >
            <svg v-if="busy" class="inline-svg onboarding-spin" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M21 12a9 9 0 1 1-6.219-8.56" />
            </svg>
            <span>{{ t(busy ? 'onboardingTestingBtn' : 'onboardingTestConnectBtn') }}</span>
          </button>
        </footer>
      </form>

      <!-- Step 3: Personalize or Start Immediately -->
      <form
        v-else
        class="onboarding-step-body step-persona"
        @submit.prevent="handleFinishPersona(false)"
      >
        <div class="persona-orb-stage">
          <JarvisOrb
            size="hero"
            :mood="busy ? 'speaking' : 'idle'"
            :interactive="false"
          />
        </div>

        <div class="step-heading centered">
          <h1>{{ t('onboardingStep3Title') }}</h1>
          <p>{{ t('onboardingStep3Desc') }}</p>
        </div>

        <div class="field-group">
          <label class="field-label" for="onboarding-assistant-name">
            {{ t('onboardingAssistantNameLabel') }}
          </label>
          <input
            id="onboarding-assistant-name"
            v-model="assistantName"
            type="text"
            class="onboarding-text-input"
            maxlength="40"
            :placeholder="t('onboardingAssistantNamePlaceholder')"
            :disabled="busy"
          />
        </div>

        <div v-if="step3ErrorText" class="onboarding-alert alert-error" role="alert">
          <svg class="inline-svg" viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
          <span>{{ step3ErrorText }}</span>
        </div>

        <footer class="onboarding-footer-actions dual-actions">
          <button
            type="button"
            class="onboarding-secondary-cta"
            :disabled="busy"
            @click="handleFinishPersona(true)"
          >
            {{ t('onboardingSkipStartBtn') }}
          </button>
          <button
            type="submit"
            class="onboarding-primary-cta"
            :disabled="busy"
          >
            <svg v-if="busy" class="inline-svg onboarding-spin" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M21 12a9 9 0 1 1-6.219-8.56" />
            </svg>
            <span>{{ t(busy ? 'onboardingSavingPersonaBtn' : 'onboardingEnterBobBtn') }}</span>
          </button>
        </footer>
      </form>
    </div>
  </div>
</template>

<style scoped>
.onboarding-viewport {
  position: fixed;
  inset: 0;
  z-index: 90;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-start;
  overflow-y: auto;
  padding: max(1rem, env(safe-area-inset-top, 0px)) 1rem max(1rem, env(safe-area-inset-bottom, 0px));
  background: var(--bg-root);
  color: var(--text-primary);
}

.onboarding-card {
  width: 100%;
  max-width: 38rem;
  margin: auto;
  display: flex;
  flex-direction: column;
  gap: var(--gap-lg);
  padding: 1.35rem 1.5rem;
  background: var(--bg-primary);
  border: 1px solid var(--line);
  border-radius: var(--radius-modal);
  box-shadow: var(--shadow-lg);
}

.onboarding-topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-sm);
}

.onboarding-brand {
  display: flex;
  align-items: center;
  gap: var(--gap-sm);
  min-width: 0;
}

.onboarding-brand-copy {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.onboarding-brand-title {
  font-size: var(--text-md);
  font-weight: 700;
  line-height: 1.2;
  color: var(--text-primary);
}

.onboarding-brand-sub {
  font-size: var(--text-2xs);
  color: var(--text-secondary);
}

.onboarding-top-actions {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
  flex-shrink: 0;
}

.onboarding-util-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--gap-3xs);
  min-height: 1.85rem;
  padding: 0.2rem 0.55rem;
  font-size: var(--text-2xs);
  border-radius: var(--radius-pill);
  border: 1px solid var(--line);
  background: var(--bg-secondary);
  color: var(--text-secondary);
}

.onboarding-util-btn:hover {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.onboarding-stepper-nav {
  border-bottom: 1px solid var(--line);
  padding-bottom: var(--gap-md);
}

.onboarding-stepper {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: var(--gap-xs);
}

.stepper-item {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
  font-size: var(--text-xs);
  color: var(--text-secondary);
}

.stepper-circle {
  width: 1.45rem;
  height: 1.45rem;
  border-radius: var(--radius-pill);
  border: 1px solid var(--line);
  background: var(--bg-secondary);
  color: var(--text-secondary);
  font-size: var(--text-2xs);
  font-weight: 600;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.stepper-item.active {
  color: var(--text-primary);
  font-weight: 600;
}

.stepper-item.active .stepper-circle {
  background: var(--accent-primary);
  border-color: var(--accent-primary);
  color: var(--text-on-accent);
}

.stepper-item.completed {
  color: var(--accent-primary);
}

.stepper-item.completed .stepper-circle {
  background: color-mix(in srgb, var(--accent-primary) 12%, transparent);
  border-color: var(--accent-primary);
  color: var(--accent-primary);
}

.stepper-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.onboarding-step-body {
  display: flex;
  flex-direction: column;
  gap: var(--gap-md);
}

.step-heading h1 {
  margin: 0 0 0.25rem;
  font-size: var(--text-lg);
  font-weight: 600;
  color: var(--text-primary);
}

.step-heading p {
  margin: 0;
  font-size: var(--text-sm);
  color: var(--text-secondary);
  line-height: 1.5;
}

.step-heading.centered {
  text-align: center;
}

.field-group {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2xs);
}

.field-label {
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--text-primary);
}

.field-hint {
  margin: 0.15rem 0 0;
  display: flex;
  align-items: flex-start;
  gap: var(--gap-2xs);
  font-size: var(--text-2xs);
  color: var(--text-secondary);
  line-height: 1.45;
}

.pairing-code-slots {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: var(--gap-xs);
}

.pairing-slot-input {
  width: 100%;
  min-height: 2.85rem;
  padding: 0;
  text-align: center;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: var(--text-lg);
  font-weight: 700;
  letter-spacing: 0.04em;
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
  background: var(--bg-secondary);
  color: var(--text-primary);
}

.pairing-slot-input:focus {
  background: var(--bg-primary);
  border-color: var(--accent-primary);
}

.password-input-wrap {
  position: relative;
  display: flex;
  align-items: center;
}

.onboarding-text-input {
  width: 100%;
  min-height: 2.5rem;
  padding: 0.45rem 0.75rem;
  font-size: var(--text-sm);
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
  background: var(--bg-primary);
  color: var(--text-primary);
}

.password-input-wrap .onboarding-text-input {
  padding-right: 2.5rem;
}

.password-eye-btn {
  position: absolute;
  right: 0.3rem;
  min-height: 1.9rem;
  padding: 0.25rem;
  border: 0;
  background: transparent;
  color: var(--text-secondary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.password-eye-btn:hover {
  color: var(--text-primary);
}

.provider-cards-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(9.5rem, 1fr));
  gap: var(--gap-sm);
}

.provider-card-btn {
  position: relative;
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
  min-height: 2.75rem;
  padding: 0.5rem 0.65rem;
  border: 1px solid var(--line);
  border-radius: var(--radius-card);
  background: var(--bg-secondary);
  color: var(--text-primary);
  text-align: start;
  transition: border-color 0.15s ease, background 0.15s ease;
}

.provider-card-btn:hover {
  background: var(--bg-tertiary);
}

.provider-card-btn.selected {
  border-color: var(--accent-primary);
  background: color-mix(in srgb, var(--accent-primary) 12%, transparent);
}

.provider-card-logo {
  width: 1.2rem;
  height: 1.2rem;
  object-fit: contain;
  border-radius: var(--radius-xs);
  flex-shrink: 0;
}

.provider-card-name {
  font-size: var(--text-xs);
  font-weight: 600;
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.provider-check {
  color: var(--accent-primary);
  flex-shrink: 0;
}

.more-providers-block {
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
}

.more-providers-toggle {
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: var(--gap-2xs);
  min-height: 1.8rem;
  padding: 0.2rem 0.6rem;
  font-size: var(--text-xs);
  border-radius: var(--radius-pill);
  border: 1px solid var(--line);
  background: var(--bg-secondary);
  color: var(--text-secondary);
}

.role-pair-strip {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--gap-xs);
  padding: 0.55rem 0.7rem;
  border-radius: var(--radius-card);
  border: 1px solid var(--line);
  background: var(--bg-secondary);
}

.role-pair-item {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
  min-width: 0;
}

.role-pair-tag {
  font-size: var(--text-2xs);
  font-weight: 600;
  color: var(--accent-primary);
}

.role-pair-tag.maker-tag {
  color: var(--text-secondary);
}

.role-pair-model {
  font-size: var(--text-xs);
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.onboarding-advanced-drawer {
  border: 1px dashed var(--line);
  border-radius: var(--radius-card);
  background: var(--bg-secondary);
  padding: 0.35rem 0.65rem;
}

.advanced-drawer-summary {
  list-style: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-xs);
  cursor: pointer;
  padding: 0.25rem 0;
  font-size: var(--text-xs);
  font-weight: 500;
  color: var(--text-secondary);
}

.advanced-drawer-summary::-webkit-details-marker {
  display: none;
}

.advanced-drawer-fields {
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
  padding: var(--gap-sm) 0 var(--gap-2xs);
  border-top: 1px solid var(--line);
  margin-top: var(--gap-2xs);
}

.onboarding-alert {
  display: flex;
  align-items: flex-start;
  gap: var(--gap-xs);
  padding: 0.55rem 0.75rem;
  border-radius: var(--radius-md);
  font-size: var(--text-xs);
  line-height: 1.45;
  border: 1px solid var(--line);
}

.onboarding-alert.alert-error {
  background: var(--error-soft);
  border-color: color-mix(in srgb, var(--color-error) 38%, var(--line));
  color: var(--error-text);
}

.onboarding-alert.alert-success {
  background: var(--success-soft);
  border-color: color-mix(in srgb, var(--color-success) 38%, var(--line));
  color: var(--success-text);
}

.persona-orb-stage {
  display: flex;
  justify-content: center;
  padding: var(--gap-xs) 0;
}

.onboarding-footer-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--gap-sm);
  padding-top: var(--gap-xs);
}

.onboarding-primary-cta {
  flex: 1;
  min-height: 2.65rem;
  padding: 0.55rem 1rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--gap-xs);
  border-radius: var(--radius-btn);
  border: 1px solid var(--accent-primary);
  background: var(--accent-primary);
  color: var(--text-on-accent);
  font-size: var(--text-sm);
  font-weight: 600;
}

.onboarding-secondary-cta {
  min-height: 2.65rem;
  padding: 0.55rem 0.9rem;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-btn);
  border: 1px solid var(--line);
  background: var(--bg-secondary);
  color: var(--text-primary);
  font-size: var(--text-xs);
  font-weight: 500;
}

.onboarding-secondary-cta:hover {
  background: var(--bg-tertiary);
}

@keyframes onboarding-rotate {
  to {
    transform: rotate(360deg);
  }
}

.onboarding-spin {
  animation: onboarding-rotate 0.85s linear infinite;
}

@media (prefers-reduced-motion: reduce) {
  .onboarding-spin {
    animation: none;
  }
}

/* Desktop wide 1280px+ centered card presentation */
@media (min-width: 1280px) {
  .onboarding-card {
    max-width: 42rem;
    padding: 1.75rem 2rem;
  }

  .provider-cards-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

/* Mobile 390px single-column touch layout with sticky bottom CTA */
@media (max-width: 640px) {
  .onboarding-viewport {
    padding: env(safe-area-inset-top, 0px) 0 0;
    background: var(--bg-primary);
  }

  .onboarding-card {
    max-width: none;
    min-height: 100%;
    margin: 0;
    border: 0;
    border-radius: 0;
    box-shadow: none;
    padding: 1rem 1rem 0;
  }

  .onboarding-step-body {
    flex: 1;
  }

  .role-pair-strip {
    grid-template-columns: 1fr;
  }

  .onboarding-footer-actions {
    position: sticky;
    bottom: 0;
    margin-top: auto;
    margin-inline: -1rem;
    padding: 0.75rem 1rem calc(0.75rem + env(safe-area-inset-bottom, 0px));
    background: var(--bg-primary);
    border-top: 1px solid var(--line);
    flex-direction: column-reverse;
    align-items: stretch;
  }

  .onboarding-primary-cta,
  .onboarding-secondary-cta {
    width: 100%;
    min-height: 2.75rem;
  }
}

@media (max-width: 390px) {
  .provider-cards-grid {
    grid-template-columns: 1fr;
  }

  .stepper-label {
    font-size: var(--text-2xs);
  }

  .pairing-slot-input {
    min-height: 2.6rem;
    font-size: var(--text-base);
  }
}
</style>
