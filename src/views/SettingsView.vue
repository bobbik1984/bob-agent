<template>
  <div class="settings-view">
    <div class="settings-scroll">
      <h2 class="settings-title">
        <SettingsIcon :size="22" class="title-icon" />
        设置
      </h2>

      <!-- AI 模型配置 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Cpu :size="16" class="section-icon" />
          AI 模型
        </h3>

        <div class="form-group">
          <label class="form-label">服务商</label>
          <CustomSelect
            v-model="config.provider"
            :options="providerOptions"
            @change="onProviderChange"
          />
        </div>

        <div class="form-group" v-if="config.provider !== 'ollama'">
          <label class="form-label">API Key</label>
          <input
            v-model="config.apiKey"
            :type="showApiKey ? 'text' : 'password'"
            class="input"
            placeholder="sk-..."
            @blur="saveConfig('apiKey', config.apiKey)"
          />
          <button class="btn-icon toggle-key" @click="showApiKey = !showApiKey">
            <EyeOff v-if="showApiKey" :size="16" />
            <Eye v-else :size="16" />
          </button>
        </div>

        <div class="form-group" v-if="config.provider === 'custom'">
          <label class="form-label">API 地址</label>
          <input
            v-model="config.baseURL"
            class="input"
            placeholder="https://your-api.com/v1"
            @blur="saveConfig('baseURL', config.baseURL)"
          />
        </div>

        <div class="form-group">
          <label class="form-label">默认模型</label>
          <CustomSelect
            v-model="config.model"
            :options="computedModelOptions"
            @change="saveConfig('model', config.model)"
          />
        </div>

        <!-- 连接测试 -->
        <div class="test-section">
          <button class="btn btn-ghost" @click="testConnection" :disabled="isTesting">
            <Loader2 v-if="isTesting" :size="14" class="animate-spin" />
            <Plug v-else :size="14" />
            <span>{{ isTesting ? '测试中...' : '测试连接' }}</span>
          </button>
          <span v-if="testResult" class="test-result" :class="testResult.ok ? 'success' : 'error'">
            {{ testResult.message }}
          </span>
        </div>
      </section>

      <!-- 外观 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Palette :size="16" class="section-icon" />
          外观
        </h3>
        <div class="form-group">
          <label class="form-label">主题</label>
          <CustomSelect
            v-model="config.theme"
            :options="themeOptions"
            @change="saveConfig('theme', config.theme)"
          />
        </div>
      </section>

      <!-- 工作目录 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <FolderOpen :size="16" class="section-icon" />
          工作目录
        </h3>
        <p class="section-desc">配置后，bob-agent 可以主动浏览和读取该目录下的文件</p>
        <div class="form-group workspace-group">
          <input
            v-model="config.workspaceDir"
            class="input"
            placeholder="点击右侧按钮选择目录..."
            readonly
          />
          <button class="btn btn-ghost browse-btn" @click="selectWorkspaceDir">
            <FolderOpen :size="14" />
            <span>浏览</span>
          </button>
        </div>
        <button
          v-if="config.workspaceDir"
          class="btn-clear"
          @click="clearWorkspaceDir"
        >
          清除工作目录
        </button>
      </section>

      <!-- 工具与扩展 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Puzzle :size="16" class="section-icon" />
          工具与扩展 (Skills)
        </h3>
        <p class="section-desc">配置外部技能所在的目录，Agent 会在启动时自动加载它们。</p>
        <div class="form-group workspace-group">
          <input
            v-model="config.externalSkillsDir"
            class="input"
            placeholder="点击右侧按钮选择外部技能目录..."
            readonly
          />
          <button class="btn btn-ghost browse-btn" @click="selectExternalSkillsDir">
            <FolderOpen :size="14" />
            <span>浏览</span>
          </button>
        </div>
        <button
          v-if="config.externalSkillsDir"
          class="btn-clear"
          @click="clearExternalSkillsDir"
        >
          清除技能目录
        </button>
      </section>

      <!-- 关于 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Info :size="16" class="section-icon" />
          关于
        </h3>
        <div class="about-info">
          <p>bob-agent v0.1.0</p>
          <p class="about-desc">AI 桌面私人秘书 — 智能对话 + 图片识别 + 日程管理 + 文件分析</p>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, defineEmits } from 'vue';
import { Settings as SettingsIcon, Cpu, Eye, EyeOff, Plug, Loader2, Palette, Info, FolderOpen, Puzzle } from 'lucide-vue-next';
import CustomSelect from '../components/CustomSelect.vue';

const emit = defineEmits(['config-changed']);

const providerOptions = [
  { label: 'DeepSeek', value: 'deepseek' },
  { label: 'OpenAI', value: 'openai' },
  { label: 'Ollama (本地)', value: 'ollama' },
  { label: '自定义', value: 'custom' },
];

const themeOptions = [
  { label: '暗色', value: 'dark' },
  { label: '亮色 (开发中)', value: 'light' },
];

