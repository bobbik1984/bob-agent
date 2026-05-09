<template>
  <div class="app-shell">
    <!-- 标题栏拖拽区域 -->
    <div class="titlebar titlebar-drag">
      <div class="titlebar-left titlebar-no-drag">
        <span class="app-name">Bob</span>
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
        <!-- 新对话按钮 -->
        <div class="sidebar-top">
          <button class="new-chat-btn btn btn-ghost" @click="createNewChat">
            <Plus :size="16" />
            <span>新对话</span>
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
              <span class="delete-btn btn-icon" title="删除对话" @click.stop="requestDeleteChat(conv.id)">
                <X :size="14" />
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
import { Inbox, Settings, Plus, X } from 'lucide-vue-next';

// ── 状态 ─────────────────────────────────────────────
const isSetupComplete = ref(false);
const currentView = ref('chat');
const conversations = ref([]);
const activeConversationId = ref(null);
const currentModel = ref('');
const renamingId = ref(null);
const renameText = ref('');
const renameInputRef = ref(null);

const bottomNavItems = [
  { id: 'inbox', icon: Inbox, label: '智能收件箱' },
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

.app-name {
  font-size: var(--text-sm);
  font-weight: 700;
  color: var(--text-primary);
  letter-spacing: 0.5px;
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

/* ── 侧栏顶部：新对话按钮 ────────────────────────── */
.sidebar-top {
  padding: var(--space-3);
  flex-shrink: 0;
}

.new-chat-btn {
  width: 100%;
  justify-content: center;
  border-style: dashed;
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
</style>
