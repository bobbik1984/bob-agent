<template>
  <Teleport to="body">
    <Transition name="quicknote">
      <div v-if="visible" class="quicknote-overlay" @click.self="close">
        <!-- 中央速记框 -->
        <div class="quicknote-bar">
          <div class="quicknote-bob">
            <div class="quicknote-bob-icon"></div>
          </div>
          <input
            ref="inputRef"
            v-model="text"
            class="quicknote-input"
            :placeholder="placeholder"
            @keydown.enter="submit"
            @keydown.escape="close"
            autocomplete="off"
            spellcheck="false"
          />
        </div>

        <Transition name="quicknote-hint">
          <div v-if="showSaved" class="quicknote-saved">
            <Check :size="14" /> {{ $t('quicknote.saved') }}
          </div>
        </Transition>

        <!-- 底部快捷控制栏 -->
        <div class="quicknote-bottom-bar">
          <button class="quicknote-bottom-btn" @click="openModelSwitcher">
            <Cpu :size="15" />
            <span>{{ $t('quicknote.select_model') }}</span>
          </button>

          <button class="quicknote-bottom-btn" @click="openScanPairing">
            <QrCode :size="15" />
            <span>{{ $t('quicknote.scan_pairing') }}</span>
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup>
import { ref, nextTick, onMounted, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { Check, Cpu, QrCode } from 'lucide-vue-next';

const { t: $t } = useI18n();

const visible = ref(false);
const text = ref('');
const showSaved = ref(false);
const inputRef = ref(null);

const placeholder = ref('灵光一现');

let _justOpened = false;

function open() {
  visible.value = true;
  showSaved.value = false;
  text.value = '';
  _justOpened = true;
  setTimeout(() => {
    _justOpened = false;
  }, 150);
  nextTick(() => {
    inputRef.value?.focus();
  });
}

function openModelSwitcher() {
  window.dispatchEvent(new CustomEvent('open-mobile-model-switcher'));
  close();
}

async function openScanPairing() {
  if (window.appAPI?.scanQrCode) {
    try {
      close();
      await nextTick();
      document.body.classList.add('scanner-active');
      const code = await window.appAPI.scanQrCode();
      document.body.classList.remove('scanner-active');
      
      if (code) {
        try {
          const payload = JSON.parse(code);
          await window.appAPI.setConfig('pairing_payload', payload);
          console.log('Saved pairing payload from FAB:', payload);
          if (window.appAPI.triggerMobileSync) {
            const syncTimeout = new Promise((_, reject) => setTimeout(() => reject(new Error('Sync Timeout')), 15000));
            await Promise.race([
              window.appAPI.triggerMobileSync(payload),
              syncTimeout
            ]);
          }
        } catch (e) {
          console.error('Invalid QR Code JSON:', code);
        }
      }
    } catch (err) {
      document.body.classList.remove('scanner-active');
      console.error('Scan failed:', err);
    }
  } else {
    alert($t('setup.scanner_not_supported') || '当前环境不支持扫码');
  }
}

function close() {
  if (_justOpened) return;
  visible.value = false;
  text.value = '';
}

async function submit() {
  const content = text.value.trim();
  if (!content) {
    close();
    return;
  }

  try {
    // 通过 IPC 写入速记文件
    await window.appAPI.notebookAppendDaily(content);
  } catch (err) {
    console.warn('[QuickNote] IPC fallback:', err);
  }

  // 显示已记录反馈
  text.value = '';
  showSaved.value = true;
  setTimeout(() => {
    showSaved.value = false;
    close();
  }, 800);
}

// 暴露 open 方法供外部调用
defineExpose({ open, close, visible });

// 全局快捷键: Ctrl+Shift+N 打开速记
function onGlobalKey(e) {
  if (e.ctrlKey && e.shiftKey && e.key === 'N') {
    e.preventDefault();
    if (visible.value) { close(); } else { open(); }
  }
}

onMounted(() => document.addEventListener('keydown', onGlobalKey));
onUnmounted(() => document.removeEventListener('keydown', onGlobalKey));
</script>

<style scoped>
/* ── 背景遮罩 + 毛玻璃 ── */
.quicknote-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
}

