<template>
  <Transition name="briefing-fade">
    <div v-if="visible" class="morning-briefing">
      <div class="briefing-header">
        <div class="briefing-icon">☀️</div>
        <div class="briefing-title">{{ t('dream.morning_title') }}</div>
        <button class="briefing-close" @click="dismiss" :title="t('dream.dismiss')">
          <X :size="14" />
        </button>
      </div>

      <div class="briefing-body">
        <div class="briefing-content" v-html="renderedBriefing"></div>

        <div v-if="stats.staled > 0 || stats.merged > 0" class="briefing-maintenance">
          <span class="maintenance-label">🧹 {{ t('dream.maintenance') }}</span>
          <span v-if="stats.staled > 0" class="maintenance-item">
            {{ t('dream.archived', { count: stats.staled }) }}
          </span>
          <span v-if="stats.merged > 0" class="maintenance-item">
            {{ t('dream.merged', { count: stats.merged }) }}
          </span>
        </div>
      </div>

      <div class="briefing-actions">
        <button class="briefing-action-btn primary" @click="startChat">
          {{ t('dream.continue_chat') }}
        </button>
        <button class="briefing-action-btn secondary" @click="dismiss">
          {{ t('dream.got_it') }}
        </button>
      </div>
    </div>
  </Transition>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { X } from 'lucide-vue-next';
import { marked } from 'marked';
import DOMPurify from 'dompurify';

const { t } = useI18n();
const emit = defineEmits(['chat', 'dismiss']);

const visible = ref(false);
const briefingText = ref('');
const stats = ref({ staled: 0, merged: 0 });
let cleanupListener = null;

const renderedBriefing = computed(() => {
  if (!briefingText.value) return '';
  const raw = marked.parse(briefingText.value, { breaks: true });
  return DOMPurify.sanitize(raw);
});

async function loadDreamReport() {
  try {
    const report = await window.electronAPI.getDreamReport();
    if (report && report.briefing) {
      briefingText.value = report.briefing;
      stats.value = report.stats || {};
      visible.value = true;
    }
  } catch (err) {
    console.error('[MorningBriefing] Failed to load dream report:', err);
  }
}

function startChat() {
  emit('chat', briefingText.value);
  visible.value = false;
  window.electronAPI.dismissDream();
}

function dismiss() {
  visible.value = false;
  emit('dismiss');
  window.electronAPI.dismissDream();
}

onMounted(() => {
  loadDreamReport();

  // 监听后台做梦完成事件
  cleanupListener = window.electronAPI.onDreamCompleted((report) => {
    if (report && report.briefing) {
      briefingText.value = report.briefing;
      stats.value = report.stats || {};
      visible.value = true;
    }
  });
});

onUnmounted(() => {
  if (cleanupListener) cleanupListener();
});
</script>

<style scoped>
.morning-briefing {
  max-width: 480px;
  margin: 0 auto 24px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  overflow: hidden;
  box-shadow: var(--shadow-lg);
}

.briefing-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-subtle);
}

.briefing-icon {
  font-size: 18px;
  line-height: 1;
}

.briefing-title {
  flex: 1;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--text-primary);
}

.briefing-close {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 4px;
  border-radius: var(--radius-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
}

.briefing-close:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.briefing-body {
  padding: 16px;
}

.briefing-content {
  font-size: var(--text-sm);
  color: var(--text-secondary);
  line-height: 1.7;
}

.briefing-content :deep(ul) {
  padding-left: 16px;
  margin: 4px 0;
}

.briefing-content :deep(li) {
  margin: 2px 0;
}

.briefing-content :deep(strong) {
  color: var(--text-primary);
}

.briefing-content :deep(p) {
  margin: 6px 0;
}

.briefing-maintenance {
  margin-top: 12px;
  padding-top: 10px;
  border-top: 1px solid var(--border-subtle);
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.maintenance-label {
  font-size: 11px;
  color: var(--text-tertiary);
}

.maintenance-item {
  font-size: 11px;
  color: var(--text-tertiary);
  background: var(--bg-hover);
  padding: 2px 8px;
  border-radius: 10px;
}

.briefing-actions {
  display: flex;
  gap: 8px;
  padding: 0 16px 16px;
}

.briefing-action-btn {
  flex: 1;
  padding: 8px 12px;
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  font-weight: 500;
  cursor: pointer;
  border: 1px solid var(--border-subtle);
  transition: all 0.2s;
}

.briefing-action-btn.primary {
  background: var(--accent);
  color: #fff;
  border-color: var(--accent);
}

.briefing-action-btn.primary:hover {
  filter: brightness(1.1);
}

.briefing-action-btn.secondary {
  background: var(--bg-primary);
  color: var(--text-secondary);
}

.briefing-action-btn.secondary:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

/* Transition */
.briefing-fade-enter-active {
  transition: all 0.4s ease;
}
.briefing-fade-leave-active {
  transition: all 0.3s ease;
}
.briefing-fade-enter-from {
  opacity: 0;
  transform: translateY(-12px);
}
.briefing-fade-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
</style>
