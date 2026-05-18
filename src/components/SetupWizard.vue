<template>
  <div class="onboarding-layout animate-fade-in">
    <div class="onboarding-card card">
      <div class="steps-indicator">
        <div :class="['step-dot', { active: step >= 1 }]"></div>
        <div :class="['step-dot', { active: step >= 2 }]"></div>
        <div :class="['step-dot', { active: step >= 3 }]"></div>
      </div>

      <!-- Step 1: Welcome & Style -->
      <div v-if="step === 1" class="step-content animate-slide-up">
        <div class="step-header">
          <Palette :size="48" class="step-icon" />
          <h2 class="wizard-title">定义专属风格</h2>
          <p class="wizard-subtitle">欢迎来到 Bob-Agent，请挑选您喜欢的主题与色彩。</p>
        </div>

        <div class="form-group">
          <label class="form-label">主题模式</label>
          <div class="theme-options">
            <button :class="['btn-theme', { active: tempConfig.theme === 'dark' }]" @click="setTheme('dark')">
              <Moon :size="20" /> 深色模式 (Dark)
            </button>
            <button :class="['btn-theme', { active: tempConfig.theme === 'light' }]" @click="setTheme('light')">
              <Sun :size="20" /> 浅色模式 (Light)
            </button>
          </div>
        </div>

        <div class="form-group" style="margin-top: 24px;">
          <label class="form-label">专属强调色 (Accent Color)</label>
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
      </div>

      <!-- Step 2: Workspace -->
      <div v-if="step === 2" class="step-content animate-slide-up">
        <div class="step-header">
          <FolderOpen :size="48" class="step-icon" />
          <h2 class="wizard-title">设置工作资产库</h2>
          <p class="wizard-subtitle">Bob-Agent 的知识库与资产将存储在您指定的文件夹中，完全归您所有。</p>
        </div>

        <div class="workspace-picker">
          <div class="current-path" :class="{ 'empty': !tempConfig.workspaceDir }">
            {{ tempConfig.workspaceDir || '尚未选择工作目录...' }}
          </div>
          <button class="btn btn-secondary" @click="selectWorkspaceDir">
            <FolderSearch :size="16" />
            选择文件夹
          </button>
        </div>
        <p class="section-desc" style="margin-top: 16px; text-align: center;">
          推荐：您可以在“我的文档”中新建一个名为 <strong>Bob-Wiki</strong> 的文件夹。
        </p>
      </div>

      <!-- Step 3: LLM Engine -->
      <div v-if="step === 3" class="step-content animate-slide-up">
        <div class="step-header">
          <Monitor :size="48" class="step-icon" />
          <h2 class="wizard-title">接入动力核心</h2>
          <p class="wizard-subtitle">选择您的主力大模型，并提供 API Key 以激活助理的思考能力。</p>
        </div>

        <div class="form-group">
          <label class="form-label">服务商</label>
          <CustomSelect
            v-model="tempConfig.provider"
            :options="providerOptions"
          />
        </div>

        <div class="form-group">
          <label class="form-label">API Key</label>
          <input class="input" type="password" v-model="tempConfig.apiKey" placeholder="sk-..." />
        </div>

        <div class="test-area">
          <button class="btn btn-secondary" @click="testConnection" :disabled="isTesting || !tempConfig.apiKey">
            <Loader2 v-if="isTesting" class="spin" :size="14" />
            <Plug v-else :size="14" />
            测试连接
          </button>
          <span v-if="testResult" :class="['test-badge', testResult.ok ? 'success' : 'error']">
            {{ testResult.ok ? '连接成功' : testResult.message }}
          </span>
        </div>
      </div>

      <div class="step-footer">
        <button class="btn btn-ghost" v-if="step > 1" @click="step--">上一步</button>
        <div class="spacer"></div>
        <button class="btn btn-primary wizard-next" v-if="step < 3" @click="step++">下一步</button>
        <button class="btn btn-primary wizard-next" v-if="step === 3" @click="finishOnboarding" :disabled="!isReady">
          <Rocket :size="16" /> 启动引擎
        </button>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue';
import { Palette, Moon, Sun, FolderOpen, FolderSearch, Monitor, Plug, Loader2, Rocket } from 'lucide-vue-next';
import CustomSelect from './CustomSelect.vue';
import { ACCENT_COLORS } from '@/constants/theme.js';

const emit = defineEmits(['complete']);

const step = ref(1);
const isTesting = ref(false);
const testResult = ref(null);

const tempConfig = ref({
  theme: 'dark',
  accentColor: '#2776bb', // MallOS 蓝 default
  workspaceDir: '',
  provider: 'deepseek',
  apiKey: ''
});

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

const isReady = computed(() => {
  return tempConfig.value.workspaceDir && tempConfig.value.apiKey;
});

onMounted(async () => {
  if (window.electronAPI) {
    const sysConfig = await window.electronAPI.getAllConfig();
    if (sysConfig.theme) tempConfig.value.theme = sysConfig.theme;
    if (sysConfig.accentColor) tempConfig.value.accentColor = sysConfig.accentColor;
    if (sysConfig.workspaceDir) tempConfig.value.workspaceDir = sysConfig.workspaceDir;
    if (sysConfig.provider) tempConfig.value.provider = sysConfig.provider;
    if (sysConfig.apiKey) tempConfig.value.apiKey = sysConfig.apiKey;

    // Apply color and theme instantly for preview
    setTheme(tempConfig.value.theme);
    setAccentColor(tempConfig.value.accentColor);
  }
});

function setTheme(t) {
  tempConfig.value.theme = t;
  document.documentElement.setAttribute('data-theme', t);
}

