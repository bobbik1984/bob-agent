<template>
  <div class="settings-view">
    <div class="settings-scroll">
      <h2 class="settings-title">⚙️ 设置</h2>

      <!-- AI 模型配置 -->
      <section class="settings-section card">
        <h3 class="section-title">🤖 AI 模型</h3>

        <div class="form-group">
          <label class="form-label">服务商</label>
          <select v-model="config.provider" class="input" @change="onProviderChange">
            <option value="deepseek">DeepSeek</option>
            <option value="openai">OpenAI</option>
            <option value="ollama">Ollama (本地)</option>
            <option value="custom">自定义</option>
          </select>
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
            {{ showApiKey ? '🙈' : '👁️' }}
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
          <select v-model="config.model" class="input" @change="saveConfig('model', config.model)">
            <option v-for="m in availableModels" :key="m.id" :value="m.id">
              {{ m.label }} ({{ m.id }})
            </option>
          </select>
        </div>

        <!-- 连接测试 -->
        <div class="test-section">
          <button class="btn btn-ghost" @click="testConnection" :disabled="isTesting">
            {{ isTesting ? '测试中...' : '🔌 测试连接' }}
          </button>
          <span v-if="testResult" class="test-result" :class="testResult.ok ? 'success' : 'error'">
            {{ testResult.message }}
          </span>
        </div>
      </section>

      <!-- 外观 -->
      <section class="settings-section card">
        <h3 class="section-title">🎨 外观</h3>
        <div class="form-group">
          <label class="form-label">主题</label>
          <select v-model="config.theme" class="input" @change="saveConfig('theme', config.theme)">
            <option value="dark">🌙 暗色</option>
            <option value="light">☀️ 亮色 (开发中)</option>
          </select>
        </div>
      </section>

      <!-- 关于 -->
      <section class="settings-section card">
        <h3 class="section-title">ℹ️ 关于</h3>
        <div class="about-info">
          <p>bob-agent v0.1.0</p>
          <p class="about-desc">AI 桌面私人秘书 — 智能对话 + 图片识别 + 日程管理 + 文件分析</p>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, defineEmits } from 'vue';

const emit = defineEmits(['config-changed']);

const config = ref({
  provider: 'deepseek',
  apiKey: '',
  model: '',
  baseURL: '',
  theme: 'dark',
});

const availableModels = ref([]);
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
      testResult.value = { ok: false, message: `❌ ${result.error}` };
    } else {
      testResult.value = { ok: true, message: '✅ 连接成功！' };
    }
  } catch (err) {
    testResult.value = { ok: false, message: `❌ ${err.message}` };
  } finally {
    isTesting.value = false;
  }
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
  font-size: var(--text-2xl);
  font-weight: 600;
  margin-bottom: var(--space-6);
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
</style>
