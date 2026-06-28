<template>
  <div class="installer-root" @mousedown="startDrag">
    <!-- 关闭按钮 - 右上角，hover 时显示 -->
    <button v-show="!showCancelDialog" class="close-btn" @mousedown.stop @click="handleClose">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
    </button>

    <div class="installer-card">
      <!-- Logo 区域 - 逐层显影动画 -->
      <div class="wizard-logo">
        <div class="logo-layer" :class="{ visible: true }" style="-webkit-mask-image: url(/bob_.svg); mask-image: url(/bob_.svg)"></div>
        <div class="logo-layer" :class="{ visible: step >= 1 }" style="-webkit-mask-image: url(/bob_b.svg); mask-image: url(/bob_b.svg)"></div>
        <div class="logo-layer" :class="{ visible: step >= 2 }" style="-webkit-mask-image: url(/bob_bo.svg); mask-image: url(/bob_bo.svg)"></div>
        <div class="logo-layer" :class="{ visible: step >= 3 }" style="-webkit-mask-image: url(/bob_bob.svg); mask-image: url(/bob_bob.svg)"></div>
      </div>

      <div class="wizard-body">
        <!-- Step 1: 选择安装目录 -->
        <div v-if="step === 1" class="page page-center animate-fade-in">
          <div class="version-tag">v0.3.1</div>
          <div class="workspace-row">
            <div class="workspace-input" :class="{ filled: installDir }" @click="selectDir">
              {{ installDir || '选择安装目录...' }}
            </div>
            <button class="workspace-btn" @click="selectDir">...</button>
          </div>
        </div>

        <!-- Step 2: 安装进度 -->
        <div v-if="step === 2" class="page page-center animate-fade-in">
          <div class="progress-text">{{ statusText }}</div>
          <div class="progress-bar-track">
            <div class="progress-bar-fill" :style="{ width: progress + '%' }"></div>
          </div>
          <div class="progress-percent">{{ progress }}%</div>
        </div>

        <!-- Step 3: 安装完成 -->
        <div v-if="step === 3" class="page page-center animate-fade-in">
          <div class="done-text">安装完成</div>
        </div>
      </div>

      <!-- 导航区域 -->
      <div class="wizard-nav">
        <div class="nav-spacer"></div>
        <!-- Step 1: 开始安装 -->
        <button v-if="step === 1" class="nav-arrow nav-launch" @click="startInstall">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
        </button>
        <!-- Step 3: 火箭启动 -->
        <button v-if="step === 3" class="nav-arrow nav-launch nav-rocket" @click="launchBob">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z"></path><path d="m12 15-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z"></path><path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0"></path><path d="M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5"></path></svg>
        </button>
      </div>
    </div>

    <!-- 取消确认浮层 -->
    <Transition name="fade">
      <div v-if="showCancelDialog" class="cancel-overlay" @mousedown.stop>
        <div class="cancel-dialog">
          <p class="cancel-text">安装正在进行中，确定要退出吗？</p>
          <p class="cancel-hint">已解压的文件不会被清理</p>
          <div class="cancel-actions">
            <button class="cancel-btn cancel-btn-continue" @click="dismissCancel">继续安装</button>
            <button class="cancel-btn cancel-btn-quit" @click="confirmCancel">退出安装</button>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

const step = ref(1);
const installDir = ref('');
const progress = ref(0);
const statusText = ref('正在解压文件...');
const showCancelDialog = ref(false);

onMounted(async () => {
  installDir.value = await invoke('get_default_install_dir');

  await listen('install-progress', (event) => {
    progress.value = event.payload;
    if (progress.value >= 100) {
      statusText.value = '正在创建快捷方式...';
    }
  });
});

async function selectDir() {
  const dir = await invoke('select_install_dir');
  if (dir) installDir.value = dir;
}

async function startInstall() {
  step.value = 2;
  try {
    await invoke('install', { installDir: installDir.value });
    progress.value = 100;
    statusText.value = '完成！';
    setTimeout(() => { step.value = 3; }, 600);
  } catch (e) {
    statusText.value = `安装失败: ${e}`;
  }
}

async function launchBob() {
  try {
    await invoke('launch_bob', { installDir: installDir.value });
  } catch (e) {
    console.error(e);
  }
  const win = getCurrentWindow();
  await win.close();
}

function handleClose() {
  if (step.value === 2) {
    showCancelDialog.value = true;
  } else {
    closeWindow();
  }
}

async function closeWindow() {
  const win = getCurrentWindow();
  await win.close();
}

function dismissCancel() {
  showCancelDialog.value = false;
}

async function confirmCancel() {
  const win = getCurrentWindow();
  await win.close();
}

async function startDrag(e) {
  if (e.target.closest('button') || e.target.closest('.workspace-input')) return;
  const win = getCurrentWindow();
  await win.startDragging();
}
</script>

<style>
/* ═══ 安装器全局样式 ═══ */
*,
*::before,
*::after {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

:root {
  --bg-root:        #0c0c0c;
  --bg-secondary:   #1c1c1c;
  --bg-hover:       #2c2c2c;
  --border-color:   rgba(255, 255, 255, 0.12);
  --text-primary:   #e8e8e8;
  --text-secondary: #a0a0a0;
  --text-tertiary:  #6a6a6a;
  --user-accent:    #2776bb;
  --user-accent-rgb: 39, 118, 187;
  --font-sans: 'Inter', 'Noto Sans SC', -apple-system, BlinkMacSystemFont, sans-serif;
}

html, body {
  height: 100%;
  margin: 0;
  background: transparent;
  font-family: var(--font-sans);
  -webkit-font-smoothing: antialiased;
  overflow: hidden;
  user-select: none;
}

#app {
  height: 100%;
}

