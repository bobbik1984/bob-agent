<template>
  <div class="onboarding-layout">
    <div class="onboarding-card">
      <div class="wizard-logo">
        <div class="logo-layer" :class="{ visible: step >= 1 }" style="-webkit-mask-image: url(/bob_.svg); mask-image: url(/bob_.svg)"></div>
        <div class="logo-layer" :class="{ visible: step >= 2 }" style="-webkit-mask-image: url(/bob_b.svg); mask-image: url(/bob_b.svg)"></div>
        <div class="logo-layer" :class="{ visible: step >= Math.ceil(totalSteps * 0.75) }" style="-webkit-mask-image: url(/bob_bo.svg); mask-image: url(/bob_bo.svg)"></div>
        <div class="logo-layer" :class="{ visible: isLastStep }" style="-webkit-mask-image: url(/bob_bob.svg); mask-image: url(/bob_bob.svg)"></div>
      </div>

      <div class="wizard-body">
        <!-- Page: Appearance (语言、主题、颜色) -->
        <div v-if="currentStepType === 'appearance'" class="page">
          <div class="theme-options">
            <button :class="['btn-theme', { active: tempConfig.language === 'zh-CN' }]" @click="setLanguage('zh-CN')">
              简体中文
            </button>
            <button :class="['btn-theme', { active: tempConfig.language === 'en-US' }]" @click="setLanguage('en-US')">
              English
            </button>
          </div>
          <div class="theme-options">
            <button :class="['btn-theme', { active: tempConfig.theme === 'dark' }]" @click="setTheme('dark')">
            <Moon :size="20" /> {{ $t('setup.theme_dark') }}
            </button>
            <button :class="['btn-theme', { active: tempConfig.theme === 'light' }]" @click="setTheme('light')">
              <Sun :size="20" /> {{ $t('setup.theme_light') }}
            </button>
          </div>
          <div class="color-options">
            <button
              v-for="color in accentColors"
              :key="color.value"
              class="color-circle"
              :class="{ active: tempConfig.accentColor === color.value }"
              :style="{ backgroundColor: color.value }"
              @click="setAccentColor(color.value)"
              :title="color.name"
            ></button>
          </div>
        </div>

        <!-- Page: Workspace (桌面端专属) -->
        <div v-if="currentStepType === 'workspace'" class="page page-center">
          <div class="workspace-row">
            <div class="workspace-input" :class="{ filled: tempConfig.workspaceDir }" @click="selectWorkspaceDir">
              {{ tempConfig.workspaceDir || $t('setup.workspace_placeholder') }}
            </div>
            <button class="workspace-btn" @click="selectWorkspaceDir">...</button>
          </div>
        </div>

        <!-- Page: LLM -->
        <div v-if="currentStepType === 'llm'" class="page page-center">
          <div class="llm-form">
            <CustomSelect v-model="tempConfig.provider" :options="providerOptions" :placeholder="$t('setup.provider_placeholder')" />
            <input class="input" type="password" v-model="tempConfig.apiKey" placeholder="API Key" />
          </div>
        </div>

        <!-- Page: WeChat (桌面端) -->
        <div v-if="currentStepType === 'wechat'" class="page page-top">
          <div class="wechat-toggle" @click="toggleWechat">
            <img :src="'./wechat.svg'" class="wechat-icon" :class="{ active: enableWechat }" alt="" />
            <label class="switch-label">
              <input type="checkbox" v-model="enableWechat" @change="toggleWechat" @click.stop />
              <span class="slider round"></span>
            </label>
          </div>
          <div v-if="enableWechat" class="qr-area animate-fade-in">
            <div v-if="!qrCodeUrl && !wechatConnected" class="qr-loading">
              <Loader2 class="animate-spin" :size="24" />
            </div>
            <div v-else-if="wechatConnected" class="qr-done">
              <Check :size="32" />
            </div>
            <div v-else>
              <img :src="qrCodeUrl" class="qr-image" alt="" />
            </div>
          </div>
        </div>

        <!-- Page: Scan to Pair (移动端专属占位) -->
        <div v-if="currentStepType === 'pair'" class="page page-center">
          <div class="pair-placeholder">
            <QrCode :size="48" style="opacity: 0.3; margin-bottom: 16px;" />
            <p style="color: var(--text-secondary); text-align: center; line-height: 1.6;">
              {{ $t('setup.pair_coming_soon') || '扫码配对功能即将上线，敬请期待' }}
            </p>
            <p style="color: var(--text-tertiary); font-size: 12px; margin-top: 8px;">
              {{ $t('setup.pair_skip_hint') || '可先跳过此步骤，稍后在设置中配对' }}
            </p>
          </div>
        </div>
      </div>

      <div class="wizard-nav">
        <button class="nav-arrow" v-if="step > 1" @click="step--">
          <ChevronLeft :size="20" />
        </button>
        <div class="nav-spacer"></div>
        <button class="nav-arrow" v-if="!isLastStep" @click="step++">
          <ChevronRight :size="20" />
        </button>
        <button class="nav-arrow nav-launch" v-if="isLastStep" @click="finishOnboarding">
          <Rocket :size="20" />
        </button>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, watch, onMounted, onUnmounted, inject } from 'vue';
