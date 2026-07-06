<template>
  <div class="app-shell" :class="{ 'is-mobile': isMobile }">
    <!-- 启动画面已移至 index.html (Native Splash) -->
    <!-- 标题栏拖拽区域 (Desktop) -->
    <div v-if="isTauri && !isNativeMobile" class="titlebar titlebar-drag">
      <div class="titlebar-left titlebar-no-drag">
        <svg class="app-logo" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 152.85 99.94" @click="openQuickNote">
          <g>
            <path fill="currentColor" d="M166.3,82.45a29.91,29.91,0,0,1-52.92,19.11,29.91,29.91,0,0,1-46,0A29.91,29.91,0,0,1,14.45,82.45V15.72a2.3,2.3,0,0,1,2.3-2.3H26a2.3,2.3,0,0,1,2.3,2.3V57.24a29.92,29.92,0,0,1,39.12,6.09,29.91,29.91,0,0,1,39.11-6.09V15.72a2.3,2.3,0,0,1,2.3-2.3H118a2.3,2.3,0,0,1,2.3,2.3V57.24a29.92,29.92,0,0,1,46,25.21Zm-13.8,0a16.11,16.11,0,1,0-16.11,16.1A16.11,16.11,0,0,0,152.5,82.45Zm-46,0a16.11,16.11,0,1,0-16.1,16.1A16.1,16.1,0,0,0,106.48,82.45Zm-46,0a16.11,16.11,0,1,0-16.11,16.1A16.11,16.11,0,0,0,60.47,82.45Z" transform="translate(-13.95 -12.92)"/>
            <path fill="none" stroke="currentColor" stroke-miterlimit="10" d="M136.39,52.54a29.94,29.94,0,0,0-16.11,4.7V15.72a2.3,2.3,0,0,0-2.3-2.3h-9.2a2.3,2.3,0,0,0-2.3,2.3V57.24a29.91,29.91,0,0,0-39.11,6.09,29.92,29.92,0,0,0-39.12-6.09V15.72a2.3,2.3,0,0,0-2.3-2.3h-9.2a2.3,2.3,0,0,0-2.3,2.3V82.45a29.91,29.91,0,0,0,52.92,19.11,29.91,29.91,0,0,0,46,0,29.91,29.91,0,1,0,23-49Z" transform="translate(-13.95 -12.92)"/>
            <path fill="none" stroke="currentColor" stroke-miterlimit="10" d="M60.47,82.45A16.11,16.11,0,1,1,44.36,66.34,16.11,16.11,0,0,1,60.47,82.45Z" transform="translate(-13.95 -12.92)"/>
            <path fill="none" stroke="currentColor" stroke-miterlimit="10" d="M106.48,82.45a16.11,16.11,0,1,1-16.1-16.11A16.1,16.1,0,0,1,106.48,82.45Z" transform="translate(-13.95 -12.92)"/>
            <path fill="none" stroke="currentColor" stroke-miterlimit="10" d="M152.5,82.45a16.11,16.11,0,1,1-16.11-16.11A16.11,16.11,0,0,1,152.5,82.45Z" transform="translate(-13.95 -12.92)"/>
          </g>
        </svg>
        <button class="titlebar-theme-btn" @click="toggleTheme" :title="currentTheme === 'dark' ? $t('nav.theme_to_light') : $t('nav.theme_to_dark')">
          <Sun v-if="currentTheme === 'dark'" :size="14" />
          <Moon v-else :size="14" />
        </button>
      </div>
      <div data-tauri-drag-region style="flex: 1; height: 100%;"></div>
      <div class="titlebar-right titlebar-no-drag">
        <button class="win-btn" @click="minimizeWindow" title="最小化">
          <svg width="10" height="1" viewBox="0 0 10 1"><rect fill="currentColor" width="10" height="1"/></svg>
        </button>
        <button class="win-btn" @click="toggleMaximize" title="最大化">
          <svg width="10" height="10" viewBox="0 0 10 10"><rect fill="none" stroke="currentColor" stroke-width="1" x="0.5" y="0.5" width="9" height="9"/></svg>
        </button>
        <button class="win-btn win-close" @click="closeWindow" title="关闭">
          <svg width="10" height="10" viewBox="0 0 10 10"><line stroke="currentColor" stroke-width="1.2" x1="0" y1="0" x2="10" y2="10"/><line stroke="currentColor" stroke-width="1.2" x1="10" y1="0" x2="0" y2="10"/></svg>
        </button>
      </div>
    </div>

    <!-- 首次启动向导 -->
    <SetupWizard v-if="!isSetupComplete" :debug-mode="DEBUG_ONBOARDING === 0" @complete="onSetupComplete" />

    <!-- 主界面 -->
    <div v-else class="main-layout">
      <!-- 侧栏 (Desktop & Mobile Drawer) -->
      <div v-if="isMobile && mobileDrawerOpen" class="mobile-drawer-overlay animate-fade-in" @click="mobileDrawerOpen = false"></div>
      <aside 
        class="sidebar"
        :class="{ 'mobile-drawer': isMobile, 'mobile-drawer-open': isMobile && mobileDrawerOpen }"
        :style="!isMobile ? { width: isSidebarCollapsed ? '0px' : sidebarWidth + 'px', minWidth: isSidebarCollapsed ? '0px' : '200px' } : {}"
      >
        <!-- ═══ 抽屉 1: 对话 ═══ -->
        <div v-show="!isMobile" class="drawer-header" :class="{ active: activeDrawer === 'chat' }" @click="activeDrawer = 'chat'">
          <div class="drawer-header-left">
            <MessageSquare :size="14" />
            <span>{{ $t('nav.conversations') || '对话' }}</span>
          </div>
          <ChevronDown v-if="activeDrawer === 'chat'" :size="14" class="drawer-chevron" />
          <ChevronRight v-else :size="14" class="drawer-chevron" />
        </div>
        <div v-show="activeDrawer === 'chat'" class="drawer-content">
          <!-- 搜索 + 新对话工具栏 -->
          <div class="sidebar-top">
            <button class="new-chat-btn" :class="{ 'sr-compact': isSearchExpanded }" @click="createNewChat" :title="$t('chat.new_conversation')">
              <Plus :size="16" />
              <span class="new-chat-label">{{ $t('chat.new_conversation') }}</span>
            </button>
            <div class="sidebar-search" :class="{ expanded: isSearchExpanded }">
              <button v-if="!isSearchExpanded" class="sidebar-icon-btn search-trigger" @click="expandSearch" title="搜索">
                <Search :size="16" />
              </button>
              <template v-else>
                <Search :size="14" class="search-icon" />
                <input
                  ref="searchInputRef"
                  v-model="searchQuery"
                  class="search-input"
                  :placeholder="$t('chat.search_placeholder')"
                  @input="onSearchInput"
                  @keydown.esc="collapseSearch"
                  @blur="onSearchBlur"
                />
                <button v-if="searchQuery" class="search-clear btn-icon" @click="clearSearch">
                  <X :size="12" />
                </button>
              </template>
            </div>
          </div>

          <!-- T-1301: 搜索结果列表 -->
          <div v-if="searchQuery && searchResults.length > 0" class="search-results">
            <div class="search-results-header">
              {{ searchResults.length }} {{ $t('chat.search_results_count') }}
            </div>
            <div
              v-for="result in searchResults"
              :key="result.id"
              class="search-result-item"
              @click="jumpToSearchResult(result)"
            >
              <div class="search-result-title">{{ result.conv_title }}</div>
              <div class="search-result-snippet" v-html="result.snippet"></div>
              <div class="search-result-time">{{ timeAgo(result.created_at) }}</div>
            </div>
          </div>
          <div v-else-if="searchQuery && searchResults.length === 0 && !isSearching" class="search-empty">
            {{ $t('chat.search_no_results') }}
          </div>

          <!-- 对话列表（搜索时隐藏） -->
          <div v-show="!searchQuery" class="conversation-list">
            <div class="conversation-items">
              <div
                v-for="conv in conversations"
                :key="conv.id"
                class="conversation-item"
                :class="{ active: activeConversationId === conv.id && activeDrawer === 'chat' }"
                @click="switchConversation(conv.id); activeDrawer = 'chat'; if (isMobile) mobileDrawerOpen = false"
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
                <span class="delete-btn btn-icon" :title="$t('nav.delete_chat')" @click.stop="requestDeleteChat(conv.id)">
                  <X :size="12" />
                </span>
              </div>
            </div>
          </div>
        </div>

        <!-- ═══ 抽屉 2: 日程 ═══ -->
        <div v-show="!isMobile" class="drawer-header" :class="{ active: activeDrawer === 'schedule' }" @click="activeDrawer = 'schedule'">
          <div class="drawer-header-left">
            <CalendarDays :size="14" />
            <span>{{ $t('nav.schedule') || '日程' }}</span>
            <span v-if="cronNotifCount > 0" class="drawer-badge">{{ cronNotifCount > 9 ? '9+' : cronNotifCount }}</span>
          </div>
          <ChevronDown v-if="activeDrawer === 'schedule'" :size="14" class="drawer-chevron" />
          <ChevronRight v-else :size="14" class="drawer-chevron" />
        </div>
        <div v-show="!isMobile && activeDrawer === 'schedule'" class="drawer-content">
          <div class="drawer-placeholder">
            <CalendarDays :size="24" style="opacity: 0.3;" />
            <span>{{ $t('nav.schedule_hint') || '日程与待办事项' }}</span>
          </div>
        </div>

        <!-- ═══ 抽屉 2.5: 知识图谱 ═══ -->
        <div v-show="!isMobile" class="drawer-header" :class="{ active: activeDrawer === 'knowledge' }" @click="activeDrawer = 'knowledge'">
          <div class="drawer-header-left">
            <Waypoints :size="14" />
            <span>{{ $t('nav.knowledge') || '知识' }}</span>
          </div>
          <ChevronDown v-if="activeDrawer === 'knowledge'" :size="14" class="drawer-chevron" />
          <ChevronRight v-else :size="14" class="drawer-chevron" />
        </div>
        <div v-show="!isMobile && activeDrawer === 'knowledge'" class="drawer-content" id="kg-sidebar-portal" style="display: flex; flex-direction: column; height: 100%;">
        </div>

        <!-- ═══ 抽屉 3: 设置 ═══ -->
        <div v-show="!isMobile" class="drawer-header" :class="{ active: activeDrawer === 'settings' }" @click="activeDrawer = 'settings'">
          <div class="drawer-header-left">
            <Settings :size="14" />
            <span>{{ $t('nav.settings') }}</span>
          </div>
          <ChevronDown v-if="activeDrawer === 'settings'" :size="14" class="drawer-chevron" />
          <ChevronRight v-else :size="14" class="drawer-chevron" />
        </div>
        <div v-show="!isMobile && activeDrawer === 'settings'" class="drawer-content">
          <nav class="settings-nav">
            <button
              v-for="item in settingsNavItems"
              :key="item.id"
              class="settings-nav-item"
              :class="{ active: activeSettingsPanel === item.id }"
              @click="activeSettingsPanel = item.id"
            >
              <component :is="item.icon" :size="14" />
              <span>{{ item.label }}</span>
            </button>
          </nav>
        </div>
      </aside>

      <!-- 拖拽把手 -->
      <div v-show="!isSidebarCollapsed" class="sidebar-resizer" @mousedown="startResize"></div>

      <!-- 侧边栏居中折叠按钮 -->
      <button 
        v-if="!isMobile"
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
        <!-- 移动端 Settings 顶部导航 -->
        <div v-if="isMobile && activeDrawer === 'settings'" class="mobile-tab-grid">
          <button
            v-for="item in settingsNavItems"
            :key="item.id"
            class="mobile-tab-item"
            :class="{ active: activeSettingsPanel === item.id }"
            @click="activeSettingsPanel = item.id"
          >
            <component :is="item.icon" :size="18" class="tab-icon" />
            <span>{{ item.label }}</span>
          </button>
        </div>

        <div class="view-wrapper" v-show="activeDrawer === 'chat'">
          <ChatView
            ref="chatViewRef"
            :conversationId="activeConversationId"
            @update-title="updateConversationTitle"
            @toggle-sidebar="mobileDrawerOpen = !mobileDrawerOpen"
          />
        </div>
        <div class="view-wrapper" v-show="activeDrawer === 'schedule'">
          <InboxView @toggle-sidebar="mobileDrawerOpen = !mobileDrawerOpen" />
        </div>
        <div class="view-wrapper" v-show="activeDrawer === 'knowledge'">
          <KnowledgeGraphView 
            @toggle-sidebar="mobileDrawerOpen = !mobileDrawerOpen"
          />
        </div>
        <div class="view-wrapper" v-show="activeDrawer === 'settings'">
          <SettingsView
            :activePanel="activeSettingsPanel"
            @config-changed="onConfigChanged"
          />
        </div>
      </main>
    </div>

    <!-- 自定义删除确认弹窗 -->
    <div v-if="showDeleteModal" class="modal-overlay">
      <div class="modal-card">
        <h3 class="modal-title">{{ $t('modal.delete_title') }}</h3>
        <p class="modal-desc">{{ $t('modal.delete_desc') }}</p>
        <div class="modal-actions">
          <button class="btn btn-ghost" @click="cancelDeleteChat">{{ $t('modal.cancel') }}</button>
          <button class="btn btn-danger" @click="confirmDeleteChat">{{ $t('modal.confirm_delete') }}</button>
        </div>
      </div>
    </div>

    <!-- 移动端悬浮球 (灵光一现) -->
    <div v-if="isMobile && isSetupComplete" 
         class="mobile-fab" 
         :class="{ 'is-idle': fabIsIdle, 'is-dragging': isFabDragging }"
         :style="fabStyle"
         @pointerdown="onFabPointerDown"
         @pointermove="onFabPointerMove"
         @pointerup="onFabPointerUp"
         @pointercancel="onFabPointerUp"
         @click.prevent>
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 152.85 99.94">
        <g><path fill="currentColor" d="M166.3,82.45a29.91,29.91,0,0,1-52.92,19.11,29.91,29.91,0,0,1-46,0A29.91,29.91,0,0,1,14.45,82.45V15.72a2.3,2.3,0,0,1,2.3-2.3H26a2.3,2.3,0,0,1,2.3,2.3V57.24a29.92,29.92,0,0,1,39.12,6.09,29.91,29.91,0,0,1,39.11-6.09V15.72a2.3,2.3,0,0,1,2.3-2.3H118a2.3,2.3,0,0,1,2.3,2.3V57.24a29.92,29.92,0,0,1,46,25.21Zm-13.8,0a16.11,16.11,0,1,0-16.11,16.1A16.11,16.11,0,0,0,152.5,82.45Zm-46,0a16.11,16.11,0,1,0-16.1,16.1A16.1,16.1,0,0,0,106.48,82.45Zm-46,0a16.11,16.11,0,1,0-16.11,16.1A16.11,16.11,0,0,0,60.47,82.45Z" transform="translate(-13.95 -12.92)"/></g>
      </svg>
    </div>

    <!-- 闪念速记浮层 -->
    <QuickNoteOverlay ref="quickNoteRef" />

    <!-- 底部导航 (Mobile) -->
    <BottomNavigation 
      v-if="isMobile && isSetupComplete" 
      :active-drawer="activeDrawer" 
      @update:active-drawer="activeDrawer = $event" 
    />
  </div>
