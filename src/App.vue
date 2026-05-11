<template>
  <div class="app-shell">
    <!-- 启动画面 Splash Screen -->
    <Transition name="splash-fade">
      <div v-if="showSplash" class="splash-overlay">
        <div class="splash-content">
          <img src="/bob_logo.svg" class="splash-logo" alt="Bob" />
          <div class="splash-loader"></div>
        </div>
      </div>
    </Transition>
    <!-- 标题栏拖拽区域 -->
    <div class="titlebar titlebar-drag">
      <div class="titlebar-left titlebar-no-drag">
        <svg class="app-logo" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 152.85 99.94">
          <g>
            <path fill="currentColor" d="M166.3,82.45a29.91,29.91,0,0,1-52.92,19.11,29.91,29.91,0,0,1-46,0A29.91,29.91,0,0,1,14.45,82.45V15.72a2.3,2.3,0,0,1,2.3-2.3H26a2.3,2.3,0,0,1,2.3,2.3V57.24a29.92,29.92,0,0,1,39.12,6.09,29.91,29.91,0,0,1,39.11-6.09V15.72a2.3,2.3,0,0,1,2.3-2.3H118a2.3,2.3,0,0,1,2.3,2.3V57.24a29.92,29.92,0,0,1,46,25.21Zm-13.8,0a16.11,16.11,0,1,0-16.11,16.1A16.11,16.11,0,0,0,152.5,82.45Zm-46,0a16.11,16.11,0,1,0-16.1,16.1A16.1,16.1,0,0,0,106.48,82.45Zm-46,0a16.11,16.11,0,1,0-16.11,16.1A16.11,16.11,0,0,0,60.47,82.45Z" transform="translate(-13.95 -12.92)"/>
            <path fill="none" stroke="currentColor" stroke-miterlimit="10" d="M136.39,52.54a29.94,29.94,0,0,0-16.11,4.7V15.72a2.3,2.3,0,0,0-2.3-2.3h-9.2a2.3,2.3,0,0,0-2.3,2.3V57.24a29.91,29.91,0,0,0-39.11,6.09,29.92,29.92,0,0,0-39.12-6.09V15.72a2.3,2.3,0,0,0-2.3-2.3h-9.2a2.3,2.3,0,0,0-2.3,2.3V82.45a29.91,29.91,0,0,0,52.92,19.11,29.91,29.91,0,0,0,46,0,29.91,29.91,0,1,0,23-49Z" transform="translate(-13.95 -12.92)"/>
            <path fill="none" stroke="currentColor" stroke-miterlimit="10" d="M60.47,82.45A16.11,16.11,0,1,1,44.36,66.34,16.11,16.11,0,0,1,60.47,82.45Z" transform="translate(-13.95 -12.92)"/>
            <path fill="none" stroke="currentColor" stroke-miterlimit="10" d="M106.48,82.45a16.11,16.11,0,1,1-16.1-16.11A16.1,16.1,0,0,1,106.48,82.45Z" transform="translate(-13.95 -12.92)"/>
            <path fill="none" stroke="currentColor" stroke-miterlimit="10" d="M152.5,82.45a16.11,16.11,0,1,1-16.11-16.11A16.11,16.11,0,0,1,152.5,82.45Z" transform="translate(-13.95 -12.92)"/>
          </g>
        </svg>
      </div>
      <div class="titlebar-right">
        <!-- Windows 控件由 titleBarOverlay 提供 -->
      </div>
    </div>

    <!-- 首次启动向导 -->
    <SetupWizard v-if="!isSetupComplete" @complete="onSetupComplete" />

    <!-- 主界面 -->
    <div v-else class="main-layout">
      <!-- 侧栏 -->
      <aside 
        class="sidebar"
        :style="{
          width: isSidebarCollapsed ? '0px' : sidebarWidth + 'px',
          minWidth: isSidebarCollapsed ? '0px' : '200px'
        }"
      >
        <!-- 新对话按钮 -->
        <div class="sidebar-top">
          <button class="new-chat-btn btn btn-ghost" @click="createNewChat">
            <Plus :size="16" />
            <span>新对话</span>
          </button>
          <button class="theme-toggle-btn btn-icon" @click="toggleTheme" :title="currentTheme === 'dark' ? '切换到亮色模式' : '切换到暗色模式'">
            <Sun v-if="currentTheme === 'dark'" :size="16" />
            <Moon v-else :size="16" />
          </button>
        </div>

        <!-- 对话列表（始终显示） -->
        <div class="conversation-list">
          <div class="conversation-items">
            <div
              v-for="conv in conversations"
              :key="conv.id"
              class="conversation-item"
              :class="{ active: activeConversationId === conv.id && currentView === 'chat' }"
              @click="switchConversation(conv.id); currentView = 'chat'"
              @dblclick.stop="startRename(conv)"
            >
              <div class="conv-body">
                <div class="conv-row-1">
                  <input
                    v-if="renamingId === conv.id"
                    v-model="renameText"
                    class="rename-input"
                    @keydown.enter="confirmRename(conv)"
                    @keydown.esc="cancelRename"
                    @blur="confirmRename(conv)"
                    @click.stop
                    ref="renameInputRef"
                  />
                  <span v-else class="conv-title">{{ conv.title }}</span>
                  <span class="conv-time">{{ timeAgo(conv.updated_at) }}</span>
                </div>
                <div class="conv-row-2">
                  {{ conv.last_message ? (conv.last_role === 'assistant' ? 'Bob: ' : '') + conv.last_message : '\u00A0' }}
                </div>
              </div>
              <span class="delete-btn btn-icon" title="删除对话" @click.stop="requestDeleteChat(conv.id)">
                <X :size="12" />
              </span>
            </div>
          </div>
        </div>

        <!-- 底部导航 -->
        <nav class="sidebar-footer-nav">
          <button
            v-for="item in bottomNavItems"
            :key="item.id"
            class="nav-item"
            :class="{ active: currentView === item.id }"
            @click="currentView = item.id"
          >
            <component :is="item.icon" class="nav-icon" :size="16" />
            <span class="nav-label">{{ item.label }}</span>
          </button>
        </nav>
      </aside>

      <!-- 拖拽把手 -->
      <div v-show="!isSidebarCollapsed" class="sidebar-resizer" @mousedown="startResize"></div>

      <!-- 侧边栏居中折叠按钮 -->
      <button 
        class="sidebar-collapse-float" 
        :class="{ 'is-collapsed': isSidebarCollapsed }"
        :style="{ left: isSidebarCollapsed ? '0px' : sidebarWidth + 'px' }" 
        @click="toggleSidebar"
      >
        <ChevronRight v-if="isSidebarCollapsed" :size="14" />
        <ChevronLeft v-else :size="14" />
      </button>

      <!-- 内容区 -->
      <main class="content">
        <ChatView
          v-show="currentView === 'chat'"
          :conversationId="activeConversationId"
          @update-title="updateConversationTitle"
        />
        <InboxView v-if="currentView === 'inbox'" />
        <SettingsView
          v-if="currentView === 'settings'"
          @config-changed="onConfigChanged"
        />
      </main>
    </div>

    <!-- 自定义删除确认弹窗 -->
    <div v-if="showDeleteModal" class="modal-overlay">
      <div class="modal-card">
        <h3 class="modal-title">删除对话</h3>
        <p class="modal-desc">确定要删除这个对话吗？此操作不可恢复。</p>
        <div class="modal-actions">
          <button class="btn btn-ghost" @click="cancelDeleteChat">取消</button>
          <button class="btn btn-danger" @click="confirmDeleteChat">确定删除</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, computed, nextTick } from 'vue';
