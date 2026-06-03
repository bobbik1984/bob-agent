<template>
  <div class="browser-enable-card">
    <div class="card-header">
      <div class="card-icon">
        <Globe :size="24" />
      </div>
      <div class="card-title-area">
        <div class="card-type">浏览器增强</div>
        <div class="card-title">需要启用浏览器来访问此网页</div>
      </div>
    </div>

    <div class="card-body">
      <div class="info-row">
        <span class="info-label">网页</span>
        <span class="info-value url-value">{{ displayUrl }}</span>
      </div>
      <div class="info-row" v-if="browserPath">
        <span class="info-label">浏览器</span>
        <span class="info-value">{{ browserName }}</span>
      </div>
      <div class="feature-list">
        <div class="feature-item">
          <Check :size="12" />
          <span>静默后台运行，不弹出窗口</span>
        </div>
        <div class="feature-item">
          <Check :size="12" />
          <span>支持 JS 动态页面</span>
        </div>
        <div class="feature-item">
          <Check :size="12" />
          <span>空闲 5 分钟自动关闭</span>
        </div>
      </div>
      <label class="remember-toggle" v-if="!confirmed">
        <input type="checkbox" v-model="remember" />
        <span>记住我的选择</span>
      </label>
    </div>

    <div class="card-footer" v-if="!confirmed">
      <button class="btn btn-ghost" @click="$emit('cancel')">跳过</button>
      <button class="btn btn-primary" @click="handleConfirm">
        <Globe :size="14" />
        启用浏览器
      </button>
    </div>
    <div class="card-footer confirmed" v-else>
      <Check :size="14" />
      <span>浏览器已启用，正在加载页面...</span>
    </div>
  </div>
</template>

<script setup>
import { ref, computed } from 'vue';
import { Globe, Check } from 'lucide-vue-next';

const props = defineProps({
  url: { type: String, default: '' },
  browserPath: { type: String, default: '' },
  browserDetected: { type: Boolean, default: false },
});

const emit = defineEmits(['confirm', 'cancel']);

const remember = ref(true); // 默认勾选
const confirmed = ref(false);

const displayUrl = computed(() => {
  try {
    const u = new URL(props.url);
    return u.hostname + u.pathname;
  } catch { return props.url; }
});

const browserName = computed(() => {
  if (props.browserPath.toLowerCase().includes('edge')) return 'Microsoft Edge';
  if (props.browserPath.toLowerCase().includes('chrome')) return 'Google Chrome';
  return '本机浏览器';
});

function handleConfirm() {
  confirmed.value = true;
  emit('confirm', { remember: remember.value, url: props.url });
}
</script>

<style scoped>
.browser-enable-card {
  background: var(--surface-card);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  padding: var(--space-4);
  max-width: 420px;
  width: 100%;
  margin: var(--space-2) 0;
  box-shadow: var(--shadow-sm);
}

.card-header {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-bottom: var(--space-3);
  padding-bottom: var(--space-3);
  border-bottom: 1px solid var(--border-subtle);
}

.card-icon {
  color: var(--accent-primary);
}

.card-title-area {
  display: flex;
  flex-direction: column;
}

.card-type {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.card-title {
  font-size: var(--text-base);
  font-weight: 600;
  color: var(--text-primary);
}

.card-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin-bottom: var(--space-4);
}

.info-row {
  display: flex;
  font-size: var(--text-sm);
}

.info-label {
  color: var(--text-tertiary);
  width: 60px;
  flex-shrink: 0;
}

.info-value {
  color: var(--text-secondary);
  flex: 1;
}

.url-value {
  word-break: break-all;
  font-family: var(--font-mono);
  font-size: 0.9em;
}

.feature-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  margin-top: var(--space-2);
  padding: var(--space-2) var(--space-3);
  background: var(--surface-hover);
  border-radius: var(--radius-md);
}

.feature-item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--text-secondary);
}

.feature-item svg {
  color: var(--color-success, #4ade80);
  flex-shrink: 0;
}

.remember-toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  cursor: pointer;
  margin-top: var(--space-2);
}

.remember-toggle input[type="checkbox"] {
  accent-color: var(--accent-primary);
}

.card-footer {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
}

.card-footer.confirmed {
  justify-content: center;
  color: var(--color-success, #4ade80);
  font-size: var(--text-sm);
  gap: var(--space-2);
  padding: var(--space-2);
}

.btn {
  padding: var(--space-2) var(--space-4);
  font-size: var(--text-sm);
  border-radius: var(--radius-md);
  cursor: pointer;
  border: 1px solid transparent;
  display: flex;
  align-items: center;
  gap: var(--space-1);
}

.btn-ghost {
  background: transparent;
  color: var(--text-secondary);
}

.btn-ghost:hover {
  background: var(--surface-hover);
}

.btn-primary {
  background: transparent;
  color: var(--text-primary);
  border: 1px solid var(--border-default);
}

.btn-primary:hover {
  border-color: var(--text-secondary);
  background: var(--surface-hover);
}
</style>