</template>

<script setup>
import { ref, onMounted, onUnmounted, computed, nextTick, provide, inject } from 'vue';
import ChatView from './views/ChatView.vue';
import InboxView from './views/InboxView.vue';
import SettingsView from './views/SettingsView.vue';
import KnowledgeGraphView from './views/KnowledgeGraphView.vue';
import SetupWizard from './components/SetupWizard.vue';
import QuickNoteOverlay from './components/QuickNoteOverlay.vue';
import BottomNavigation from './components/BottomNavigation.vue';
import { Inbox, Settings, Plus, X, Sun, Moon, ChevronLeft, ChevronRight, ChevronDown, Search, MessageSquare, CalendarDays, Brain, Plug, FolderOpen, Palette, Info, Sunrise, Waypoints, Menu, Smartphone } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { getModelMeta } from '@/composables/useModelSwitcher';

// Tauri Window API (用于自定义窗口按钮)
function minimizeWindow() { window.appAPI.minimizeWindow(); }
function toggleMaximize() { window.appAPI.toggleMaximize(); }
function closeWindow() { window.appAPI.hideWindow(); } 

const { locale, t } = useI18n();

// ── 状态 ─────────────────────────────────────────────
const isSetupComplete = ref(false);
const currentView = ref('chat');  // legacy — kept for backward compat during transition
const activeDrawer = ref('chat');         // 'chat' | 'schedule' | 'settings'
provide('activeDrawer', activeDrawer);
const activeSettingsPanel = ref('model'); // 'model' | 'connections' | 'workspace' | 'daily_routine' | 'appearance' | 'about'
const chatViewRef = ref(null);
const quickNoteRef = ref(null);
const mobileDrawerOpen = ref(false);
// ── 响应式移动端检测 (宽高比 1:1 断点) ──
function checkMobile() {
  return window.innerHeight > window.innerWidth;
}
const isMobile = ref(checkMobile());
provide('isMobile', isMobile);