/* ── 输入条 ── */
.quicknote-bar {
  display: flex;
  align-items: center;
  gap: 0;
  width: min(560px, 85vw);
  height: 48px;
  background: var(--bg-primary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-lg);
  overflow: hidden;
  box-shadow: var(--shadow-lg), 0 0 0 1px var(--border-subtle);
}

/* Bob 图标（胶囊左端） */
.quicknote-bob {
  width: 48px;
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.quicknote-bob-icon {
  width: 20px;
  height: 20px;
  background-color: var(--logo-color);
  -webkit-mask-image: url(/bob_logo.svg);
  mask-image: url(/bob_logo.svg);
  -webkit-mask-size: contain;
  mask-size: contain;
  -webkit-mask-repeat: no-repeat;
  mask-repeat: no-repeat;
  -webkit-mask-position: center;
  mask-position: center;
  opacity: 0.7;
}

/* 输入框 */
.quicknote-input {
  flex: 1;
  height: 100%;
  border: none;
  outline: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 15px;
  font-family: var(--font-sans);
  padding-right: 20px;
  letter-spacing: 0.01em;
}

.quicknote-input::placeholder {
  color: var(--text-tertiary);
  opacity: 0.6;
}

/* ── 已记录反馈 ── */
.quicknote-saved {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 16px;
  font-size: 13px;
  color: var(--accent-primary);
  opacity: 0.9;
}

/* ── 底部快捷栏 ── */
.quicknote-bottom-bar {
  position: absolute;
  bottom: calc(24px + env(safe-area-inset-bottom, 0px));
  left: 24px;
  right: 24px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  pointer-events: none;
}

.quicknote-bottom-btn {
  pointer-events: auto;
  display: flex;
  align-items: center;
  gap: 8px;
  height: 40px;
  padding: 0 16px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-default);
  border-radius: 20px;
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  box-shadow: var(--shadow-md);
  cursor: pointer;
  transition: all 0.2s ease;
  outline: none;
  -webkit-tap-highlight-color: transparent;
}

.quicknote-bottom-btn:hover {
  color: var(--text-primary);
  border-color: var(--border-hover);
  background: var(--bg-tertiary);
}

.quicknote-bottom-btn:active {
  transform: scale(0.95);
  background: var(--bg-primary);
}

/* ── 动画 ── */
.quicknote-enter-active {
  transition: opacity 0.2s ease-out;
}
.quicknote-leave-active {
  transition: opacity 0.15s ease-in;
}
.quicknote-enter-from,
.quicknote-leave-to {
  opacity: 0;
}

.quicknote-enter-active .quicknote-bar {
  transition: transform 0.25s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.2s;
}
.quicknote-leave-active .quicknote-bar {
  transition: transform 0.15s ease-in, opacity 0.12s;
}
.quicknote-enter-from .quicknote-bar {
  transform: scale(0.92) translateY(8px);
  opacity: 0;
}
.quicknote-leave-to .quicknote-bar {
  transform: scale(0.96) translateY(4px);
  opacity: 0;
}

.quicknote-enter-active .quicknote-bottom-btn {
  transition: transform 0.25s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.2s;
}
.quicknote-leave-active .quicknote-bottom-btn {
  transition: transform 0.15s ease-in, opacity 0.12s;
}
.quicknote-enter-from .quicknote-bottom-btn {
  transform: translateY(12px);
  opacity: 0;
}
.quicknote-leave-to .quicknote-bottom-btn {
  transform: translateY(4px);
  opacity: 0;
}

.quicknote-hint-enter-active {
  transition: opacity 0.2s, transform 0.2s;
}
.quicknote-hint-leave-active {
  transition: opacity 0.15s;
}
.quicknote-hint-enter-from {
  opacity: 0;
  transform: translateY(4px);
}
.quicknote-hint-leave-to {
  opacity: 0;
}
</style>
