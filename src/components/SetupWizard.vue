<template>
  <div class="setup-wizard animate-fade-in">
    <div class="wizard-card card">
      <div class="wizard-header">
        <Hexagon :size="56" class="wizard-logo" />
        <h1 class="wizard-title">{{ $t('setup.welcome') }}</h1>
        <p class="wizard-subtitle">{{ $t('setup.subtitle') }}</p>
      </div>

      <!-- Step 1: 选择服务商 -->
      <div v-if="step === 1" class="wizard-step animate-slide-up">
        <h3 class="step-title">{{ $t('setup.step1') }}</h3>
        <div class="provider-grid">
          <button
            v-for="p in providers"
            :key="p.id"
            class="provider-card"
            :class="{ selected: selectedProvider === p.id }"
            @click="selectedProvider = p.id"
          >
            <component :is="p.icon" class="provider-icon" :size="28" />
            <span class="provider-name">{{ p.name }}</span>
            <span class="provider-desc">{{ p.desc }}</span>
          </button>
        </div>
        <button class="btn btn-primary wizard-next" :disabled="!selectedProvider" @click="step = 2">{{ $t('setup.next') }}</button>
      </div>

      <!-- Step 2: 填写 API Key -->
      <div v-if="step === 2" class="wizard-step animate-slide-up">
        <h3 class="step-title">{{ $t('setup.step2') }}</h3>
        <p class="step-desc" v-if="selectedProvider === 'ollama'">{{ $t('setup.ollama_desc') }}</p>
        <template v-else>
          <p class="step-desc">{{ $t('setup.get_key_prefix') }}{{ providerName }}{{ $t('setup.get_key_suffix') }}</p>
          <input
            v-model="apiKey"
            type="password"
            class="input wizard-input"
            placeholder="sk-..."
            @keydown.enter="finish"
          />
        </template>
        <div class="wizard-actions">
          <button class="btn btn-ghost" @click="step = 1">{{ $t('setup.back') }}</button>
          <button
            class="btn btn-primary"
            :disabled="selectedProvider !== 'ollama' && !apiKey.trim()"
            @click="finish"
          >
            开始使用
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { Hexagon, Brain, Globe, Laptop, Settings } from 'lucide-vue-next';

const emit = defineEmits(['complete']);
const { t } = useI18n();

const step = ref(1);
const selectedProvider = ref('');
const apiKey = ref('');

const providers = [
  { id: 'deepseek', icon: Brain, name: 'DeepSeek', desc: '推荐 · 性价比最高' },
  { id: 'openai', icon: Globe, name: 'OpenAI', desc: 'GPT-4.1 系列' },
  { id: 'ollama', icon: Laptop, name: 'Ollama', desc: '本地模型 · 无需联网' },
  { id: 'custom', icon: Settings, name: '自定义', desc: 'OpenAI 兼容端点' },
];

const providerName = computed(() => {
  const p = providers.find(p => p.id === selectedProvider.value);
  return p?.name || '';
});

async function finish() {
  // 保存配置
  await window.electronAPI.setConfig('provider', selectedProvider.value);
  if (apiKey.value.trim()) {
    await window.electronAPI.setConfig('apiKey', apiKey.value.trim());
  }

  // 获取默认模型
  const models = await window.electronAPI.getModels();
  const defaultModel = models.find(m => m.default) || models[0];
  if (defaultModel) {
    await window.electronAPI.setConfig('model', defaultModel.id);
  }

  emit('complete');
}
</script>

<style scoped>
.setup-wizard {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-root);
}

.wizard-card {
  width: 480px;
  max-width: 90vw;
  padding: var(--space-10);
}

.wizard-header {
  text-align: center;
  margin-bottom: var(--space-8);
}

.wizard-logo {
  margin: 0 auto var(--space-4);
  color: var(--text-primary);
  opacity: 0.9;
}

.wizard-title {
  font-size: var(--text-2xl);
  font-weight: 700;
  background: var(--gradient-brand);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}

.wizard-subtitle {
  color: var(--text-tertiary);
  margin-top: var(--space-2);
}

.step-title {
  font-size: var(--text-lg);
  font-weight: 500;
  margin-bottom: var(--space-4);
}

.step-desc {
  color: var(--text-secondary);
  font-size: var(--text-sm);
  margin-bottom: var(--space-4);
}

.provider-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-3);
  margin-bottom: var(--space-6);
}

.provider-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-5) var(--space-3);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  background: var(--surface-glass);
  cursor: pointer;
  transition: all var(--duration-normal) var(--ease-out);
  font-family: var(--font-sans);
}

.provider-card:hover {
  border-color: var(--border-default);
  background: var(--bg-hover);
}

.provider-card.selected {
  border-color: var(--accent-primary);
  background: var(--gradient-subtle);
  box-shadow: var(--shadow-glow);
}

.provider-icon {
  margin-bottom: var(--space-1);
  color: var(--text-secondary);
}
.provider-card.selected .provider-icon {
  color: var(--accent-primary);
}

.provider-name {
  font-weight: 600;
  font-size: var(--text-base);
  color: var(--text-primary);
}

.provider-desc {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
}

.wizard-input {
  margin-bottom: var(--space-6);
}

.wizard-next {
  width: 100%;
  padding: var(--space-3);
}

.wizard-actions {
  display: flex;
  justify-content: space-between;
  gap: var(--space-3);
  margin-top: var(--space-4);
}

.wizard-actions .btn {
  flex: 1;
  padding: var(--space-3);
}
</style>