const isNativeMobile = /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i.test(navigator.userAgent);
const isTauri = !!window.__TAURI_INTERNALS__;

let resizeDebounce;
function onResizeHandler() {
  clearTimeout(resizeDebounce);
  resizeDebounce = setTimeout(() => { isMobile.value = checkMobile(); }, 100);
}

// ── 闪念速记：全局 provide，子组件 inject 后调用即可 ──
function openQuickNote() {
  quickNoteRef.value?.open();
}
provide('openQuickNote', openQuickNote);


import { watch } from 'vue';
watch(currentView, (newView) => {
  if (newView === 'chat' && chatViewRef.value) {
    chatViewRef.value.refreshModel();
    chatViewRef.value.scrollToBottom();
  }
});

// 抽屉切换时同步刷新
watch(activeDrawer, (newDrawer) => {
  if (newDrawer === 'chat' && chatViewRef.value) {
    chatViewRef.value.refreshModel();
    chatViewRef.value.scrollToBottom();
  }
  if (newDrawer === 'schedule') {
    cronNotifCount.value = 0;
  }
});
const conversations = ref([]);
const activeConversationId = ref(null);
const currentModel = ref('');
const renamingId = ref(null);
const renameText = ref('');
const renameInputRef = ref(null);
const currentTheme = ref('dark');
provide('currentTheme', currentTheme);