import { useI18n } from 'vue-i18n';
import { Moon, Sun, ChevronLeft, ChevronRight, Loader2, Rocket, Check, Smartphone, QrCode } from 'lucide-vue-next';
import CustomSelect from './CustomSelect.vue';
import { ACCENT_COLORS } from '@/constants/theme.js';

const { locale, t: $t } = useI18n();
const isMobile = inject('isMobile', false);

const emit = defineEmits(['complete']);
const step = ref(1); // 1-indexed within wizardSteps
const isTesting = ref(false);
const testResult = ref(null);

// 移动端跳过 workspace 步骤，微信步骤改为扫码配对
const wizardSteps = computed(() => {
  if (isMobile) {
    return ['appearance', 'llm', 'pair']; // 3 步: 外观 → LLM → 扫码配对
  }
  return ['appearance', 'workspace', 'llm', 'wechat']; // 4 步: 外观 → 工作间 → LLM → 微信
});

const currentStepType = computed(() => wizardSteps.value[step.value - 1]);
const totalSteps = computed(() => wizardSteps.value.length);
const isLastStep = computed(() => step.value >= totalSteps.value);

const enableWechat = ref(false);
const qrCodeUrl = ref('');
const wechatConnected = ref(false);
const rawQrCode = ref('');
let pollTimer = null;

const tempConfig = ref({
  language: locale.value,
  theme: 'dark',
  accentColor: '#2776bb',
  workspaceDir: '',
  provider: 'deepseek',
  apiKey: ''
});

// 记录初始快照，用于 finishOnboarding 时只写入用户实际修改过的字段
let initialSnapshot = {};

const accentColors = ACCENT_COLORS;

const providerOptions = [
  { label: 'DeepSeek', value: 'deepseek' },
  { label: 'OpenAI', value: 'openai' },
  { label: '通义千问 (Qwen)', value: 'qwen' },
  { label: '豆包 (Doubao)', value: 'doubao' },
  { label: '智谱 AI (GLM)', value: 'zhipu' },
  { label: 'Kimi (Moonshot)', value: 'kimi' },
  { label: 'MiniMax', value: 'minimax' }
];

onMounted(async () => {
  // 先拍快照：从已有配置（而非 tempConfig）直接拍摄，防止用户在加载期间修改 tempConfig 导致快照被污染
  const snapshotBase = { ...tempConfig.value };
  if (window.appAPI) {
    const saved = await window.appAPI.getAllConfig();
    if (saved) {
      // 用已有配置覆盖 snapshotBase 和 tempConfig
      if (saved.language) { snapshotBase.language = saved.language; tempConfig.value.language = saved.language; locale.value = saved.language; }
      if (saved.theme) { snapshotBase.theme = saved.theme; tempConfig.value.theme = saved.theme; }
      if (saved.accentColor) { snapshotBase.accentColor = saved.accentColor; tempConfig.value.accentColor = saved.accentColor; }
      if (saved.workspaceDir) { snapshotBase.workspaceDir = saved.workspaceDir; tempConfig.value.workspaceDir = saved.workspaceDir; }
      if (saved.provider) { snapshotBase.provider = saved.provider; tempConfig.value.provider = saved.provider; }
      // apiKey 不从配置回填（安全考虑，存在 apiKeys 子对象中）
    }
  }
  // 将主题和色彩应用到 DOM（使用已有配置或默认值）
  document.documentElement.setAttribute('data-theme', tempConfig.value.theme);
  setAccentColor(tempConfig.value.accentColor);
  // 快照来自 saved config 原始值，不受用户在 await 期间的点击影响
  initialSnapshot = JSON.parse(JSON.stringify(snapshotBase));
});