import ChatView from './views/ChatView.vue';
import InboxView from './views/InboxView.vue';
import SettingsView from './views/SettingsView.vue';
import SetupWizard from './components/SetupWizard.vue';
import { Inbox, Settings, Plus, X, Sun, Moon, ChevronLeft, ChevronRight } from 'lucide-vue-next';

// ── 状态 ─────────────────────────────────────────────
const isSetupComplete = ref(false);
const currentView = ref('chat');
const conversations = ref([]);
const activeConversationId = ref(null);
const currentModel = ref('');
const renamingId = ref(null);
const renameText = ref('');
const renameInputRef = ref(null);
const currentTheme = ref('dark');
const showSplash = ref(true);

const sidebarWidth = ref(260);
const isSidebarCollapsed = ref(false);
const isResizing = ref(false);

function startResize(e) {
  isResizing.value = true;
  document.addEventListener('mousemove', handleResize);
  document.addEventListener('mouseup', stopResize);
  document.body.style.cursor = 'col-resize';
  document.body.style.userSelect = 'none';
}

function handleResize(e) {
  if (!isResizing.value) return;
  let newWidth = e.clientX;
  if (newWidth < 200) newWidth = 200;
  if (newWidth > 600) newWidth = 600;
  sidebarWidth.value = newWidth;
}