const showSplash = ref(true);

const sidebarWidth = ref(260);
const isSidebarCollapsed = ref(false);
const isResizing = ref(false);

// T-1301: 搜索状态
const searchQuery = ref('');
const searchResults = ref([]);
const isSearching = ref(false);
const isSearchExpanded = ref(false);
const searchInputRef = ref(null);
let searchDebounce = null;

// T-1303: Cron 通知状态
const cronNotifCount = ref(0);
let unlistenSchedulerGlobal = null;

// ====== 调试开关：强制显示向导 ======
// 1 = 正常运行
// 0 = 强制进入初次设置页面（并且点击完成后不会修改实际的数据库状态，方便反复测试）
const DEBUG_ONBOARDING = 1;

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
  window.appAPI.setConfig('sidebarWidth', sidebarWidth.value);
}

function toggleSidebar() {
  isSidebarCollapsed.value = !isSidebarCollapsed.value;
}

async function toggleTheme() {
  currentTheme.value = currentTheme.value === 'dark' ? 'light' : 'dark';
  
  // T-554 UX: 平滑过渡逻辑
  document.documentElement.classList.add('theme-transitioning');
  
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      document.documentElement.setAttribute('data-theme', currentTheme.value);
      setTimeout(() => {
        document.documentElement.classList.remove('theme-transitioning');
      }, 850);
    });
  });

  localStorage.setItem('bob-theme', currentTheme.value);
  window.dispatchEvent(new CustomEvent('bob-theme-changed', { detail: currentTheme.value }));
  window.appAPI.setConfig('theme', currentTheme.value);
  if (window.appAPI.updateTheme) {
    window.appAPI.updateTheme(currentTheme.value);
  }
}

const bottomNavItems = computed(() => [
  { id: 'inbox', icon: Inbox, label: t('nav.inbox') },
  { id: 'settings', icon: Settings, label: t('nav.settings') },
]);

const modelInfo = computed(() => {
  if (!currentModel.value) return { name: t('app.not_configured'), logo: null };
  const meta = getModelMeta(currentModel.value);
  if (!meta.name || meta.name.toLowerCase() === currentModel.value.toLowerCase()) {
    // Keep raw name if getModelMeta returns raw id
    return { name: currentModel.value, logo: meta.logo };
  }
  return meta;
});

// 设置抽屉导航菜单项
const settingsNavItems = computed(() => [
  { id: 'model', icon: Brain, label: t('settings.nav_model') || '模型基础设置' },
  { id: 'connections', icon: Plug, label: t('settings.nav_connections') || '连接中心' },
  { id: 'workspace', icon: FolderOpen, label: t('settings.nav_workspace') || '工作区' },
  { id: 'daily_routine', icon: Sunrise, label: t('settings.nav_daily_routine') || '每日工作' },
  { id: 'appearance', icon: Palette, label: t('settings.nav_appearance') || '外观与语言' },
  { id: 'about', icon: Info, label: t('settings.nav_about') || '关于' },
]);


// ── 生命周期 ─────────────────────────────────────────
let unlistenConfigReconciled = null;
let unlistenRemoteMessage = null;

function handleBackButton() {
  if (showDeleteModal.value) {
    cancelDeleteChat();
    return true;
  }
  // TODO: Add other back handlers for chat view, etc.
  return false;
}

