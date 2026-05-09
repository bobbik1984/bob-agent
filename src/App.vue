<template>
  <div class="app-shell">
    <!-- 标题栏拖拽区域 -->
    <div class="titlebar titlebar-drag">
      <div class="titlebar-left titlebar-no-drag">
        <Hexagon class="app-logo" :size="20" />
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
            <component :is="item.icon" class="nav-icon" :size="18" />
            <span class="nav-label">{{ item.label }}</span>
          </button>
        </nav>

        <!-- 对话列表 (仅对话视图) -->
        <div v-if="currentView === 'chat'" class="conversation-list">
          <button class="new-chat-btn btn btn-ghost" @click="createNewChat">
            <Plus :size="16" />
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
              <button class="delete-btn btn-icon" title="删除对话" @click.stop="deleteChat(conv.id)">
                <X :size="14" />
              </button>
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
import { Hexagon, MessageSquare, Inbox, Settings, Plus, X } from 'lucide-vue-next';

// ── 状态 ─────────────────────────────────────────────
const isSetupComplete = ref(false);
const currentView = ref('chat');
const conversations = ref([]);
const activeConversationId = ref(null);
const currentModel = ref('');

const navItems = [
  { id: 'chat', icon: MessageSquare, label: '对话' },
  { id: 'inbox', icon: Inbox, label: '智能收件箱' },
  { id: 'settings', icon: Settings, label: '设置' },
];

const modelLabel = computed(() => {
  if (!currentModel.value) return '未配置';
  const name = currentModel.value;
  if (name.includes('deepseek')) return 'DeepSeek';
  if (name.includes('gpt-4.1-mini')) return 'GPT-4.1 Mini';
  if (name.includes('gpt-4.1')) return 'GPT-4.1';
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

async function deleteChat(id) {
  if (!confirm('确定要删除这个对话吗？')) return;
  await window.electronAPI.deleteConversation(id);
  conversations.value = conversations.value.filter(c => c.id !== id);
  
  if (activeConversationId.value === id) {
    if (conversations.value.length > 0) {
      activeConversationId.value = conversations.value[0].id;
    } else {
      await createNewChat();
    }
  }
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
  color: var(--text-primary);
  opacity: 0.9;
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
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
  transition: color var(--duration-fast);
}

.nav-item.active .nav-icon {
  color: var(--accent-primary);
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
  display: flex;
  align-items: center;
  justify-content: space-between;
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
}

.conv-title {
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  padding-right: var(--space-2);
}

.delete-btn {
  opacity: 0;
  padding: 2px;
  color: var(--text-tertiary);
  transition: all var(--duration-fast);
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