function stopResize() {
  isResizing.value = false;
  document.removeEventListener('mousemove', handleResize);
  document.removeEventListener('mouseup', stopResize);
  document.body.style.cursor = '';
  document.body.style.userSelect = '';
  window.electronAPI.setConfig('sidebarWidth', sidebarWidth.value);
}

function toggleSidebar() {
  isSidebarCollapsed.value = !isSidebarCollapsed.value;
}

async function toggleTheme() {
  currentTheme.value = currentTheme.value === 'dark' ? 'light' : 'dark';
  document.documentElement.setAttribute('data-theme', currentTheme.value);
  await window.electronAPI.setConfig('theme', currentTheme.value);
  if (window.electronAPI.updateTheme) {
    window.electronAPI.updateTheme(currentTheme.value);
  }
}

const bottomNavItems = [
  { id: 'inbox', icon: Inbox, label: '日程' },
  { id: 'settings', icon: Settings, label: '设置' },
];

const modelInfo = computed(() => {
  if (!currentModel.value) return { name: '未配置', logo: null };
  const name = currentModel.value.toLowerCase();
  
  if (name.includes('deepseek')) {
    return { name: 'DeepSeek', logo: '/logos/deepseek.png' };
  }
  if (name.includes('gpt-4') || name.includes('openai')) {
    return { name: 'OpenAI', logo: '/logos/openai.png' };
  }
  if (name.includes('llama') || name.includes('ollama')) {
    return { name: 'Ollama', logo: null };
  }
  if (name.includes('gemini')) {
    return { name: 'Gemini', logo: '/logos/gemini.png' };
  }
  return { name: currentModel.value, logo: null };
});

// ── 生命周期 ─────────────────────────────────────────
onMounted(async () => {
  // 检查是否已配置
  isSetupComplete.value = await window.electronAPI.isSetupComplete();

  if (isSetupComplete.value) {
    await loadConversations();
    currentModel.value = await window.electronAPI.getConfig('model') || '';
    // 恢复 UI 缩放偏好和主题和侧边栏宽度
    const savedWidth = await window.electronAPI.getConfig('sidebarWidth');
    if (savedWidth) sidebarWidth.value = savedWidth;

    const uiScale = await window.electronAPI.getConfig('uiScale');
    if (uiScale) {
      document.documentElement.setAttribute('data-ui-scale', uiScale);
    }
    const theme = await window.electronAPI.getConfig('theme');
    if (theme) {
      currentTheme.value = theme;
      document.documentElement.setAttribute('data-theme', theme);
      if (window.electronAPI.updateTheme) {
        window.electronAPI.updateTheme(theme);
      }
    }
  }

  // 启动画面淡出 — 保证至少展示 1.2 秒
  setTimeout(() => { showSplash.value = false; }, 1200);
});

// ── 对话管理 ─────────────────────────────────────────
async function loadConversations() {
  conversations.value = await window.electronAPI.getConversations();
  // 没有对话就创建一个
  if (conversations.value.length === 0) {
    await createNewChat();
  } else if (!activeConversationId.value) {
    activeConversationId.value = conversations.value[0].id;
  }
}

async function createNewChat() {
  if (activeConversationId.value) {
    window.electronAPI.summarizeSession(activeConversationId.value).catch(e => console.error(e));
  }
  const conv = await window.electronAPI.createConversation('新对话', currentModel.value);
  conversations.value.unshift(conv);
  activeConversationId.value = conv.id;
}

function switchConversation(id) {
  if (activeConversationId.value && activeConversationId.value !== id) {
    // Trigger background summarization for the old conversation
    window.electronAPI.summarizeSession(activeConversationId.value).catch(err => {
      console.error('Background session summarization failed:', err);
    });
  }
  activeConversationId.value = id;
}

const showDeleteModal = ref(false);
const pendingDeleteId = ref(null);

function requestDeleteChat(id) {
  pendingDeleteId.value = id;
  showDeleteModal.value = true;
}

async function confirmDeleteChat() {
  const id = pendingDeleteId.value;
  if (!id) return;
  
  await window.electronAPI.deleteConversation(id);
  conversations.value = conversations.value.filter(c => c.id !== id);
  
  if (activeConversationId.value === id) {
    // If we are deleting the active chat, we don't need to summarize it.
    if (conversations.value.length > 0) {
      activeConversationId.value = conversations.value[0].id;
    } else {
      await createNewChat();
    }
  }
  
  showDeleteModal.value = false;
  pendingDeleteId.value = null;
}