// 离开 LLM 步骤时立即保存 API Key 到 OS Keychain
watch(step, async (newStep, oldStep) => {
  const oldStepType = wizardSteps.value[oldStep - 1];
  if (oldStepType === 'llm' && tempConfig.value.apiKey && window.appAPI?.setApiKey) {
    await window.appAPI.setApiKey(tempConfig.value.provider, tempConfig.value.apiKey);
  }
});

onUnmounted(() => { if (pollTimer) clearTimeout(pollTimer); });

async function toggleWechat() {
  if (enableWechat.value) {
    await loadQrCode();
  } else {
    if (pollTimer) { clearTimeout(pollTimer); pollTimer = null; }
    qrCodeUrl.value = '';
  }
}

async function loadQrCode() {
  if (!window.appAPI) return;
  try {
    const res = await window.appAPI.wechatGetLoginQr();
    if (res && res.qrcode_img_content) {
      qrCodeUrl.value = res.qrcode_img_content;
      rawQrCode.value = res.qrcode;
      pollQrStatus();
    }
  } catch (e) {
    console.error('Failed to get QR code', e);
  }
}

async function pollQrStatus() {
  if (!enableWechat.value || wechatConnected.value) return;
  try {
    const res = await window.appAPI.wechatCheckLoginStatus(rawQrCode.value);
    if (res && (res.status === 'confirmed' || res.status === 'binded_redirect')) {
      wechatConnected.value = true;
      return;
    }
    if (res && res.status === 'expired') { await loadQrCode(); return; }
  } catch (e) {}
  pollTimer = setTimeout(pollQrStatus, 2000);
}

function setLanguage(lang) {
  tempConfig.value.language = lang;
  locale.value = lang;
}

function setTheme(t) {
  tempConfig.value.theme = t;
  document.documentElement.setAttribute('data-theme', t);
}

function setAccentColor(color) {
  tempConfig.value.accentColor = color;
  document.documentElement.style.setProperty('--user-accent', color);
  const hex = color.replace('#', '');
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);
  document.documentElement.style.setProperty('--user-accent-rgb', `${r}, ${g}, ${b}`);
}

async function selectWorkspaceDir() {
  if (window.appAPI) {
    const dir = await window.appAPI.selectWorkspaceDir();
    if (dir) tempConfig.value.workspaceDir = dir;
  }
}

async function finishOnboarding() {
  if (window.appAPI) {
    // 只写入用户在向导中实际修改过的字段，不动已有配置
    const fieldsToCheck = ['language', 'theme', 'accentColor', 'workspaceDir', 'provider'];
    for (const key of fieldsToCheck) {
      if (tempConfig.value[key] !== initialSnapshot[key]) {
        await window.appAPI.setConfig(key, tempConfig.value[key]);
      }
    }
    // API Key：仅当用户在向导里输入了新 Key 时才写入
    if (tempConfig.value.apiKey && window.appAPI.setApiKey) {
      await window.appAPI.setApiKey(tempConfig.value.provider, tempConfig.value.apiKey);
    }
    await window.appAPI.setConfig('onboarded', true);
    // 仅当 provider 发生了变更时，才重新绑定默认模型
    if (tempConfig.value.provider !== initialSnapshot.provider) {
      const models = await window.appAPI.getModels(tempConfig.value.provider);
      const defaultModel = models.find(m => m.default) || models[0];
      if (defaultModel) await window.appAPI.setConfig('model', defaultModel.id);
    }
  }
  emit('complete');
}
</script>

<style scoped>
.onboarding-layout {
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--bg-root);
}

.onboarding-card {
  width: 100%;
  max-width: 420px;
  display: flex;
  flex-direction: column;
  align-items: center;
}

/* ── Logo ── */
.wizard-logo {
  position: relative;
  width: 140px;
  height: 92px;
  margin: 0 auto 48px;
}

.logo-layer {
  position: absolute;
  inset: 0;
  background-color: var(--logo-color);
  -webkit-mask-size: contain;
  mask-size: contain;
  -webkit-mask-repeat: no-repeat;
  mask-repeat: no-repeat;
  -webkit-mask-position: center;
  mask-position: center;
  opacity: 0;
  transition: opacity 0.6s ease;
}

.logo-layer.visible {
  opacity: 1;
}