function setAccentColor(color) {
  tempConfig.value.accentColor = color;
  // --user-accent: the user-chosen brand color, used ONLY for point elements
  // (logos, indicators, calendar dots, step-dots, step-icons).
  // It must NOT override --accent-primary which controls the full theme palette.
  document.documentElement.style.setProperty('--user-accent', color);
  
  const hex = color.replace('#', '');
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);
  document.documentElement.style.setProperty('--user-accent-rgb', `${r}, ${g}, ${b}`);
}

async function selectWorkspaceDir() {
  if (window.electronAPI) {
    const dir = await window.electronAPI.selectWorkspaceDir();
    if (dir) tempConfig.value.workspaceDir = dir;
  }
}

async function testConnection() {
  isTesting.value = true;
  testResult.value = null;
  
  if (window.electronAPI) {
    // Temporary save to test
    await window.electronAPI.setConfig('provider', tempConfig.value.provider);
    await window.electronAPI.setConfig('apiKey', tempConfig.value.apiKey);
    
    try {
      const result = await window.electronAPI.sendChat([
        { role: 'user', content: 'hello' }
      ], { useClerk: false });
      
      if (result.error) {
        testResult.value = { ok: false, message: result.error };
      } else {
        testResult.value = { ok: true, message: '连接成功' };
      }
    } catch (e) {
      testResult.value = { ok: false, message: e.message };
    }
  }
  isTesting.value = false;
}

async function finishOnboarding() {
  if (window.electronAPI) {
    await window.electronAPI.setConfig('theme', tempConfig.value.theme);
    await window.electronAPI.setConfig('accentColor', tempConfig.value.accentColor);
    await window.electronAPI.setConfig('workspaceDir', tempConfig.value.workspaceDir);
    await window.electronAPI.setConfig('provider', tempConfig.value.provider);
    await window.electronAPI.setConfig('apiKey', tempConfig.value.apiKey);
    await window.electronAPI.setConfig('onboarded', true);

    // Get default model and save
    const models = await window.electronAPI.getModels();
    const defaultModel = models.find(m => m.default) || models[0];
    if (defaultModel) {
      await window.electronAPI.setConfig('model', defaultModel.id);
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
  padding: 20px;
}

.onboarding-card {
  width: 100%;
  max-width: 540px;
  min-height: 480px;
  display: flex;
  flex-direction: column;
  padding: 40px;
  box-shadow: 0 20px 40px rgba(0,0,0,0.2);
  border-radius: 12px;
  background-color: var(--bg-primary);
}

.steps-indicator {
  display: flex;
  justify-content: center;
  gap: 12px;
  margin-bottom: 40px;
}

.step-dot {
  width: 10px;
  height: 10px;
  border-radius: 5px;
  background-color: var(--border-color);
  transition: all 0.3s ease;
}

.step-dot.active {
  background-color: var(--user-accent);
  width: 24px;
}

.step-content {
  flex: 1;
  display: flex;
  flex-direction: column;
}

.step-header {
  text-align: center;
  margin-bottom: 32px;
}

.step-icon {
  color: var(--user-accent);
  margin-bottom: 16px;
}

.wizard-title {
  font-size: 24px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 8px;
}

.wizard-subtitle {
  color: var(--text-secondary);
  font-size: 14px;
  line-height: 1.5;
}

.theme-options {
  display: flex;
  gap: 16px;
}

.btn-theme {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 16px;
  background-color: var(--bg-secondary);
  border: 2px solid var(--border-color);
  border-radius: 8px;
  color: var(--text-primary);
  cursor: pointer;
  transition: all 0.2s ease;
  font-weight: 500;
}

.btn-theme:hover {
  background-color: var(--bg-hover);
}

.btn-theme.active {
  border-color: var(--user-accent);
  background-color: rgba(var(--user-accent-rgb, 59, 130, 246), 0.1);
  color: var(--user-accent);
}

.color-options {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
  justify-content: center;
  margin-top: 8px;
}

.color-circle {
  width: 24px;
  height: 24px;
  border-radius: 12px;
  border: 2px solid transparent;
  cursor: pointer;
  transition: transform 0.2s, box-shadow 0.2s;
}

.color-circle:hover {
  transform: scale(1.1);
}

.color-circle.active {
  border-color: var(--text-primary);
  transform: scale(1.1);
  box-shadow: 0 0 0 2px var(--bg-primary), 0 0 0 4px var(--text-primary);
}

.workspace-picker {
  display: flex;
  align-items: center;
  gap: 12px;
  background-color: var(--bg-secondary);
  padding: 16px;
  border-radius: 8px;
  border: 1px dashed var(--border-color);
}

.current-path {
  flex: 1;
  font-family: monospace;
  font-size: 13px;
  color: var(--text-primary);
  word-break: break-all;
}

.current-path.empty {
  color: var(--text-tertiary);
}

.test-area {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-top: 16px;
}

.test-badge {
  font-size: 13px;
  padding: 6px 12px;
  border-radius: 4px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 250px;
}

.test-badge.success { background-color: rgba(var(--user-accent-rgb), 0.1); color: var(--user-accent); }
.test-badge.error { background-color: var(--color-error-bg); color: var(--color-error); }

.step-footer {
  display: flex;
  margin-top: 40px;
  padding-top: 20px;
  border-top: 1px solid var(--border-color);
}

.spacer {
  flex: 1;
}

.wizard-next {
  min-width: 120px;
}

.spin {
  animation: spin 1s linear infinite;
}
@keyframes spin { 100% { transform: rotate(360deg); } }
</style>