function cancelDeleteChat() {
  showDeleteModal.value = false;
  pendingDeleteId.value = null;
}

// ── 重命名对话 ───────────────────────────────────────
async function startRename(conv) {
  renamingId.value = conv.id;
  renameText.value = conv.title;
  await nextTick();
  // Focus the input
  const inputs = document.querySelectorAll('.rename-input');
  if (inputs.length > 0) inputs[inputs.length - 1].focus();
}

async function confirmRename(conv) {
  const newTitle = renameText.value.trim();
  if (newTitle && newTitle !== conv.title) {
    conv.title = newTitle;
    // 持久化到 conversations 表
    await window.electronAPI.renameConversation(conv.id, newTitle);
  }
  renamingId.value = null;
  renameText.value = '';
}

function cancelRename() {
  renamingId.value = null;
  renameText.value = '';
}

function updateConversationTitle(id, title) {
  const conv = conversations.value.find(c => c.id === id);
  if (conv) conv.title = title;
}

// ── 相对时间 ─────────────────────────────────────────
function timeAgo(dateStr) {
  if (!dateStr) return '';
  const now = Date.now();
  const then = new Date(dateStr + 'Z').getTime(); // SQLite stores UTC
  const diff = Math.max(0, now - then);
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return '刚刚';
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d`;
  const months = Math.floor(days / 30);
  return `${months}mo`;
}

// ── 设置 ─────────────────────────────────────────────
async function onSetupComplete() {
  isSetupComplete.value = true;
  currentModel.value = await window.electronAPI.getConfig('model') || '';
  await loadConversations();
}

async function onConfigChanged() {
  currentModel.value = await window.electronAPI.getConfig('model') || '';
}
</script>

<style scoped>
.app-shell {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--bg-root);
}

/* ── 标题栏 ─────────────────────────────────────────── */
.titlebar {
  height: var(--titlebar-height);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--space-4) 0 var(--space-2);
  background: var(--bg-primary);
  border-bottom: 1px solid var(--border-subtle);
  flex-shrink: 0;
}

.titlebar-left {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.sidebar-toggle-btn {
  color: var(--text-secondary);
  background: transparent;
  padding: var(--space-1);
}
.sidebar-toggle-btn:hover {
  color: var(--text-primary);
}

.app-logo {
  height: 18px;
  width: auto;
  color: var(--logo-color);
  margin-left: 0;
}




/* ── 主布局 ─────────────────────────────────────────── */
.main-layout {
  flex: 1;
  display: flex;
  position: relative;
  overflow: hidden;
}

/* ── 侧栏 ───────────────────────────────────────────── */
.sidebar {
  /* 移除原本固定的宽度和 resize: horizontal，改为内联样式控制 */
  max-width: 600px;
  overflow-x: hidden;
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  border-right: 1px solid var(--border-subtle);
  flex-shrink: 0;
  transition: width var(--duration-fast) var(--ease-out), min-width var(--duration-fast) var(--ease-out);
}

.sidebar-resizer {
  width: 6px;
  background: transparent;
  cursor: col-resize;
  flex-shrink: 0;
  z-index: 10;
  margin-left: -3px; /* 让把手居中覆盖在边框上 */
  margin-right: -3px;
  position: relative;
}

.sidebar-resizer::after {
  content: '';
  position: absolute;
  top: 0;
  bottom: 0;
  left: 3px;
  width: 1px;
  background: transparent;
  transition: background var(--duration-fast);
}

.sidebar-resizer:hover::after, .sidebar-resizer:active::after {
  background: var(--accent-primary);
}

.sidebar-collapse-float {
  position: absolute;
  top: 50%;
  transform: translateY(-50%) translateX(-100%);
  width: 14px;
  height: 48px;
  background: var(--surface-glass);
  border: 1px solid var(--border-subtle);
  border-right: none;
  border-radius: 4px 0 0 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
  cursor: pointer;
  z-index: 20;
  transition: all var(--duration-fast);
}

.sidebar-collapse-float:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.sidebar-collapse-float.is-collapsed {
  transform: translateY(-50%);
  border-right: 1px solid var(--border-subtle);
  border-left: none;
  border-radius: 0 4px 4px 0;
  background: var(--bg-primary);
  box-shadow: var(--shadow-sm);
}

/* ── 侧栏顶部：新对话按钮 ────────────────────────── */
.sidebar-top {
  padding: var(--space-3);
  flex-shrink: 0;
  display: flex;
  gap: var(--space-2);
}

.new-chat-btn {
  flex: 1;
  justify-content: center;
  border-style: dashed;
}

.theme-toggle-btn {
  height: auto;
  aspect-ratio: 1 / 1;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-md);
  color: var(--text-secondary);
  background: transparent;
  border: 1px solid var(--border-subtle);
  cursor: pointer;
  transition: all var(--duration-fast);
}

.theme-toggle-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

/* ── 对话列表 ───────────────────────────────────────── */
.conversation-list {
  flex: 1;
  display: flex;
  flex-direction: column;
  padding: 0 var(--space-3);
  overflow: hidden;
}

.conversation-items {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.conversation-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: var(--space-2) var(--space-3);
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-family: var(--font-sans);
  text-align: left;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.conv-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.conv-row-1 {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  position: relative;
}

.conv-title {
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: var(--text-sm);
  line-height: 1.3;
}

.conv-time {
  flex-shrink: 0;
  font-size: 10px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  line-height: 1;
  transition: opacity var(--duration-fast);
}

.conversation-item:hover .conv-time {
  opacity: 0;
}

.conv-row-2 {
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.3;
}

.delete-btn {
  position: absolute;
  right: var(--space-3);
  top: 50%;
  transform: translateY(-50%);
  height: 16px;
  width: 16px;
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  padding: 0;
  flex-shrink: 0;
  color: var(--text-tertiary);
  transition: all var(--duration-fast);
  background: var(--bg-secondary);
  border-radius: 2px;
}

.conversation-item:hover .delete-btn {
  opacity: 1;
}

.delete-btn:hover {
  color: var(--error);
  background: var(--surface-glass);
}

.conversation-item:hover {
  background: var(--surface-glass);
  color: var(--text-primary);
}

.conversation-item.active {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

/* ── 侧栏底部导航 ─────────────────────────────────── */
.sidebar-footer-nav {
  display: flex;
  flex-direction: column;
  padding: var(--space-3);
  gap: 1px;
  margin-top: auto;
  flex-shrink: 0;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
}

.nav-item:hover {
  background: var(--surface-glass);
  color: var(--text-primary);
}

.nav-item.active {
  background: var(--gradient-subtle);
  color: var(--accent-tertiary);
  font-weight: 500;
}

.nav-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
  transition: color var(--duration-fast);
}

.nav-item.active .nav-icon {
  color: var(--accent-primary);
}

/* ── 对话重命名 ─────────────────────────────────────── */
.rename-input {
  flex: 1;
  background: var(--bg-tertiary);
  border: 1px solid var(--accent-primary);
  border-radius: var(--radius-sm);
  color: var(--text-primary);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  padding: 2px 6px;
  outline: none;
}

/* ── 内容区 ─────────────────────────────────────────── */
.content {
  flex: 1;
  overflow: hidden;
  background: var(--bg-root);
}

/* ── 启动画面 ──────────────────────────────────────── */
.splash-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  background: var(--bg-root, #0c0c0c);
  display: flex;
  align-items: center;
  justify-content: center;
}

.splash-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 32px;
}

.splash-logo {
  width: 120px;
  height: auto;
  opacity: 0.6;
  filter: var(--logo-filter, brightness(0.8));
  animation: splash-breathe 2s ease-in-out infinite;
}

@keyframes splash-breathe {
  0%, 100% { opacity: 0.4; transform: scale(1); }
  50% { opacity: 0.7; transform: scale(1.03); }
}

.splash-loader {
  width: 48px;
  height: 2px;
  background: var(--border-subtle, #333);
  border-radius: 1px;
  overflow: hidden;
  position: relative;
}

.splash-loader::after {
  content: '';
  position: absolute;
  inset: 0;
  width: 50%;
  background: var(--accent-primary, #6366f1);
  border-radius: 1px;
  animation: splash-slide 1.2s ease-in-out infinite;
}

@keyframes splash-slide {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(300%); }
}

/* Splash 淡出过渡 */
.splash-fade-leave-active {
  transition: opacity 0.5s ease;
}
.splash-fade-leave-to {
  opacity: 0;
}
</style>