const config = ref({
  provider: 'deepseek',
  apiKey: '',
  model: '',
  baseURL: '',
  theme: 'dark',
  workspaceDir: '',
  externalSkillsDir: '',
});

const availableModels = ref([]);
const computedModelOptions = computed(() => {
  return availableModels.value.map(m => ({
    label: `${m.label} (${m.id})`,
    value: m.id
  }));
});

const showApiKey = ref(false);
const isTesting = ref(false);
const testResult = ref(null);

onMounted(async () => {
  const allConfig = await window.electronAPI.getAllConfig();
  config.value = {
    provider: allConfig.provider || 'deepseek',
    apiKey: allConfig.apiKey || '',
    model: allConfig.model || '',
    baseURL: allConfig.baseURL || '',
    theme: allConfig.theme || 'dark',
    workspaceDir: allConfig.workspaceDir || '',
    externalSkillsDir: allConfig.externalSkillsDir || '',
  };
  await loadModels();
});

async function loadModels() {
  availableModels.value = await window.electronAPI.getModels();
}

async function saveConfig(key, value) {
  await window.electronAPI.setConfig(key, value);
  emit('config-changed');
}

async function onProviderChange() {
  await saveConfig('provider', config.value.provider);
  await loadModels();
  // 自动选择默认模型
  const defaultModel = availableModels.value.find(m => m.default);
  if (defaultModel) {
    config.value.model = defaultModel.id;
    await saveConfig('model', defaultModel.id);
  }
}

async function testConnection() {
  isTesting.value = true;
  testResult.value = null;

  try {
    const result = await window.electronAPI.sendChat([
      { role: 'user', content: '你好，请回复"连接成功"' }
    ]);

    if (result.error) {
      testResult.value = { ok: false, message: result.error };
    } else {
      testResult.value = { ok: true, message: '连接成功' };
    }
  } catch (err) {
    testResult.value = { ok: false, message: err.message };
  } finally {
    isTesting.value = false;
  }
}

async function selectWorkspaceDir() {
  const dirPath = await window.electronAPI.selectWorkspaceDir();
  if (dirPath) {
    config.value.workspaceDir = dirPath;
    await saveConfig('workspaceDir', dirPath);
  }
}

async function clearWorkspaceDir() {
  config.value.workspaceDir = '';
  await saveConfig('workspaceDir', '');
}

async function selectExternalSkillsDir() {
  const dirPath = await window.electronAPI.selectDir();
  if (dirPath) {
    config.value.externalSkillsDir = dirPath;
    await saveConfig('externalSkillsDir', dirPath);
  }
}

async function clearExternalSkillsDir() {
  config.value.externalSkillsDir = '';
  await saveConfig('externalSkillsDir', '');
}
</script>

<style scoped>
.settings-view {
  height: 100%;
  overflow: hidden;
}

.settings-scroll {
  height: 100%;
  overflow-y: auto;
  padding: var(--space-8);
  max-width: 600px;
  margin: 0 auto;
}

.settings-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-2xl);
  font-weight: 600;
  margin-bottom: var(--space-6);
}

.title-icon {
  color: var(--text-secondary);
}

.section-icon {
  color: var(--text-tertiary);
  vertical-align: middle;
}

.settings-section {
  margin-bottom: var(--space-5);
}

.section-title {
  font-size: var(--text-lg);
  font-weight: 500;
  margin-bottom: var(--space-4);
  color: var(--text-primary);
}

.form-group {
  margin-bottom: var(--space-4);
  position: relative;
}

.form-label {
  display: block;
  font-size: var(--text-sm);
  color: var(--text-secondary);
  margin-bottom: var(--space-2);
  font-weight: 500;
}

.toggle-key {
  position: absolute;
  right: var(--space-2);
  top: 28px;
}

select.input {
  cursor: pointer;
  appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23a0a0b8' d='M6 8L1 3h10z'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 12px center;
  padding-right: 32px;
}

.test-section {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-top: var(--space-3);
}

.test-result {
  font-size: var(--text-sm);
}

.test-result.success {
  color: var(--color-success);
}

.test-result.error {
  color: var(--color-error);
}

.about-info {
  color: var(--text-secondary);
  font-size: var(--text-sm);
}

.about-desc {
  color: var(--text-tertiary);
  margin-top: var(--space-1);
}

.section-desc {
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  margin-bottom: var(--space-4);
}

.workspace-group {
  display: flex;
  gap: var(--space-2);
  align-items: center;
}

.workspace-group .input {
  flex: 1;
  cursor: default;
}

.browse-btn {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  white-space: nowrap;
  flex-shrink: 0;
}

.btn-clear {
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  font-size: var(--text-xs);
  cursor: pointer;
  padding: var(--space-1) 0;
  margin-top: var(--space-2);
  font-family: var(--font-sans);
  transition: color var(--duration-fast);
}

.btn-clear:hover {
  color: var(--color-error);
}
</style>
