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
import InboxView from './views/InboxView.vue';
import DesktopRightSidebar from './components/DesktopRightSidebar.vue';
import { useInbox } from './composables/useInbox.mjs';
import { useJarvisHome } from './composables/useJarvisHome.mjs';

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
  assistantConversationId,
  messages,
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
  deviceSyncPhase,
  forceRefreshActiveConversation,
  startup,
  resetSyncBackoffAndReload,
  startPolling,
  disposeChatSession,
  organizeSessionsViaAgent,
} = useChatSession(shell);
shell.organizeSessionsViaAgent = organizeSessionsViaAgent;
shell.catalog = catalog;
shell.projectList = projectList;
shell.assistantConversationId = assistantConversationId;
shell.messages = messages;

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
  subscriptionQuotas,
  loadModelHub,
  prepaidUsageList,
  showPrepaidCost,
  togglePrepaidCost,
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

const inbox = useInbox(shell);
shell.inbox = inbox;

const jarvisHome = useJarvisHome(shell);
shell.jarvisHome = jarvisHome;

provide('bobAppShell', shell);

let releaseViewport = () => {};

function handleGlobalPointerDown(event) {
  if (!quickSwitcherOpen.value) return;
  const target = event.target;
  if (quickPopoverRef.value?.contains(target) || quickPillRef.value?.contains(target)) return;
  quickSwitcherOpen.value = false;
  quickSwitcherSearchQuery.value = '';
}