onMounted(async () => {
  // ── 响应式布局监听 ──
  window.addEventListener('resize', onResizeHandler);

  if (isMobile.value) {
    // 拦截 Android 物理返回键
    history.pushState(null, '', location.href);
    window.addEventListener('popstate', (e) => {
      history.pushState(null, '', location.href);
      handleBackButton();
    });
  }

  // ── 拦截 F5 / Ctrl+R：桌面应用不需要硬刷新，改为软重载 ──
  document.addEventListener('keydown', async (e) => {
    if (e.key === 'F5' || (e.ctrlKey && e.key === 'r')) {
      e.preventDefault();
      // 软重载：重新拉取对话列表和配置，等同于重新打开应用
      await loadConversations();
      currentModel.value = await window.appAPI.getConfig('model') || '';
      const theme = await window.appAPI.getConfig('theme');
      if (theme) {
        currentTheme.value = theme;
        document.documentElement.setAttribute('data-theme', theme);
      }
      console.log('[F5] 软重载完成');
    }
  });

  // 检查是否已配置
  isSetupComplete.value = await window.appAPI.isSetupComplete();
  if (DEBUG_ONBOARDING === 0) {
    isSetupComplete.value = false;
  }

  if (isSetupComplete.value) {
    await loadConversations();
    currentModel.value = await window.appAPI.getConfig('model') || '';
    // 恢复 UI 缩放偏好和主题和侧边栏宽度
    const savedWidth = await window.appAPI.getConfig('sidebarWidth');
    if (savedWidth) sidebarWidth.value = savedWidth;

    const uiScale = await window.appAPI.getConfig('uiScale');
    if (uiScale) {
      document.documentElement.setAttribute('data-ui-scale', uiScale);
    }
    const theme = await window.appAPI.getConfig('theme');
    if (theme) {
      currentTheme.value = theme;
      document.documentElement.setAttribute('data-theme', theme);
      if (window.appAPI.updateTheme) {
        window.appAPI.updateTheme(theme);
      }
    }
    const accentColor = await window.appAPI.getConfig('accentColor');
    if (accentColor) {
      localStorage.setItem('bob-accent', accentColor);
      document.documentElement.style.setProperty('--user-accent', accentColor);
      const hex = accentColor.replace('#', '');
      const r = parseInt(hex.substring(0, 2), 16);
      const g = parseInt(hex.substring(2, 4), 16);
      const b = parseInt(hex.substring(4, 6), 16);
      document.documentElement.style.setProperty('--user-accent-rgb', `${r}, ${g}, ${b}`);
    }
    // 恢复用户语言偏好
    const savedLang = await window.appAPI.getConfig('language');
    if (savedLang) locale.value = savedLang;
  }

  // 本地存储同步主题，供 index.html 启动瞬间读取
  if (currentTheme.value) localStorage.setItem('bob-theme', currentTheme.value);

  // 显示并聚焦原生窗口，防止藏在后台
  // 注意：不要使用 await，否则任何调用失败（比如窗口并未最小化而引发的异常）都会阻塞后续的开屏动画移除
  window.appAPI.unminimizeWindow().catch(() => {});
  window.appAPI.showWindow().catch(() => {});
  window.appAPI.focusWindow().catch(() => {});

  // 启动画面淡出 — 原生 Splash 渐隐 1 秒
  setTimeout(() => { 
    showSplash.value = false;
    const splash = document.getElementById('native-splash');
    if (splash) {
      splash.style.opacity = '0';
      setTimeout(() => splash.remove(), 1000); // 等待 CSS 1s transition 结束
    }
  }, 1000);

  // ── Outbox Reconciler 事件监听 (T-813) ─────────────
  if (window.appAPI.onConfigReconciled) {
    unlistenConfigReconciled = window.appAPI.onConfigReconciled((payload) => {
      const count = payload?.applied || 0;
      console.log(`[Reconciler] ${count} 条配置已生效，刷新 UI...`);
      // 刷新 ChatView 模型指示器
      if (chatViewRef.value && chatViewRef.value.refreshModel) {
        chatViewRef.value.refreshModel();
      }
      // 重新加载当前模型
      window.appAPI.getConfig('model').then(m => {
        if (m) currentModel.value = m;
      });
    });
  }

  // ── 远程消息通知：微信等通道产生新消息时刷新侧边栏 ──────
  if (window.appAPI.onRemoteNewMessage) {
    unlistenRemoteMessage = await window.appAPI.onRemoteNewMessage((event) => {
      const convId = event?.payload?.conversation_id || event?.conversation_id;
      console.log(`[Remote] 收到远程新消息通知, conv_id=${convId}`);
      // 刷新侧边栏对话列表（新对话出现 / 时间戳更新）
      loadConversations();
    });
  }

  // T-1303: 全局监听 Cron 任务完成事件，更新导航栏红点
  if (window.appAPI.onSchedulerCompleted) {
    unlistenSchedulerGlobal = window.appAPI.onSchedulerCompleted((payload) => {
      console.log('[App] scheduler:completed', payload?.title);
      cronNotifCount.value += 1;
    });
  }
});

onUnmounted(() => {
  window.removeEventListener('resize', onResizeHandler);
  clearTimeout(resizeDebounce);
  if (unlistenConfigReconciled) unlistenConfigReconciled();
  if (unlistenRemoteMessage) unlistenRemoteMessage();
  if (unlistenSchedulerGlobal) unlistenSchedulerGlobal();
  if (searchDebounce) clearTimeout(searchDebounce);
});

// ── 对话管理 ─────────────────────────────────────────
async function loadConversations() {
  conversations.value = await window.appAPI.getConversations();
  // 没有对话就创建一个
  if (conversations.value.length === 0) {
    await createNewChat();
  } else if (!activeConversationId.value) {
    activeConversationId.value = conversations.value[0].id;
  }
}

async function createNewChat() {
  if (activeConversationId.value) {
    window.appAPI.summarizeSession(activeConversationId.value).catch(e => console.error(e));
  }
  const conv = await window.appAPI.createConversation(t('chat.new_conversation'), currentModel.value);
  conversations.value.unshift(conv);
  activeConversationId.value = conv.id;
}

