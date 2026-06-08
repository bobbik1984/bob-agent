<template>
  <!-- 关于 & 数据 -->
  <section class="settings-section card">
    <h3 class="section-title">
      <Info :size="16" class="section-icon" />
      {{ $t('settings.about') }}
    </h3>
    <div class="about-info">
      <p>bob-agent v{{ appVersion }}</p>
    </div>
    
    <div style="margin-top: 16px; padding-top: 16px; border-top: 1px solid var(--border-subtle); display: grid; grid-template-columns: repeat(auto-fit, minmax(130px, 1fr)); gap: 12px;">
      <button class="btn btn-primary" style="display: flex; align-items: center; justify-content: center; gap: 6px;" @click="openDocs">
        <BookOpen :size="14" />
        <span>{{ $t('settings.open_docs') }}</span>
      </button>
      
      <button class="btn btn-primary" style="display: flex; align-items: center; justify-content: center; gap: 6px;" @click="openDataDir">
        <FolderOpen :size="14" />
        <span>{{ $t('settings.open_data_dir') }}</span>
      </button>
      
      <button class="btn btn-primary" style="display: flex; align-items: center; justify-content: center; gap: 6px;" @click="openLogDir">
        <FileText :size="14" />
        <span>{{ $t('settings.open_log_dir') }}</span>
      </button>
      
      <button class="btn btn-danger" style="display: flex; align-items: center; justify-content: center; gap: 6px;" @click="factoryReset">
        <Trash2 :size="14" />
        <span>{{ $t('settings.clear_all_data') }}</span>
      </button>
    </div>
  </section>

  <!-- 使用文档弹窗 -->
  <Transition name="briefing-fade">
    <div v-if="showHelpModal" class="wechat-modal-overlay" @click.self="showHelpModal = false">
      <div class="help-modal">
        <div class="briefing-header">
          <div class="briefing-icon"><BookOpen :size="18" /></div>
          <div class="briefing-title" style="flex: 1; font-size: 14px; font-weight: 600; color: var(--text-primary);">{{ $t('settings.open_docs') }}</div>
          <button class="briefing-close" @click="showHelpModal = false" style="background: none; border: none; color: var(--text-tertiary); cursor: pointer; padding: 4px; border-radius: 4px; display: flex; align-items: center; justify-content: center;">
            <X :size="14" />
          </button>
        </div>
        <div class="help-body" v-html="renderedGuide"></div>
      </div>
    </div>
  </Transition>
</template>

<script setup>
import { ref, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import { Info, BookOpen, FolderOpen, FileText, Trash2, X } from 'lucide-vue-next';

const props = defineProps({
  config: { type: Object, required: true },
});
const emit = defineEmits(['config-changed']);
const { t } = useI18n();

const appVersion = ref('0.32.0');
const showHelpModal = ref(false);
const renderedGuide = ref('');

async function openDocs() {
  showHelpModal.value = true;
  if (!renderedGuide.value) {
    try {
      const resp = await fetch('/guide.md');
      const md = await resp.text();
      const raw = marked.parse(md, { breaks: true });
      renderedGuide.value = DOMPurify.sanitize(raw);
    } catch (e) {
      renderedGuide.value = '<p style="color: var(--text-secondary)">Failed to load guide.</p>';
    }
  }
}

function openDataDir() {
  if (window.electronAPI.openDataDir) {
    window.electronAPI.openDataDir();
  }
}

function openLogDir() {
  if (window.electronAPI.openLogDir) {
    window.electronAPI.openLogDir();
  }
}

async function factoryReset() {
  if (confirm(t('modal.factory_reset_warning'))) {
    if (window.electronAPI.factoryReset) {
      await window.electronAPI.factoryReset();
    }
  }
}

onMounted(async () => {
  if (window.electronAPI.getVersion) {
    appVersion.value = await window.electronAPI.getVersion();
  }
});
</script>

<style scoped>
.settings-section {
  margin-bottom: var(--space-5);
}

.section-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--text-lg);
  font-weight: 500;
  margin-bottom: var(--space-4);
  color: var(--text-primary);
}

.about-info {
  color: var(--text-secondary);
  font-size: var(--text-sm);
}

/* ── Help modal ── */
.wechat-modal-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(4px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.help-modal {
  width: 580px;
  max-height: 80vh;
  background: var(--bg-secondary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
}

.briefing-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-subtle);
}

.briefing-close:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.help-body {
  flex: 1;
  overflow-y: auto;
  padding: 24px 28px;
  font-size: var(--text-sm);
  color: var(--text-secondary);
  line-height: 1.8;
}

.help-body :deep(h1) {
  font-size: 20px;
  font-weight: 700;
  color: var(--text-primary);
  margin: 0 0 8px;
}

.help-body :deep(h2) {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
  margin: 20px 0 8px;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--border-subtle);
}

.help-body :deep(h3) {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  margin: 14px 0 4px;
}

.help-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--border-subtle);
  margin: 12px 0;
}

.help-body :deep(ul),
.help-body :deep(ol) {
  padding-left: 20px;
  margin: 4px 0;
}

.help-body :deep(li) {
  margin: 2px 0;
}

.help-body :deep(strong) {
  font-weight: 600;
  color: var(--text-primary);
}

.help-body :deep(code) {
  font-family: var(--font-mono, 'JetBrains Mono', monospace);
  font-size: 12px;
  background: var(--bg-hover);
  padding: 2px 6px;
  border-radius: 4px;
}

.help-body :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin: 8px 0;
  font-size: 13px;
}

.help-body :deep(th),
.help-body :deep(td) {
  padding: 6px 12px;
  border: 1px solid var(--border-subtle);
  text-align: left;
}

.help-body :deep(th) {
  background: var(--bg-hover);
  font-weight: 600;
  color: var(--text-primary);
}

.help-body :deep(p) {
  margin: 6px 0;
}

.help-body :deep(a) {
  color: var(--user-accent);
  text-decoration: none;
}

.help-body :deep(a:hover) {
  text-decoration: underline;
}

/* Transition */
.briefing-fade-enter-active {
  transition: all 0.3s ease;
}
.briefing-fade-leave-active {
  transition: all 0.2s ease;
}
.briefing-fade-enter-from {
  opacity: 0;
  transform: scale(0.95);
}
.briefing-fade-leave-to {
  opacity: 0;
  transform: scale(0.95);
}
</style>
