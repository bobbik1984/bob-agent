<script setup>
import { onMounted, onUnmounted, provide } from 'vue';
import { useI18n } from 'vue-i18n';
import ScheduleView from './views/ScheduleView.vue';
import WorkView from './views/WorkView.vue';
import KnowledgeView from './views/KnowledgeView.vue';
import SettingsView from './views/SettingsView.vue';
import NavigationDrawer from './components/NavigationDrawer.vue';
import PrimaryChatPane from './components/PrimaryChatPane.vue';
import QuickNoteOverlay from './components/QuickNoteOverlay.vue';
import { bindViewport } from './viewport.mjs';
import { getProviderLogo } from './layout-mode.mjs';
import brandLogo from './assets/bob-logo.svg';
import { canCancelExecution } from './execution-controller.mjs';
import { useChatSession } from './composables/useChatSession.mjs';
import { useOnlineWorkNode } from './composables/useOnlineWorkNode.mjs';
import { useModelHub } from './composables/useModelHub.mjs';
import { usePersonalOps } from './composables/usePersonalOps.mjs';
import { useQuickNote } from './composables/useQuickNote.mjs';

const { t, locale } = useI18n();

const shell = {
  t,
  locale,
  brandLogo,
  getProviderLogo,
  canCancelExecution,
};

const {
  available,
  kernelBridge,
  sessionId,
  catalog,
  remoteConversation,
  layoutState,
  updateLayoutState,
  readiness,
  homeFault,
  devicePaired,
  deviceOnline,
  connectionTarget,
  connectionLabel,
  activeDrawer,
  settingsOpen,
  immersive,
  immersiveActive,
  settingsView,
  settingsTab,
  navigationOpen,
  deviceCapabilities,
  currentConversation,
  totalUnreadCount,
  projectList,
  switchPrimaryDrawer,
  openDevices,
  startup,
  resetSyncBackoffAndReload,
  startPolling,
  disposeChatSession,
} = useChatSession(shell);

const {
  onlineState,
  handleOnlineUpdateOrigin,
  handleOnlineSaveNode,
  handleOnlineBeginPairing,
  handleOnlineExchangePairing,
  handleOnlineRefresh,
  handleOnlineSelectProject,
  handleOnlineLoadMoreMessages,
  handleOnlineSubmitOperation,
  handleOnlineRevoke,
  handleOnlineDisconnect,
  handleWorkCreateProject,
  handleWorkCreateConversation,
  handleWorkOpenConversation,
  handleWorkOpenSettingsTab,
} = useOnlineWorkNode(shell);

const {
  modelHub,
  activeMainRole,
  quickSwitcherOpen,
  quickSwitcherSearchQuery,
  quickPopoverRef,
  quickPillRef,
} = useModelHub(shell);

const {
  personalOps,
  personalBusy,
  personalFault,
  recognizingTicket,
  saveCalendarItem,
  handleCreateSchedule,
  handleCreateProjectTodo,
  handleStartTodoSession,
  handleOpenProjectTodo,
  handlePromoteTodoToProject,
  handleUpdateScheduleStatus,
  handleSaveNote,
  handleDeleteSchedule,
  handleDeleteNote,
  handleSaveTicket,
  handleDeleteTicket,
  handleRecognizeTicketFile,
  handleCreateCapture,
  handleUpdateMemory,
  handleCreateDecision,
  handleUpdateDecision,
  promoteConversationToWork,
} = usePersonalOps(shell);

const {
  quickNoteOpen,
  openQuickNoteOverlay,
} = useQuickNote(shell);

provide('bobAppShell', shell);

let releaseViewport = () => {};

function handleGlobalPointerDown(event) {
  if (!quickSwitcherOpen.value) return;
  const target = event.target;
  if (
    quickPopoverRef.value?.contains(target) ||
    quickPillRef.value?.contains(target)
  ) {
    return;
  }
  quickSwitcherOpen.value = false;
  quickSwitcherSearchQuery.value = '';
}

function handleGlobalKeydown(event) {
  if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === 'n') {
    event.preventDefault();
    if (quickNoteOpen.value) {
      quickNoteOpen.value = false;
    } else {
      void openQuickNoteOverlay();
    }
    return;
  }
  if (event.key === 'Escape' && quickNoteOpen.value) {
    event.preventDefault();
    quickNoteOpen.value = false;
    return;
  }
  if (event.key === 'Escape' && quickSwitcherOpen.value) {
    event.preventDefault();
    quickSwitcherOpen.value = false;
    quickSwitcherSearchQuery.value = '';
  }
}

function handleVisibilityChange() {
  if (typeof document !== 'undefined' && !document.hidden) {
    resetSyncBackoffAndReload();
  }
}

function dismissConversationMenu(event) {
  const menu = document.querySelector('.conversation-menu[open]');
  if (menu && !menu.contains(event.target)) menu.open = false;
}