/* ── Body ── */
.wizard-body {
  width: 100%;
  min-height: 300px;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.page {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.page-center {
  justify-content: center;
  align-items: center;
}

.page-top {
  justify-content: flex-start;
  align-items: center;
  padding-top: 20px;
}

/* ── Page 1 ── */
.theme-options {
  display: flex;
  gap: 16px;
  width: 100%;
}

.btn-theme {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 14px;
  background-color: var(--bg-secondary);
  border: 2px solid var(--border-color);
  border-radius: 8px;
  color: var(--text-primary);
  cursor: pointer;
  transition: all 0.2s ease;
  font-weight: 500;
}

.btn-theme:hover { background-color: var(--bg-hover); }

.btn-theme.active {
  border-color: var(--user-accent);
  background-color: rgba(var(--user-accent-rgb, 59, 130, 246), 0.1);
  color: var(--user-accent);
}

.color-options {
  display: flex;
  gap: 14px;
  flex-wrap: wrap;
  justify-content: center;
}

.color-circle {
  width: 24px;
  height: 24px;
  border-radius: 12px;
  border: 2px solid transparent;
  cursor: pointer;
  transition: transform 0.2s, box-shadow 0.2s;
}

.color-circle:hover { transform: scale(1.15); }

.color-circle.active {
  border-color: var(--text-primary);
  transform: scale(1.15);
  box-shadow: 0 0 0 2px var(--bg-root), 0 0 0 4px var(--text-primary);
}

/* ── Page 2 ── */
.workspace-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}

.workspace-input {
  flex: 1;
  padding: 12px 16px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  color: var(--text-tertiary);
  font-size: 14px;
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  transition: border-color 0.2s;
}

.workspace-input.filled { color: var(--text-primary); }
.workspace-input:hover { border-color: var(--user-accent); }

.workspace-btn {
  padding: 12px 16px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  color: var(--text-secondary);
  font-size: 16px;
  font-weight: 700;
  letter-spacing: 2px;
  cursor: pointer;
  transition: all 0.2s;
}

.workspace-btn:hover {
  background: var(--bg-hover);
  border-color: var(--user-accent);
  color: var(--user-accent);
}

/* ── Page 3 ── */
.llm-form {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

/* ── Page 4 ── */
.wechat-toggle {
  display: flex;
  align-items: center;
  gap: 16px;
  cursor: pointer;
}

.wechat-icon {
  width: 36px;
  height: 36px;
  opacity: 0.3;
  filter: grayscale(1) brightness(1.5);
  transition: all 0.3s ease;
}

.wechat-icon.active {
  opacity: 1;
  filter: grayscale(0) brightness(0) invert(1);
}

:root[data-theme="light"] .wechat-icon.active {
  filter: grayscale(0) brightness(0);
}

.switch-label {
  position: relative;
  display: inline-block;
  width: 44px;
  height: 22px;
  flex-shrink: 0;
}

.switch-label input { opacity: 0; width: 0; height: 0; }

.slider {
  position: absolute;
  cursor: pointer;
  inset: 0;
  background-color: var(--bg-secondary);
  border: 1px solid var(--border-color);
  transition: .3s;
}

.slider:before {
  position: absolute;
  content: "";
  height: 16px;
  width: 16px;
  left: 2px;
  bottom: 2px;
  background-color: var(--text-tertiary);
  transition: .3s;
}

input:checked + .slider {
  background-color: var(--user-accent);
  border-color: var(--user-accent);
}

input:checked + .slider:before {
  background-color: var(--bg-primary);
  transform: translateX(22px);
}

.slider.round { border-radius: 22px; }
.slider.round:before { border-radius: 50%; }

.qr-area {
  margin-top: 16px;
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 140px;
}

.qr-loading {
  width: 140px;
  height: 140px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-secondary);
  border-radius: 12px;
}

.qr-done {
  width: 140px;
  height: 140px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--user-accent);
  background: rgba(var(--user-accent-rgb), 0.1);
  border-radius: 12px;
}

.qr-image {
  width: 140px;
  height: 140px;
  border-radius: 12px;
}

/* ── Nav ── */
.wizard-nav {
  width: 100%;
  display: flex;
  align-items: center;
  margin-top: 48px;
}

.nav-spacer { flex: 1; }

.nav-arrow {
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: 1px solid var(--border-color);
  border-radius: 50%;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.2s;
}

.nav-arrow:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
  border-color: var(--user-accent);
}

.nav-launch {
  color: var(--user-accent);
  border-color: var(--user-accent);
}

.nav-launch:hover {
  background: rgba(var(--user-accent-rgb), 0.15);
}

</style>
