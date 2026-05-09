<template>
  <div class="app-shell">
    <!-- 标题栏拖拽区域 -->
    <div class="titlebar titlebar-drag">
      <div class="titlebar-left titlebar-no-drag">
        <span class="app-logo">🤖</span>
        <span class="app-name">bob-agent</span>
      </div>
      <div class="titlebar-center">
        <!-- 模型指示器 -->
        <div v-if="currentView === 'chat'" class="model-badge badge badge-accent titlebar-no-drag">
          {{ modelLabel }}
        </div>
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
      <aside class="sidebar">
        <!-- 导航 -->
        <nav class="sidebar-nav">
          <button
            v-for="item in navItems"
            :key="item.id"
            class="nav-item"
            :class="{ active: currentView === item.id }"
            @click="currentView = item.id"
          >
            <span class="nav-icon">{{ item.icon }}</span>
            <span class="nav-label">{{ item.label }}</span>
          </button>
        </nav>

        <!-- 对话列表 (仅对话视图) -->
        <div v-if="currentView === 'chat'" class="conversation-list">
          <button class="new-chat-btn btn btn-ghost" @click="createNewChat">
            <span>＋</span>
            <span>新对话</span>
          </button>

          <div class="conversation-items">
            <button
              v-for="conv in conversations"
              :key="conv.id"
              class="conversation-item"
              :class="{ active: activeConversationId === conv.id }"
              @click="switchConversation(conv.id)"
            >
              <span class="conv-title">{{ conv.title }}</span>
            </button>
          </div>
        </div>

        <!-- 侧栏底部 -->
        <div class="sidebar-footer">
          <div class="version-info">v0.1.0</div>
        </div>
      </aside>

      <!-- 内容区 -->
      <main class="content">
        <ChatView
          v-if="currentView === 'chat'"
          :conversationId="activeConversationId"
          @update-title="updateConversationTitle"
        />
        <InboxView v-else-if="currentView === 'inbox'" />
        <SettingsView
          v-else-if="currentView === 'settings'"
          @config-changed="onConfigChanged"
        />
      </main>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, computed } from 'vue';
import ChatView from './views/ChatView.vue';
import InboxView from './views/InboxView.vue';
import SettingsView from './views/SettingsView.vue';
import SetupWizard from './components/SetupWizard.vue';

// ── 状态 ─────────────────────────────────────────────
const isSetupComplete = ref(false);
const currentView = ref('chat');
const conversations = ref([]);
const activeConversationId = ref(null);
const currentModel = ref('');

const navItems = [
  { id: 'chat', icon: '💬', label: '对话' },
  { id: 'inbox', icon: '📥', label: '智能收件箱' },
  { id: 'settings', icon: '⚙️', label: '设置' },
];

const modelLabel = computed(() => {
  if (!currentModel.value) return '未配置';
  // 简化显示
  const name = currentModel.value;
  if (name.includes('deepseek')) return '🧠 DeepSeek';
  if (name.includes('gpt-4.1-mini')) return '⚡ GPT-4.1 Mini';
  if (name.includes('gpt-4.1')) return '🧠 GPT-4.1';
  return name;
});

// ── 生命周期 ─────────────────────────────────────────
onMounted(async () => {
  // 检查是否已配置
  isSetupComplete.value = await window.electronAPI.isSetupComplete();

  if (isSetupComplete.value) {
    await loadConversations();
    currentModel.value = await window.electronAPI.getConfig('model') || '';
  }
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
  const conv = await window.electronAPI.createConversation('新对话', currentModel.value);
  conversations.value.unshift(conv);
  activeConversationId.value = conv.id;
}

function switchConversation(id) {
  activeConversationId.value = id;
}

function updateConversationTitle(id, title) {
  const conv = conversations.value.find(c => c.id === id);
  if (conv) conv.title = title;
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
  padding: 0 var(--space-4);
  background: var(--bg-primary);
  border-bottom: 1px solid var(--border-subtle);
  flex-shrink: 0;
}

.titlebar-left {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.app-logo {
  font-size: var(--text-lg);
}

.app-name {
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--text-secondary);
  letter-spacing: 0.5px;
}

.titlebar-center {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
}

.model-badge {
  cursor: pointer;
  font-size: var(--text-xs);
}

/* ── 主布局 ─────────────────────────────────────────── */
.main-layout {
  flex: 1;
  display: flex;
  overflow: hidden;
}

/* ── 侧栏 ───────────────────────────────────────────── */
.sidebar {
  width: var(--sidebar-width);
  display: flex;
  flex-direction: column;
  background: var(--bg-primary);
  border-right: 1px solid var(--border-subtle);
  flex-shrink: 0;
}

.sidebar-nav {
  display: flex;
  flex-direction: column;
  padding: var(--space-3);
  gap: var(--space-1);
}

.nav-item {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border: none;
  border-radius: var(--radius-md);
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
  font-size: var(--text-lg);
  width: 24px;
  text-align: center;
}

/* ── 对话列表 ───────────────────────────────────────── */
.conversation-list {
  flex: 1;
  display: flex;
  flex-direction: column;
  padding: var(--space-3);
  overflow: hidden;
}

.new-chat-btn {
  width: 100%;
  justify-content: center;
  margin-bottom: var(--space-3);
  border-style: dashed;
}

.conversation-items {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.conversation-item {
  display: block;
  width: 100%;
  padding: var(--space-2) var(--space-3);
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  text-align: left;
  cursor: pointer;
  transition: all var(--duration-fast) var(--ease-out);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.conversation-item:hover {
  background: var(--surface-glass);
  color: var(--text-primary);
}

.conversation-item.active {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

/* ── 侧栏底部 ───────────────────────────────────────── */
.sidebar-footer {
  padding: var(--space-3);
  border-top: 1px solid var(--border-subtle);
}

.version-info {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  text-align: center;
}

/* ── 内容区 ─────────────────────────────────────────── */
.content {
  flex: 1;
  overflow: hidden;
  background: var(--bg-root);
}
</style>