onMounted(async () => {
  releaseViewport = bindViewport(window, document.documentElement);
  updateLayoutState();
  window.addEventListener('resize', updateLayoutState, { passive: true });
  document.addEventListener('pointerdown', handleGlobalPointerDown, true);
  document.addEventListener('pointerdown', dismissConversationMenu);
  document.addEventListener('keydown', handleGlobalKeydown);
  document.addEventListener('visibilitychange', handleVisibilityChange);
  await startup();
  if (available) {
    deviceCapabilities.value = await kernelBridge.pairingCapabilities().catch(() => null);
  }
  startPolling();
});

onUnmounted(() => {
  disposeChatSession();
  if (typeof window !== 'undefined') {
    window.removeEventListener('resize', updateLayoutState);
  }
  if (typeof document !== 'undefined') {
    document.removeEventListener('pointerdown', handleGlobalPointerDown, true);
    document.removeEventListener('pointerdown', dismissConversationMenu);
    document.removeEventListener('keydown', handleGlobalKeydown);
    document.removeEventListener('visibilitychange', handleVisibilityChange);
  }
  releaseViewport();
});
</script>

<template>
  <div
    class="app-shell"
    :class="[
      `layout-${layoutState.layoutMode}`,
      `terminal-${layoutState.terminalKind}`,
      `aspect-${layoutState.aspect}`,
      { 'is-mobile': layoutState.isCompact, 'is-immersive': immersiveActive },
    ]"
  >
    <header class="topbar">
      <button
        type="button"
        class="icon-button"
        :aria-label="t('openNavigation')"
        :aria-expanded="navigationOpen"
        @click="navigationOpen = true"
      >
        <svg class="topbar-svg" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 6h16M4 12h16M4 18h16" />
        </svg>
        <span v-if="totalUnreadCount > 0" class="unread-dot" aria-hidden="true"></span>
      </button>
      <span class="page-name">
        {{
          settingsOpen || activeDrawer === 'settings'
            ? t(
                settingsView === 'provider'
                  ? 'directModelProvider'
                  : settingsTab === 'connection'
                    ? 'deviceAndConnection'
                    : 'settings',
              )
            : activeDrawer === 'schedule'
              ? t('scheduleViewTitle')
              : activeDrawer === 'work'
                ? t('workViewTitle')
                : activeDrawer === 'knowledge'
                  ? t('knowledgeViewTitle')
                  : (remoteConversation?.session.title || currentConversation)
        }}
      </span>
      <button
        v-if="activeDrawer === 'chat' && !settingsOpen && !remoteConversation"
        type="button"
        class="mini-ghost-btn"
        :title="t('promoteToWorkBtn')"
        @click="promoteConversationToWork"
      >
        {{ t('promoteToWorkBtn') }}
      </button>
      <button
        type="button"
        class="device-indicator"
        :class="{
          paired: devicePaired,
          online: deviceOnline,
          unconfirmed: homeFault || (devicePaired && !deviceOnline),
        }"
        :aria-label="connectionLabel"
        :title="connectionLabel"
        @click="openDevices"
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <template v-if="connectionTarget.kind === 'server'">
            <rect x="3" y="3" width="18" height="7" rx="2" /><rect x="3" y="14" width="18" height="7" rx="2" /><path d="M6 6.5h2M6 17.5h2" />
          </template>
          <template v-else-if="connectionTarget.kind === 'home'">
            <path d="m3 10 9-7 9 7M5 9v12h14V9M9 21v-7h6v7" />
          </template>
          <template v-else><circle cx="12" cy="12" r="8" /><path d="M8 12h8M12 8v8" /></template>
        </svg>
        <span class="device-dot" aria-hidden="true"></span>
      </button>
      <details class="conversation-menu" @keydown.esc="$event.currentTarget.open=false; $event.currentTarget.querySelector('summary').focus()" @focusout="!$event.currentTarget.contains($event.relatedTarget) && ($event.currentTarget.open=false)">
        <summary :aria-label="t('conversationMenu')"><svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true"><circle cx="5" cy="12" r="1.5" fill="currentColor"/><circle cx="12" cy="12" r="1.5" fill="currentColor"/><circle cx="19" cy="12" r="1.5" fill="currentColor"/></svg></summary>
        <div class="conversation-menu-panel" @click="$event.currentTarget.closest('details').open = false">
          <button @click="immersive = !immersive"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 3H3v5m13-5h5v5M3 16v5h5m13-5v5h-5"/></svg>{{ t(immersive ? 'exitImmersive' : 'enterImmersive') }}</button>
          <button @click="settingsOpen = true"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M4 17h16"/><circle cx="9" cy="7" r="3"/><circle cx="15" cy="17" r="3"/></svg>{{ t('settings') }}</button>
        </div>
      </details>
    </header>

    <SettingsView />

    <NavigationDrawer />

    <div v-if="personalFault" class="notice" role="alert">
      <span>{{ personalFault }}</span>
      <button type="button" @click="personalFault = null">×</button>
    </div>

    <!-- Primary View 2: 日程 (Schedule / InboxView) -->
    <ScheduleView
      v-if="activeDrawer === 'schedule'"
      :schedules="personalOps.schedules"
      :notes="personalOps.notes"
      :recognizing-ticket="recognizingTicket"
      :save-item="saveCalendarItem"
      :projects="projectList"
      :online-projects="onlineState.projects || []"
      :busy="personalBusy"
      @create-schedule="handleCreateSchedule"
      @update-status="handleUpdateScheduleStatus"
      @delete-schedule="handleDeleteSchedule"
      @open-project-todo="handleOpenProjectTodo"
      @promote-todo-project="handlePromoteTodoToProject"
      @save-ticket="handleSaveTicket"
      @delete-ticket="handleDeleteTicket"
      @recognize-ticket-file="handleRecognizeTicketFile"
    />

    <!-- Primary View 3: 工作 (Work / RemoteAgent & Project Matrix) -->
    <WorkView
      v-if="activeDrawer === 'work'"
      :readiness="readiness"
      :catalog="catalog"
      :schedules="personalOps.schedules"
      :active-session-id="sessionId"
      :active-main-role="activeMainRole"
      :online-state="onlineState"
      :remote-nodes="modelHub.state.remote_nodes || []"
      :paired-online="deviceOnline"
      @open-conversation="handleWorkOpenConversation"
      @create-project="handleWorkCreateProject"
      @create-conversation="handleWorkCreateConversation"
      @open-settings-tab="handleWorkOpenSettingsTab"
      @create-project-todo="handleCreateProjectTodo"
      @toggle-todo-status="handleUpdateScheduleStatus"
      @start-todo-session="handleStartTodoSession"
      @online-update-origin="handleOnlineUpdateOrigin"
      @online-save-node="handleOnlineSaveNode"
      @online-begin-pairing="handleOnlineBeginPairing"
      @online-exchange-pairing="handleOnlineExchangePairing"
      @online-refresh="handleOnlineRefresh"
      @online-select-project="handleOnlineSelectProject"
      @online-load-more-messages="handleOnlineLoadMoreMessages"
      @online-submit-operation="handleOnlineSubmitOperation"
      @online-disconnect="handleOnlineDisconnect"
      @online-revoke="handleOnlineRevoke"
    />

    <!-- Primary View 4: 知识 (Knowledge / Notes, Captures & Long-term Memory) -->
    <KnowledgeView
      v-if="activeDrawer === 'knowledge'"
      :notes="personalOps.notes"
      :captures="personalOps.captures"
      :memories="personalOps.memories"
      :decisions="personalOps.decisions"
      :busy="personalBusy"
      :recognizing-ticket="recognizingTicket"
      @save-note="handleSaveNote"
      @delete-note="handleDeleteNote"
      @save-ticket="handleSaveTicket"
      @delete-ticket="handleDeleteTicket"
      @recognize-ticket-file="handleRecognizeTicketFile"
      @create-capture="handleCreateCapture"
      @update-memory="handleUpdateMemory"
      @create-decision="handleCreateDecision"
      @update-decision="handleUpdateDecision"
    />


    <PrimaryChatPane />

    <QuickNoteOverlay />

    <!-- Legacy-aligned Bottom 5-Tab Navigation Bar: 对话 | 日程 | 工作 | 知识 | 设置 -->
    <nav class="bottom-nav" role="navigation">
      <button
        type="button"
        class="bottom-nav-item"
        :class="{ active: activeDrawer === 'chat' && !settingsOpen }"
        @click="switchPrimaryDrawer('chat')"
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
        </svg>
        <span>{{ t('navChat') }}</span>
      </button>
      <button
        type="button"
        class="bottom-nav-item"
        :class="{ active: activeDrawer === 'schedule' && !settingsOpen }"
        @click="switchPrimaryDrawer('schedule')"
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <rect x="3" y="4" width="18" height="18" rx="2" />
          <path d="M16 2v4M8 2v4M3 10h18" />
        </svg>
        <span>{{ t('navSchedule') }}</span>
      </button>
      <button
        type="button"
        class="bottom-nav-item"
        :class="{ active: activeDrawer === 'knowledge' && !settingsOpen }"
        @click="switchPrimaryDrawer('knowledge')"
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="18" cy="5" r="3" />
          <circle cx="6" cy="12" r="3" />
          <circle cx="18" cy="19" r="3" />
          <path d="m8.59 13.51 6.83 3.98M15.41 6.51l-6.82 3.98" />
        </svg>
        <span>{{ t('navKnowledge') }}</span>
      </button>
      <button
        type="button"
        class="bottom-nav-item"
        :aria-label="t('settings')"
        :class="{ active: settingsOpen || activeDrawer === 'settings' }"
        @click="switchPrimaryDrawer('settings')"
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="12" cy="12" r="3" />
          <path
            d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9c.26.6.77 1.02 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"
          />
        </svg>
        <span>{{ t('navSettings') }}</span>
      </button>
    </nav>
  </div>
</template>