function switchConversation(id) {
  if (activeConversationId.value && activeConversationId.value !== id) {
    // Trigger background summarization for the old conversation
    window.appAPI.summarizeSession(activeConversationId.value).catch(err => {
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
  
  await window.appAPI.deleteConversation(id);
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
    await window.appAPI.renameConversation(conv.id, newTitle);
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
function timeAgo(dateVal) {
  if (!dateVal) return '';
  const now = Date.now();
  // SQLite returns ms timestamp as Number, old configs might return strings
  const then = typeof dateVal === 'number' ? dateVal : new Date(dateVal + (String(dateVal).includes('Z') ? '' : 'Z')).getTime();
  
  if (isNaN(then)) return '';
  
  const diff = Math.max(0, now - then);
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return t('app.just_now');
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d`;
  const months = Math.floor(days / 30);
  return `${months}mo`;
}

// ── 设置 ─────────────────────────────────────────────
async function onSetupComplete(payload) {
  const startRect = payload?.startRect;

  // 优先使用向导传来的值（debug 模式下后端没有保存）
  const theme = payload?.theme || await window.appAPI.getConfig('theme');
  if (theme) {
    currentTheme.value = theme;
    document.documentElement.setAttribute('data-theme', theme);
    localStorage.setItem('bob-theme', theme);
    if (window.appAPI.updateTheme) {
      window.appAPI.updateTheme(theme);
    }
  }
  const accentColor = payload?.accentColor || await window.appAPI.getConfig('accentColor');
  if (accentColor) {
    localStorage.setItem('bob-accent', accentColor);
    document.documentElement.style.setProperty('--user-accent', accentColor);
    const hex = accentColor.replace('#', '');
    const r = parseInt(hex.substring(0, 2), 16);
    const g = parseInt(hex.substring(2, 4), 16);
    const b = parseInt(hex.substring(4, 6), 16);
    document.documentElement.style.setProperty('--user-accent-rgb', `${r}, ${g}, ${b}`);
  }

  // 切换到聊天界面
  isSetupComplete.value = true;
  currentModel.value = await window.appAPI.getConfig('model') || '';
  await loadConversations();

  // 如果有起始坐标，执行精确飞行动画
  if (startRect) {
    await nextTick();
    requestAnimationFrame(() => {
      const target = document.querySelector('.empty-bob-logo');
      if (!target) return;

      const endRect = target.getBoundingClientRect();

      // 隐藏目标 logo，防止飞行过程中出现两个叠影
      target.style.opacity = '0';

      // 创建一个 div 飞行体，用 mask-image 确保颜色跟随主题
      const flyer = document.createElement('div');
      const logoColor = getComputedStyle(document.documentElement).getPropertyValue('--logo-color').trim();
      const DURATION = 1200; // ms
      flyer.style.cssText = `
        position: fixed;
        z-index: 5;
        pointer-events: none;
        left: ${startRect.left}px;
        top: ${startRect.top}px;
        width: ${startRect.width}px;
        height: ${startRect.height}px;
        opacity: 0.25;
        background-color: ${logoColor};
        -webkit-mask-image: url(/bob_bob.svg);
        mask-image: url(/bob_bob.svg);
        -webkit-mask-size: contain;
        mask-size: contain;
        -webkit-mask-repeat: no-repeat;
        mask-repeat: no-repeat;
        -webkit-mask-position: center;
        mask-position: center;
        transition: left ${DURATION}ms cubic-bezier(0.22, 1, 0.36, 1),
                    top ${DURATION}ms cubic-bezier(0.22, 1, 0.36, 1),
                    width ${DURATION}ms cubic-bezier(0.22, 1, 0.36, 1),
                    height ${DURATION}ms cubic-bezier(0.22, 1, 0.36, 1),
                    opacity ${DURATION}ms cubic-bezier(0.22, 1, 0.36, 1);
      `;
      document.body.appendChild(flyer);

      // 强制回流后触发动画
      flyer.offsetHeight;
      flyer.style.left = `${endRect.left}px`;
      flyer.style.top = `${endRect.top}px`;
      flyer.style.width = `${endRect.width}px`;
      flyer.style.height = `${endRect.height}px`;
      flyer.style.opacity = '0.05';

      // 等全部 transition 播完后（用 setTimeout 比 transitionend 更可靠）
      // 先恢复目标 logo，再移除飞行体，保证无缝
      setTimeout(() => {
        target.style.opacity = '';  // 恢复为 CSS 默认的 0.05
        requestAnimationFrame(() => {
          flyer.remove();
        });
      }, DURATION + 50); // +50ms 安全余量
    });
  }
}

async function onConfigChanged() {
  currentModel.value = await window.appAPI.getConfig('model') || '';
}

// T-1301: 搜索逻辑 (300ms debounce)
function onSearchInput() {
  if (searchDebounce) clearTimeout(searchDebounce);
  const q = searchQuery.value.trim();
  if (!q) {
    searchResults.value = [];
    isSearching.value = false;
    return;
  }
  isSearching.value = true;
  searchDebounce = setTimeout(async () => {
    try {
      searchResults.value = await window.appAPI.searchMessages(q);
    } catch (err) {
      console.error('[Search] FTS error:', err);
      searchResults.value = [];
    } finally {
      isSearching.value = false;
    }
  }, 300);
}

function clearSearch() {
  searchQuery.value = '';
  searchResults.value = [];
  isSearching.value = false;
  isSearchExpanded.value = false;
}

async function expandSearch() {
  isSearchExpanded.value = true;
  await nextTick();
  searchInputRef.value?.focus();
}

function collapseSearch() {
  if (!searchQuery.value) {
    isSearchExpanded.value = false;
  }
}

function onSearchBlur() {
  // 延迟收起，给点击搜索结果留出时间
  setTimeout(() => {
    if (!searchQuery.value) {
      isSearchExpanded.value = false;
    }
  }, 200);
}

function jumpToSearchResult(result) {
  // 跳转到搜索结果所在的对话
  activeConversationId.value = result.conversation_id;
  currentView.value = 'chat';
  clearSearch();
}

// T-1303: 点击导航时清除通知计数
function onNavClick(viewId) {
  currentView.value = viewId;
  if (viewId === 'inbox') {
    cronNotifCount.value = 0;
  }
}

// ── Mobile FAB (灵光一现悬浮球) 逻辑 ──
const fabX = ref(null);
const fabY = ref(null);
const isFabDragging = ref(false);
const fabIsIdle = ref(true);
let fabTimer = null;
let dragStartX = 0;
let dragStartY = 0;
let initialFabX = 0;
let initialFabY = 0;
let isMoved = false;

const fabStyle = computed(() => {
  if (fabX.value === null || fabY.value === null) return {};
  return {
    left: `${fabX.value}px`,
    top: `${fabY.value}px`,
    right: 'auto',
    bottom: 'auto'
  };
});

function onFabPointerDown(e) {
  dragStartX = e.clientX;
  dragStartY = e.clientY;
  isMoved = false;
  
  const rect = e.currentTarget.getBoundingClientRect();
  initialFabX = rect.left;
  initialFabY = rect.top;

  fabIsIdle.value = false;
  clearTimeout(fabTimer);
  
  // Capture pointer to track outside the button
  e.currentTarget.setPointerCapture(e.pointerId);
}

function onFabPointerMove(e) {
  if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
  
  const deltaX = e.clientX - dragStartX;
  const deltaY = e.clientY - dragStartY;

  // 手机触控天然抖动大，阈值设为 15px 防止误判为拖拽
  const dragThreshold = 15;
  if (Math.abs(deltaX) > dragThreshold || Math.abs(deltaY) > dragThreshold) {
    isMoved = true;
    isFabDragging.value = true;
  }

  if (isMoved) {
    fabX.value = initialFabX + deltaX;
    fabY.value = initialFabY + deltaY;
    
    // 边界检测
    const maxW = window.innerWidth - 48; // fab 宽 48
    const maxH = window.innerHeight - 48;
    if (fabX.value < 0) fabX.value = 0;
    if (fabX.value > maxW) fabX.value = maxW;
    if (fabY.value < 0) fabY.value = 0;
    if (fabY.value > maxH) fabY.value = maxH;
  }
}

function onFabPointerUp(e) {
  if (e.currentTarget.hasPointerCapture(e.pointerId)) {
    e.currentTarget.releasePointerCapture(e.pointerId);
  }
  
  isFabDragging.value = false;
  if (!isMoved) {
    e.preventDefault(); // 阻断浏览器合成 click 穿透
    // 纯点击，唤起速记面板
    openQuickNote();
  } else {
    // 拖拽结束，自动吸附到屏幕边缘
    const center = window.innerWidth / 2;
    if (fabX.value < center) {
      fabX.value = 16;
    } else {
      fabX.value = window.innerWidth - 48 - 16;
    }
  }

  // 重置闲置计时器 (1秒后恢复半显)
  fabTimer = setTimeout(() => {
    fabIsIdle.value = true;
  }, 1000);
}

</script>

<style scoped>
.view-wrapper {
  flex: 1;
  min-width: 0;
  height: 100%;
  display: flex;
  flex-direction: column;
}

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
  padding: 0 0 0 var(--space-2);
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
  cursor: pointer;
  transition: opacity 0.15s;
}
.app-logo:hover {
  opacity: 0.6;
}

.titlebar-theme-btn {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all var(--duration-fast);
}

.titlebar-theme-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
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
  background: var(--user-accent, var(--accent-primary));
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

/* ── 侧栏顶部工具栏 ──────────────────────────────── */
.sidebar-top {
  padding: var(--space-2) var(--space-3);
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 6px;
}

.sidebar-icon-btn {
  width: 34px;
  height: 34px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-subtle);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all var(--duration-fast);
}

.sidebar-icon-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

/* 新对话按钮：默认长条，展开搜索时缩为图标 */
.new-chat-btn {
  flex: 1;
  height: 34px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border-radius: var(--radius-md);
  border: 1px dashed var(--border-subtle);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: var(--text-sm);
  font-family: var(--font-sans);
  transition: all var(--duration-fast) var(--ease-out);
  overflow: hidden;
  white-space: nowrap;
}

.new-chat-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.new-chat-btn.sr-compact {
  flex: 0 0 34px;
  width: 34px;
  border-style: solid;
}

.new-chat-btn.sr-compact .new-chat-label {
  display: none;
}

/* ── 搜索框（可展开） ─────────────────────────────── */
.sidebar-search {
  display: flex;
  align-items: center;
  transition: all var(--duration-fast) var(--ease-out);
}

.sidebar-search.expanded {
  flex: 1;
  min-width: 0;
  gap: 6px;
  padding: 0 10px;
  height: 34px;
  background: var(--surface-input, var(--bg-tertiary));
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
}

.sidebar-search.expanded:focus-within {
  border-color: var(--user-accent, var(--accent-primary));
}

.search-icon {
  color: var(--text-muted);
  flex-shrink: 0;
}

.search-input {
  flex: 1;
  background: transparent;
  border: none;
  outline: none;
  color: var(--text-primary);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  min-width: 0;
}

.search-input::placeholder {
  color: var(--text-muted);
}

.search-clear {
  width: 18px;
  height: 18px;
  padding: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
  border-radius: var(--radius-sm);
  flex-shrink: 0;
}

.search-clear:hover {
  color: var(--text-primary);
  background: var(--bg-hover);
}

/* ── T-1301: 搜索结果 ───────────────────────────────── */
.search-results {
  flex: 1;
  overflow-y: auto;
  padding: 0 var(--space-3);
}

.search-results-header {
  font-size: 11px;
  color: var(--text-muted);
  padding: var(--space-1) var(--space-2);
  margin-bottom: var(--space-1);
}

.search-result-item {
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background var(--duration-fast);
  margin-bottom: 2px;
}

.search-result-item:hover {
  background: var(--surface-glass);
}

.search-result-title {
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--text-primary);
  margin-bottom: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.search-result-snippet {
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.4;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.search-result-snippet :deep(mark) {
  background: color-mix(in srgb, var(--user-accent, var(--accent-primary)) 25%, transparent);
  color: var(--accent-tertiary);
  border-radius: 2px;
  padding: 0 1px;
}

.search-result-time {
  font-size: 11px;
  color: var(--text-muted);
  margin-top: 2px;
}

.search-empty {
  text-align: center;
  padding: var(--space-6) var(--space-3);
  color: var(--text-muted);
  font-size: var(--text-sm);
}

/* ── T-1303: 导航通知红点 ────────────────────────────── */
.nav-icon-wrapper {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
}

.nav-badge {
  position: absolute;
  top: -6px;
  right: -8px;
  min-width: 14px;
  height: 14px;
  line-height: 14px;
  font-size: 9px;
  font-weight: 600;
  text-align: center;
  background: var(--error, #e74c3c);
  color: var(--text-inverse);
  border-radius: 7px;
  padding: 0 3px;
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
  color: var(--user-accent, var(--accent-primary));
}

/* ── 对话重命名 ─────────────────────────────────────── */
.rename-input {
  flex: 1;
  background: var(--bg-tertiary);
  border: 1px solid var(--user-accent, var(--accent-primary));
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
  display: flex;
  flex-direction: column;
}
.app-shell.is-mobile .content {
  padding-bottom: calc(60px + env(safe-area-inset-bottom, 0px));
}
@media (max-aspect-ratio: 1/1) {
  .content > * {
    flex: 1;
    min-height: 0;
    height: auto !important;
  }
  .content > .mobile-tab-grid {
    flex: none !important;
    height: auto !important;
  }
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

/* ── 手风琴抽屉 ─────────────────────────────────────── */
.drawer-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-2) var(--space-3);
  cursor: pointer;
  color: var(--text-tertiary);
  font-size: var(--text-sm);
  font-family: var(--font-sans);
  border-top: 1px solid var(--border-subtle);
  flex-shrink: 0;
  transition: all var(--duration-fast) var(--ease-out);
  user-select: none;
}

.drawer-header:first-child {
  border-top: none;
}

.drawer-header:hover {
  color: var(--text-primary);
  background: var(--surface-glass);
}

.drawer-header.active {
  color: var(--text-primary);
  background: var(--bg-secondary);
  font-weight: 500;
}

.drawer-header-left {
  display: flex;
  align-items: center;
  gap: 8px;
}

.drawer-chevron {
  opacity: 0.4;
  transition: transform var(--duration-fast);
}

.drawer-badge {
  min-width: 16px;
  height: 16px;
  line-height: 16px;
  font-size: 10px;
  font-weight: 600;
  text-align: center;
  background: var(--color-error);
  color: var(--bg-root);
  border-radius: 8px;
  padding: 0 4px;
}

.drawer-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-height: 0;
}

.drawer-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: var(--space-6) var(--space-3);
  color: var(--text-muted);
  font-size: var(--text-sm);
}

/* ── 设置导航菜单 ─────────────────────────────────── */
.settings-nav {
  display: flex;
  flex-direction: column;
  padding: var(--space-2) var(--space-3);
  gap: 1px;
}

.settings-nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
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

.settings-nav-item:hover {
  background: var(--surface-glass);
  color: var(--text-primary);
}

.settings-nav-item.active {
  background: var(--gradient-subtle);
  color: var(--accent-tertiary);
  font-weight: 500;
}

.settings-nav-item.active svg {
  color: var(--user-accent, var(--accent-primary));
}
/* ── Mobile Drawer ────────────────────────────────────────── */
.mobile-drawer-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  z-index: 199;
}

.mobile-drawer {
  position: fixed;
  top: 0;
  left: -280px;
  width: 280px;
  height: 100%;
  z-index: 200;
  transition: transform 0.3s cubic-bezier(0.16, 1, 0.3, 1);
  box-shadow: 2px 0 12px rgba(0,0,0,0.2);
}

.mobile-drawer-open {
  transform: translateX(280px);
}

/* 隐藏手机侧边栏中的部分多余信息（如大号 logo） */
.app-shell.is-mobile .sidebar-top .app-logo {
  display: none;
}
.app-shell.is-mobile .titlebar-theme-btn {
  display: none;
}

/* ── Mobile UX: Global Safe Area & Readability ── */
.app-shell.is-mobile {
  padding-top: env(safe-area-inset-top, 0px);
  position: relative;
}

.app-shell.is-mobile::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: env(safe-area-inset-top, 0px);
  background-color: #000000;
  z-index: 9999;
}

.app-shell.is-mobile .sidebar-top {
  padding: 12px 16px;
}

.app-shell.is-mobile .conversation-item {
  padding: 12px 16px;
  gap: 12px;
}

.app-shell.is-mobile .conv-title {
  font-size: 15px;
  font-weight: 500;
}

.app-shell.is-mobile .conv-row-2 {
  font-size: 13px;
  line-height: 1.4;
}

.app-shell.is-mobile .conv-time {
  font-size: 12px;
}

/* ── Mobile FAB (悬浮气泡) ── */
.mobile-fab {
  position: fixed;
  z-index: 9999;
  width: 48px;
  height: 48px;
  border-radius: 50%;
  background: var(--surface-glass);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border: 1px solid var(--border-subtle);
  color: var(--user-accent, var(--text-primary));
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 4px 16px rgba(0,0,0,0.15);
  transition: opacity 0.3s ease, transform 0.15s cubic-bezier(0.34, 1.56, 0.64, 1), background 0.3s ease;
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
  -webkit-touch-callout: none;
  -webkit-user-select: none;
  user-select: none;
  outline: none;
  
  /* 默认位置 (初始) */
  right: 16px;
  bottom: calc(170px + env(safe-area-inset-bottom, 0px));
  touch-action: none;
}

.mobile-fab svg {
  width: 24px;
  height: auto;
  pointer-events: none;
}

.mobile-fab.is-idle {
  opacity: 0.5;
}

.mobile-fab:hover {
  opacity: 1 !important;
}

.mobile-fab:not(.is-idle) {
  opacity: 1;
}

.mobile-fab.is-dragging {
  transform: scale(1.1);
  transition: transform 0.1s ease, opacity 0.1s ease;
  /* 取消 transition left/top 防止拖拽卡顿 */
}

/* 释放后自动吸附使用 transition */
.mobile-fab:not(.is-dragging) {
  transition: opacity 0.3s ease, transform 0.15s ease, left 0.3s ease, top 0.3s ease;
}

.mobile-fab:active {
  background: var(--surface-card);
  transform: scale(0.92);
}
</style>
