<template>
  <Teleport to="body">
    <Transition name="quicknote">
      <div v-if="visible" class="quicknote-overlay" @click.self="close">
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
          <button class="quicknote-model-btn" @click="openModelSwitcher" title="切换模型">
            <Cpu :size="18" />
          </button>
        </div>
        <Transition name="quicknote-hint">
          <div v-if="showSaved" class="quicknote-saved">
            <Check :size="14" /> 已记录
          </div>
        </Transition>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup>
import { ref, nextTick, onMounted, onUnmounted } from 'vue';
import { Check, Cpu } from 'lucide-vue-next';

const visible = ref(false);
const text = ref('');
const showSaved = ref(false);
const inputRef = ref(null);

const placeholder = ref('灵光一现');

function open() {
  visible.value = true;
  showSaved.value = false;
  text.value = '';
  nextTick(() => {
    inputRef.value?.focus();
  });
}

function openModelSwitcher() {
  window.dispatchEvent(new CustomEvent('open-mobile-model-switcher'));
  close();
}

function close() {
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
.quicknote-model-btn {
  width: 48px;
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  transition: color 0.2s;
  flex-shrink: 0;
}
.quicknote-model-btn:hover {
  color: var(--text-primary);
}

/* ── 背景遮罩 + 毛玻璃 ── */
.quicknote-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.65);
  backdrop-filter: blur(24px);
  -webkit-backdrop-filter: blur(24px);
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
  margin-top: 12px;
  font-size: 13px;
  color: var(--accent-primary);
  opacity: 0.9;
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