/* ═══ 安装器根布局 ═══ */
.installer-root {
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  position: relative;
}

.installer-card {
  width: 100%;
  height: 100%;
  padding: 48px 48px 32px;
  background: var(--bg-root);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
}

/* ── Logo ── */
.wizard-logo {
  position: relative;
  width: 120px;
  height: 80px;
  margin: 0 auto 36px;
}

.logo-layer {
  position: absolute;
  inset: 0;
  background-color: var(--text-primary);
  -webkit-mask-size: contain;
  mask-size: contain;
  -webkit-mask-repeat: no-repeat;
  mask-repeat: no-repeat;
  -webkit-mask-position: center;
  mask-position: center;
  opacity: 0;
  transition: opacity 0.8s ease;
}

.logo-layer.visible {
  opacity: 1;
}

/* ── Body ── */
.wizard-body {
  width: 100%;
  min-height: 140px;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.page {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.page-center {
  justify-content: center;
  align-items: center;
}

/* ── Step 1: 路径选择 ── */
.version-tag {
  font-size: 13px;
  color: var(--text-tertiary);
  letter-spacing: 1px;
  margin-bottom: 4px;
}

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
  font-size: 13px;
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

/* ── Step 2: 进度条 ── */
.progress-text {
  font-size: 14px;
  color: var(--text-secondary);
  margin-bottom: 4px;
}

.progress-bar-track {
  width: 100%;
  height: 6px;
  background: var(--bg-secondary);
  border-radius: 3px;
  overflow: hidden;
  position: relative;
}

.progress-bar-fill {
  height: 100%;
  background: var(--user-accent);
  border-radius: 3px;
  transition: width 0.3s ease;
  box-shadow: 0 0 12px rgba(var(--user-accent-rgb), 0.5);
  position: relative;
}

.progress-bar-fill::after {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: linear-gradient(
    90deg,
    transparent,
    rgba(255, 255, 255, 0.3),
    transparent
  );
  animation: shimmer 1.5s infinite;
}

@keyframes shimmer {
  0%   { transform: translateX(-100%); }
  100% { transform: translateX(100%); }
}

.progress-percent {
  font-size: 12px;
  color: var(--text-tertiary);
  margin-top: 2px;
}

/* ── Step 3: 完成 ── */
.done-text {
  font-size: 18px;
  font-weight: 500;
  color: var(--text-primary);
  letter-spacing: 1px;
}

/* ── 导航 ── */
.wizard-nav {
  width: 100%;
  display: flex;
  align-items: center;
  margin-top: 36px;
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

.nav-rocket {
  animation: rocketGlow 2s ease-in-out infinite;
}

@keyframes rocketGlow {
  0%, 100% { box-shadow: 0 0 8px rgba(var(--user-accent-rgb), 0.3); }
  50%      { box-shadow: 0 0 20px rgba(var(--user-accent-rgb), 0.6); }
}

/* ── 动画 ── */
.animate-fade-in {
  animation: fadeIn 0.3s ease;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(8px); }
  to   { opacity: 1; transform: translateY(0); }
}
/* ── 关闭按钮 (右上角 hover 显示) ── */
.close-btn {
  position: absolute;
  top: 10px;
  right: 10px;
  z-index: 100;
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  border-radius: 4px;
  color: var(--text-tertiary);
  cursor: pointer;
  opacity: 0;
  transition: all 0.25s ease;
}

.installer-root:hover .close-btn {
  opacity: 0.35;
}

.close-btn:hover {
  opacity: 1 !important;
  background: rgba(239, 68, 68, 0.12);
  color: #ef4444;
}

/* ── 取消确认浮层 ── */
.cancel-overlay {
  position: absolute;
  inset: 0;
  z-index: 200;
  background: rgba(0, 0, 0, 0.55);
  backdrop-filter: blur(6px);
  display: flex;
  align-items: center;
  justify-content: center;
}

.cancel-dialog {
  padding: 28px 32px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  text-align: center;
}

.cancel-text {
  font-size: 14px;
  color: var(--text-primary);
  margin-bottom: 6px;
}

.cancel-hint {
  font-size: 12px;
  color: var(--text-tertiary);
  margin-bottom: 20px;
}

.cancel-actions {
  display: flex;
  gap: 12px;
  justify-content: center;
}

.cancel-btn {
  padding: 8px 22px;
  border-radius: 6px;
  font-size: 13px;
  font-family: var(--font-sans);
  cursor: pointer;
  border: 1px solid var(--border-color);
  transition: all 0.2s;
}

.cancel-btn-continue {
  background: transparent;
  color: var(--text-secondary);
}

.cancel-btn-continue:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
  border-color: var(--user-accent);
}

.cancel-btn-quit {
  background: rgba(239, 68, 68, 0.08);
  color: #ef4444;
  border-color: rgba(239, 68, 68, 0.25);
}

.cancel-btn-quit:hover {
  background: rgba(239, 68, 68, 0.18);
  border-color: rgba(239, 68, 68, 0.4);
}

/* ── 浮层过渡动画 ── */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