function handleGlobalKeydown(event) {
  if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLowerCase() === 'n') {
    event.preventDefault();
    if (quickNoteOpen.value) quickNoteOpen.value = false;
    else void openQuickNoteOverlay();
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
  if (typeof document !== 'undefined' && !document.hidden) resetSyncBackoffAndReload();
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
  if (typeof window !== 'undefined') window.removeEventListener('resize', updateLayoutState);
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
      <button type="button" class="icon-button topbar-nav-toggle" :aria-label="t('openNavigation')" :aria-expanded="navigationOpen" @click="navigationOpen = true">
        <svg class="topbar-svg" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6h16M4 12h16M4 18h16" /></svg>
        <span v-if="totalUnreadCount > 0" class="unread-dot" aria-hidden="true"></span>
      </button>
      <span class="page-name">
        {{
          settingsOpen || activeDrawer === 'settings'
            ? t(settingsView === 'provider' ? 'directModelProvider' : settingsTab === 'connection' ? 'deviceAndConnection' : 'settings')
            : activeDrawer === 'schedule'
              ? t('scheduleViewTitle')
              : activeDrawer === 'inbox'
                ? t('inboxTitle')
              : activeDrawer === 'work'
                ? t('workViewTitle')
                : activeDrawer === 'knowledge'
                  ? t('knowledgeViewTitle')
                  : (remoteConversation?.session.title || currentConversation)
        }}
      </span>
      <button v-if="activeDrawer === 'chat' && !settingsOpen && !remoteConversation && (messages?.length > 0)" type="button" class="mini-ghost-btn" :title="t('promoteToWorkBtn')" @click="promoteConversationToWork">
        {{ t('promoteToWorkBtn') }}
      </button>
      <button
        type="button"
        class="device-indicator"
        :class="{
          paired: devicePaired || !!remoteConversation,
          online: (deviceOnline || !!remoteConversation) && deviceSyncPhase !== 'offline',
          checking: deviceSyncPhase === 'checking',
          syncing: deviceSyncPhase === 'syncing',
          synced: deviceSyncPhase === 'synced',
          unconfirmed: deviceSyncPhase === 'offline' || homeFault || (devicePaired && !deviceOnline),
        }"
        :aria-label="`${connectionLabel} · ${t('forceRefreshBtn')}`"
        :title="`${connectionLabel} · ${t('forceRefreshBtn')}`"
        @click="forceRefreshActiveConversation"
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
      <details class="conversation-menu" @toggle="$event.currentTarget.open && (loadModelHub(), jarvisHome?.refreshJarvisState(sessionId))" @keydown.esc="$event.currentTarget.open=false; $event.currentTarget.querySelector('summary').focus()" @focusout="$event.relatedTarget && !$event.currentTarget.contains($event.relatedTarget) && ($event.currentTarget.open=false)">
        <summary :aria-label="t('conversationMenu')"><svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true"><circle cx="5" cy="12" r="1.5" fill="currentColor"/><circle cx="12" cy="12" r="1.5" fill="currentColor"/><circle cx="19" cy="12" r="1.5" fill="currentColor"/></svg></summary>
        <div class="conversation-menu-panel">
          <button type="button" class="conversation-menu-action" @click="immersive = !immersive; $event.currentTarget.closest('details.conversation-menu').open = false"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 3H3v5m13-5h5v5M3 16v5h5m13-5v5h-5"/></svg>{{ t(immersive ? 'exitImmersive' : 'enterImmersive') }}</button>
          <button type="button" class="conversation-menu-action" @click="settingsOpen = true; $event.currentTarget.closest('details.conversation-menu').open = false"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M4 17h16"/><circle cx="9" cy="7" r="3"/><circle cx="15" cy="17" r="3"/></svg>{{ t('settings') }}</button>
          <div class="conversation-menu-divider" role="separator"></div>
          <div class="quota-sub-section" :aria-label="t('quotaSubscription')">
            <div v-for="item in subscriptionQuotas" :key="item.id" class="quota-sub-row"
              :title="[item.label, item.compact?.id, item.source && t('quotaSource', {node:item.source}), item.updated && t('quotaUpdatedAt', {time:item.updated}), item.status === 'stale' && t('quotaStale'), !item.compact && t('quotaAwaitingSource')].filter(Boolean).join(' · ')">
              <span class="quota-brand-badge"><img :src="item.logo" :alt="item.label" class="quota-brand-logo" /></span>
              <div class="quota-dual-bars">
                <div class="quota-bar-line">
                  <div class="quota-bar-track" role="meter" :aria-label="item.label + ' ' + t('quotaSubscription')" :aria-valuenow="item.compact && !item.compact.expired ? item.compact.remaining_percent : undefined" :aria-valuetext="!item.compact ? t('quotaAwaitingSource') : item.compact.expired ? t('quotaWindowExpired') : item.status === 'stale' ? t('quotaStale') : undefined" aria-valuemin="0" aria-valuemax="100">
                    <div class="quota-bar-fill quota-remaining-fill" :style="{'--remaining': item.compact && !item.compact.expired ? item.compact.remaining_percent + '%' : '0%'}"></div>
                  </div>
                  <span class="quota-bar-pct">{{ item.compact && !item.compact.expired ? Math.round(item.compact.remaining_percent) + '%' : '—' }}<span v-if="item.status === 'stale'" :aria-label="t('quotaStale')">*</span></span>
                </div>
                <div class="quota-bar-line">
                  <div class="quota-bar-track" role="meter" :aria-label="item.label + ' ' + t('quotaResetTime')" :aria-valuenow="item.compact?.timePercent ?? undefined" :aria-valuetext="item.compact?.timeText || t('quotaAwaitingSource')" aria-valuemin="0" aria-valuemax="100">
                    <div class="quota-bar-fill quota-time-fill" :style="{width: (item.compact?.timePercent ?? 0) + '%'}"></div>
                  </div>
                  <span class="quota-bar-pct quota-time-text">{{ item.compact?.timeText || '—' }}</span>
                </div>
              </div>
            </div>
          </div>

        </div>
      </details>
    </header>

    <div class="app-body-container">
      <!-- Desktop Left Rail (Screen Desktop A/B) -->
      <nav v-if="!layoutState.isCompact" class="desktop-rail" role="navigation" aria-label="Desktop Rail">
        <button type="button" class="rail-item" :class="{ on: activeDrawer === 'chat' && !settingsOpen }" :title="t('navChat')" @click="switchPrimaryDrawer('chat')">
          <svg class="rail-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" /></svg>
          <span>{{ t('navChat') }}</span>
        </button>
        <button type="button" class="rail-item" :class="{ on: activeDrawer === 'schedule' && !settingsOpen }" :title="t('navSchedule')" @click="switchPrimaryDrawer('schedule')">
          <svg class="rail-icon" viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="18" rx="2" /><path d="M16 2v4M8 2v4M3 10h18" /></svg>
          <span>{{ t('navSchedule') }}</span>
        </button>
        <button type="button" class="rail-item" :class="{ on: activeDrawer === 'inbox' && !settingsOpen }" :title="t('inboxTitle')" @click="switchPrimaryDrawer('inbox')">
          <svg class="rail-icon" viewBox="0 0 24 24" aria-hidden="true"><polyline points="22 12 16 12 14 15 10 15 8 12 2 12" /><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" /></svg>
          <span v-if="(inbox?.unsortedCount?.value || inbox?.unsortedCount || 0) > 0" class="dot-badge">{{ inbox?.unsortedCount?.value ?? inbox?.unsortedCount ?? 0 }}</span>
          <span>{{ t('inboxTitle') }}</span>
        </button>
        <button type="button" class="rail-item" :class="{ on: activeDrawer === 'knowledge' && !settingsOpen }" :title="t('navKnowledge')" @click="switchPrimaryDrawer('knowledge')">
          <svg class="rail-icon" viewBox="0 0 24 24" aria-hidden="true"><circle cx="18" cy="5" r="3" /><circle cx="6" cy="12" r="3" /><circle cx="18" cy="19" r="3" /><path d="m8.59 13.51 6.83 3.98M15.41 6.51l-6.82 3.98" /></svg>
          <span>{{ t('navKnowledge') }}</span>
        </button>
        <span class="rail-spacer"></span>
        <button type="button" class="rail-item" :class="{ on: settingsOpen || activeDrawer === 'settings' }" :title="t('navSettings')" @click="switchPrimaryDrawer('settings')">
          <svg class="rail-icon" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9c.26.6.77 1.02 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" /></svg>
          <span>{{ t('navSettings') }}</span>
        </button>
      </nav>

      <!-- Desktop Docked Conversation Sidebar in Chat Tab -->
      <aside v-if="!layoutState.isCompact && activeDrawer === 'chat' && !settingsOpen" class="desktop-docked-sidebar" :aria-label="t('conversations')">
        <NavigationDrawer :docked="true" />
      </aside>

      <!-- Main Content Views -->
      <div class="app-content-area">
        <div v-if="personalFault" class="notice" role="alert">
          <span>{{ personalFault }}</span>
          <button type="button" @click="personalFault = null">×</button>
        </div>

        <SettingsView />

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

        <InboxView v-if="activeDrawer === 'inbox'" />

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
      </div>

      <DesktopRightSidebar
        v-if="!layoutState.isCompact && activeDrawer === 'chat' && !settingsOpen"
        :schedules="personalOps.schedules"
        :inbox="inbox"
        @navigate="switchPrimaryDrawer"
        @open-quick-note="openQuickNoteOverlay"
      />
    </div>

    <!-- Mobile Sliding Modal Drawer -->
    <NavigationDrawer v-if="layoutState.isCompact" :docked="false" />

    <QuickNoteOverlay />

    <!-- Mobile Bottom 5-Tab Navigation Bar: 对话 | 今天 | 收集箱 | 知识 | 设置 -->
    <nav class="bottom-nav" role="navigation">
      <button type="button" class="bottom-nav-item" :class="{ active: activeDrawer === 'chat' && !settingsOpen }" @click="switchPrimaryDrawer('chat')">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" /></svg>
        <span>{{ t('navChat') }}</span>
      </button>
      <button type="button" class="bottom-nav-item" :class="{ active: activeDrawer === 'schedule' && !settingsOpen }" @click="switchPrimaryDrawer('schedule')">
        <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="18" rx="2" /><path d="M16 2v4M8 2v4M3 10h18" /></svg>
        <span>{{ t('navSchedule') }}</span>
      </button>
      <button type="button" class="bottom-nav-item" :class="{ active: activeDrawer === 'inbox' && !settingsOpen }" @click="switchPrimaryDrawer('inbox')">
        <svg viewBox="0 0 24 24" aria-hidden="true"><polyline points="22 12 16 12 14 15 10 15 8 12 2 12" /><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" /></svg>
        <span v-if="(inbox?.unsortedCount?.value || inbox?.unsortedCount || 0) > 0" class="nav-badge" :aria-label="t('inboxUnsortedBadge', {count: (inbox?.unsortedCount?.value ?? inbox?.unsortedCount ?? 0)})">{{ inbox?.unsortedCount?.value ?? inbox?.unsortedCount ?? 0 }}</span>
        <span>{{ t('inboxTitle') }}</span>
      </button>
      <button type="button" class="bottom-nav-item" :class="{ active: activeDrawer === 'knowledge' && !settingsOpen }" @click="switchPrimaryDrawer('knowledge')">
        <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="18" cy="5" r="3" /><circle cx="6" cy="12" r="3" /><circle cx="18" cy="19" r="3" /><path d="m8.59 13.51 6.83 3.98M15.41 6.51l-6.82 3.98" /></svg>
        <span>{{ t('navKnowledge') }}</span>
      </button>
      <button type="button" class="bottom-nav-item" :aria-label="t('settings')" :class="{ active: settingsOpen || activeDrawer === 'settings' }" @click="switchPrimaryDrawer('settings')">
        <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9c.26.6.77 1.02 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" /></svg>
        <span>{{ t('navSettings') }}</span>
      </button>
    </nav>
  </div>
</template>
