<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import HomeConnection from './HomeConnection.vue';
import { bindViewport } from './viewport.mjs';
import brandLogo from './assets/bob-logo.svg';
import { useI18n } from 'vue-i18n';
import { kernelBridge } from './native-bridge.mjs';
import { createSendController } from './send-controller.mjs';
import {
  createExecutionController,
  canCancelExecution,
  executionStatusKey,
  singleFlight,
  blocksNewExecution,
} from './execution-controller.mjs';

const { t, locale } = useI18n();
const sessionId = ref(null);
const messages = ref([]);
const readiness = ref(null);
const executions = ref([]);
const executionFault = ref(null);
const homeFault = ref(false);
const cancelPending = ref(new Set());
const dispatchPending = ref(false);
const catalog = ref(null);
const sidebarGroupMode = ref('project');
const theme = ref('system');
const creatingKind = ref(null); // 'project' | 'conversation' | null
const creatingForProjectId = ref(null);
const newItemTitle = ref('');

let poll = null;
let disposed = false;
let releaseViewport = () => {};
let draftSaveTimer = null;
let scrollSaveTimer = null;
const locallyAccepted = new Map();
let unconfirmedRequest = null;

const executionController = createExecutionController(kernelBridge);
const executionEnabled = computed(
  () => readiness.value?.mode === 'codex_readonly' && readiness.value?.execution_available === true,
);
const homeConfigured = computed(() => readiness.value?.mode === 'home_configured');
const sendLabel = computed(() => executionEnabled.value || homeConfigured.value);
const byMessage = computed(() => new Map(executions.value.map(run => [run.message_id, run])));

function messageExecution(message) {
  return (
    byMessage.value.get(message.id) ||
    executions.value.find(run => run.request_id === message.request_id && !!message.request_id)
  );
}
function messageStatus(message) {
  if (message.role === 'assistant') {
    return ['codex_execution', 'home_codex_execution'].includes(message.source)
      ? 'replySaved'
      : 'probeReplySaved';
  }
  return executionStatusKey(messageExecution(message)) || statusKeys[message.send_state] || 'unknown';
}

const available = kernelBridge.available();
const connectionKey = computed(() =>
  !available
    ? 'nativeRequired'
    : executionEnabled.value
      ? 'codexReadOnlyReady'
      : homeConfigured.value
        ? 'homeConfiguredHint'
        : readiness.value
          ? 'providerUnconfigured'
          : 'loading',
);

const settingsOpen = ref(false);
const settingsView = ref('home'); // 'home' | 'device'
const navigationOpen = ref(false);
const devicePanel = ref(null);
const deviceCapabilities = ref(null);

const fallbackConversationTitle = computed(
  () => messages.value.find(m => m.role === 'user')?.content?.slice(0, 60) || t('conversation'),
);
const activeConversationItem = computed(() =>
  catalog.value?.conversations?.find(c => c.id === sessionId.value) || null,
);
const currentConversation = computed(
  () => activeConversationItem.value?.title || fallbackConversationTitle.value,
);
const totalUnreadCount = computed(() =>
  (catalog.value?.conversations || []).reduce((sum, c) => sum + (c.unread_count || 0), 0),
);
const standaloneList = computed(() => {
  if (catalog.value?.standalone_conversations) return catalog.value.standalone_conversations;
  if (!sessionId.value) return [];
  return [
    {
      id: sessionId.value,
      title: fallbackConversationTitle.value,
      unread_count: 0,
      target_device_online: true,
      status_reason: '',
    },
  ];
});
const projectList = computed(() => catalog.value?.projects || []);
const deviceGroupList = computed(() => catalog.value?.device_groups || []);
function projectConversations(projectId) {
  return (catalog.value?.conversations || []).filter(c => c.project_id === projectId);
}

function toggleSettingsHome() {
  if (settingsOpen.value && settingsView.value === 'home') {
    settingsOpen.value = false;
  } else {
    settingsView.value = 'home';
    settingsOpen.value = true;
  }
}

async function openDevices() {
  settingsView.value = 'device';
  settingsOpen.value = true;
  await nextTick();
  await devicePanel.value?.openDetails();
}

const settingsPanel = ref(null);
let settingsTrigger = null;
watch(settingsOpen, async open => {
  if (open) settingsTrigger = document.activeElement;
  await nextTick();
  if (open) settingsPanel.value?.querySelector('button')?.focus();
  else settingsTrigger?.focus();
});
watch(settingsView, async () => {
  if (!settingsOpen.value) return;
  await nextTick();
  settingsPanel.value?.querySelector('button')?.focus();
});

function settingsKeydown(event) {
  if (event.key === 'Escape') {
    event.preventDefault();
    settingsOpen.value = false;
    return;
  }
  if (event.key !== 'Tab') return;
  const controls = [
    ...settingsPanel.value.querySelectorAll('button,select,input,textarea,summary,[tabindex="0"]'),
  ].filter(el => !el.disabled && el.getClientRects().length);
  const first = controls[0];
  const last = controls.at(-1);
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}

const copyState = ref({});
async function copyReply(message) {
  try {
    await navigator.clipboard.writeText(message.content);
    copyState.value = { ...copyState.value, [message.id]: 'copied' };
  } catch {
    copyState.value = { ...copyState.value, [message.id]: 'copyFailed' };
  }
}

const draft = ref('');
const saving = ref(false);
const loading = ref(true);
const fault = ref(null);
const input = ref(null);
const historyRegion = ref(null);

const executionBlocked = computed(() => blocksNewExecution(executions.value));
const activeExecution = computed(() => executions.value.find(run => canCancelExecution(run)));
const canSend = computed(
  () =>
    !!sessionId.value &&
    !!draft.value.trim() &&
    !loading.value &&
    !saving.value &&
    !executionBlocked.value,
);

function applyTheme(value) {
  if (value === 'light' || value === 'dark') {
    document.documentElement.dataset.theme = value;
  } else {
    delete document.documentElement.dataset.theme;
  }
}

watch(locale, value => {
  document.documentElement.lang = value;
  if (available && kernelBridge.savePreferences) {
    void kernelBridge.savePreferences({ locale: value }).catch(() => {});
  }
});

watch(theme, value => {
  applyTheme(value);
  if (available && kernelBridge.savePreferences) {
    void kernelBridge.savePreferences({ theme: value }).catch(() => {});
  }
});

async function switchSidebarMode(mode) {
  sidebarGroupMode.value = mode;
  if (available && kernelBridge.savePreferences) {
    await kernelBridge.savePreferences({ sidebarGroupMode: mode }).catch(() => {});
  }
}

const controller = createSendController({
  bridge: kernelBridge,
  createIntent: (content, sentRevision) => ({
    request_id: crypto.randomUUID(),
    message_id: crypto.randomUUID(),
    session_id: sessionId.value,
    content,
    expected_draft_revision: sentRevision,
  }),
  onAccepted: () => {},
});

function scheduleDraftSave(convId, text) {
  if (!available || !convId || !kernelBridge.saveViewState) return;
  if (draftSaveTimer) clearTimeout(draftSaveTimer);
  draftSaveTimer = setTimeout(() => {
    void kernelBridge
      .saveViewState({ conversationId: convId, draft: text, markRead: false })
      .catch(() => {});
  }, 180);
}

function updateDraft(event) {
  draft.value = event.target.value;
  controller.setDraft(draft.value);
  if (sessionId.value) {
    void controller.prewarm();
    scheduleDraftSave(sessionId.value, draft.value);
  }
}

function focusComposer() {
  if (sessionId.value) void controller.prewarm();
}

function onHistoryScroll() {
  if (!available || !sessionId.value || !historyRegion.value || !kernelBridge.saveViewState) return;
  const offset = Math.max(0, Math.round(historyRegion.value.scrollTop));
  const convId = sessionId.value;
  if (scrollSaveTimer) clearTimeout(scrollSaveTimer);
  scrollSaveTimer = setTimeout(() => {
    void kernelBridge
      .saveViewState({ conversationId: convId, scrollOffset: offset, markRead: false })
      .catch(() => {});
  }, 220);
}

function nearBottom() {
  const region = historyRegion.value;
  return !region || region.scrollHeight - region.scrollTop - region.clientHeight < 48;
}

function applyCatalogPreferences(snap) {
  if (!snap) return;
  catalog.value = snap;
  if (snap.preferences?.sidebar_group_mode) {
    sidebarGroupMode.value = snap.preferences.sidebar_group_mode;
  }
  if (snap.preferences?.theme) {
    theme.value = snap.preferences.theme;
    applyTheme(theme.value);
  }
}

const reload = singleFlight(async () => {
  if (!sessionId.value || disposed) return;
  void kernelBridge
    .syncHome()
    .then(status => {
      if (status !== 'sync_in_progress') homeFault.value = false;
    })
    .catch(() => {
      homeFault.value = true;
    });
  const tasks = [
    kernelBridge.history(sessionId.value),
    kernelBridge.readiness(),
    kernelBridge.executions(sessionId.value),
    kernelBridge.catalog ? kernelBridge.catalog() : Promise.resolve(null),
  ];
  const results = await Promise.allSettled(tasks);
  if (disposed) return;
  const follow = nearBottom();
  if (results[0].status === 'fulfilled') {
    const stored = results[0].value;
    for (const message of stored) locallyAccepted.delete(message.id);
    messages.value = [...stored, ...locallyAccepted.values()];
    fault.value = null;
  } else {
    fault.value = 'historyFailed';
  }
  if (results[1].status === 'fulfilled') readiness.value = results[1].value;
  else {
    readiness.value = null;
    fault.value ||= 'statusFailed';
  }
  if (results[2].status === 'fulfilled') {
    executions.value = results[2].value;
    if (
      unconfirmedRequest &&
      executions.value.some(run => run.request_id === unconfirmedRequest)
    ) {
      unconfirmedRequest = null;
      executionFault.value = null;
    }
  } else {
    fault.value ||= 'executionReadFailed';
  }
  if (results[3].status === 'fulfilled' && results[3].value) {
    applyCatalogPreferences(results[3].value);
  }
  await nextTick();
  if (follow && historyRegion.value) {
    historyRegion.value.scrollTop = historyRegion.value.scrollHeight;
  }
});

async function flushCurrentViewState() {
  if (!available || !sessionId.value || !kernelBridge.saveViewState) return;
  if (draftSaveTimer) {
    clearTimeout(draftSaveTimer);
    draftSaveTimer = null;
  }
  const offset = historyRegion.value ? Math.max(0, Math.round(historyRegion.value.scrollTop)) : 0;
  await kernelBridge
    .saveViewState({
      conversationId: sessionId.value,
      draft: draft.value,
      scrollOffset: offset,
      markRead: true,
    })
    .catch(() => {});
}

async function selectConversation(targetId) {
  if (!targetId) return;
  if (targetId === sessionId.value) {
    navigationOpen.value = false;
    return;
  }
  await flushCurrentViewState();
  locallyAccepted.clear();
  sessionId.value = targetId;
  if (available && kernelBridge.selectConversation) {
    const snap = await kernelBridge.selectConversation(targetId).catch(() => null);
    if (snap) applyCatalogPreferences(snap);
  }
  const targetItem = catalog.value?.conversations?.find(c => c.id === targetId);
  const restoredDraft = targetItem?.view_state?.draft ?? '';
  const restoredRev = targetItem?.view_state?.draft_revision ?? 0;
  const restoredScroll = targetItem?.view_state?.scroll_offset ?? 0;
  draft.value = restoredDraft;
  controller.loadDraft(restoredDraft, restoredRev);
  navigationOpen.value = false;
  await reload();
  await nextTick();
  if (historyRegion.value && restoredScroll > 0) {
    historyRegion.value.scrollTop = restoredScroll;
  }
}

function startCreateProject() {
  creatingKind.value = 'project';
  creatingForProjectId.value = null;
  newItemTitle.value = '';
}

function startCreateConversation(projectId = null) {
  creatingKind.value = 'conversation';
  creatingForProjectId.value = projectId;
  newItemTitle.value = '';
}

async function submitCreateItem() {
  if (!available) return;
  const title = newItemTitle.value.trim();
  if (creatingKind.value === 'project') {
    if (!title || !kernelBridge.createProject) return;
    const pid = `proj-${crypto.randomUUID().slice(0, 8)}`;
    const snap = await kernelBridge.createProject(pid, title).catch(() => null);
    if (snap) applyCatalogPreferences(snap);
    creatingKind.value = null;
    newItemTitle.value = '';
  } else if (creatingKind.value === 'conversation') {
    if (!kernelBridge.createConversation) return;
    await flushCurrentViewState();
    const cid = `conv-${crypto.randomUUID().slice(0, 8)}`;
    const pid = creatingForProjectId.value;
    const snap = await kernelBridge
      .createConversation(cid, pid, title || t('newConversation'))
      .catch(() => null);
    creatingKind.value = null;
    creatingForProjectId.value = null;
    newItemTitle.value = '';
    if (snap) {
      applyCatalogPreferences(snap);
      locallyAccepted.clear();
      sessionId.value = cid;
      draft.value = '';
      controller.loadDraft('', 0);
      navigationOpen.value = false;
      await reload();
    }
  }
}

async function cancel(execution) {
  if (!canCancelExecution(execution) || cancelPending.value.has(execution.id)) return;
  cancelPending.value = new Set([...cancelPending.value, execution.id]);
  try {
    const receipt = await executionController.cancel(execution);
    executions.value = executions.value.map(run => (run.id === receipt.id ? receipt : run));
    executionFault.value = null;
  } catch {
    executionFault.value = 'cancelUnconfirmed';
  } finally {
    const pending = new Set(cancelPending.value);
    pending.delete(execution.id);
    cancelPending.value = pending;
    await reload();
  }
}

async function focusPairedComposer() {
  await reload();
  await nextTick();
  input.value?.focus();
}

async function startup() {
  loading.value = true;
  fault.value = null;
  if (!available) {
    loading.value = false;
    return;
  }
  try {
    const state = await kernelBridge.bootstrap();
    sessionId.value = state.session_id;
    await reload();
    const activeItem = catalog.value?.conversations?.find(c => c.id === sessionId.value);
    if (activeItem?.view_state && !draft.value) {
      draft.value = activeItem.view_state.draft || '';
      controller.loadDraft(draft.value, activeItem.view_state.draft_revision || 0);
    }
  } catch {
    fault.value = 'startupFailed';
  } finally {
    loading.value = false;
  }
}

async function send() {
  if (!canSend.value) return;
  saving.value = true;
  fault.value = null;
  const sentText = draft.value;
  const shouldExecute = executionEnabled.value;
  try {
    const receipt = await controller.send();
    if (receipt && !messages.value.some(message => message.id === receipt.message_id)) {
      const message = {
        id: receipt.message_id,
        request_id: receipt.request_id,
        role: 'user',
        content: sentText,
        send_state: 'local_saved',
      };
      locallyAccepted.set(message.id, message);
      messages.value.push(message);
    }
    draft.value = controller.snapshot().draft;
    await nextTick();
    input.value?.focus();
    if (receipt && shouldExecute) {
      dispatchPending.value = true;
      executionFault.value = null;
      try {
        const execution = await executionController.dispatch(receipt);
        executions.value = [
          ...executions.value.filter(run => run.id !== execution.id),
          execution,
        ];
      } catch {
        unconfirmedRequest = receipt.request_id;
        executionFault.value = 'executionStartUnconfirmed';
      } finally {
        dispatchPending.value = false;
      }
    }
    await reload();
  } catch {
    draft.value = controller.snapshot().draft;
    fault.value = 'saveFailed';
  } finally {
    saving.value = false;
  }
}

function keydown(event) {
  if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    void send();
  }
}

const statusKeys = {
  reply_saved: 'probeReplySaved',
  local_saved: 'localSaved',
  queued_for_wakeup: 'waitingHome',
  not_dispatched: 'waitingHome',
  dispatching: 'sending',
  dispatched: 'waitingReceipt',
  unknown_send: 'unknown',
  peer_accepted: 'homeReceived',
  rejected: 'rejected',
  cancelled_local: 'cancelled',
  needs_attention: 'attention',
};

onMounted(async () => {
  releaseViewport = bindViewport(window, document.documentElement);
  await startup();
  if (available) {
    deviceCapabilities.value = await kernelBridge.pairingCapabilities().catch(() => null);
  }
  if (!disposed && available) {
    poll = setInterval(() => {
      void reload();
    }, 1500);
  }
});

onUnmounted(() => {
  disposed = true;
  if (draftSaveTimer) clearTimeout(draftSaveTimer);
  if (scrollSaveTimer) clearTimeout(scrollSaveTimer);
  releaseViewport();
  if (poll) clearInterval(poll);
});
</script>
<template>
  <div class="app-shell">
    <header class="topbar">
      <button
        type="button"
        class="icon-button"
        :aria-label="t('openNavigation')"
        :aria-expanded="navigationOpen"
        @click="navigationOpen = true"
      >
        ☰
        <span v-if="totalUnreadCount > 0" class="unread-dot" aria-hidden="true"></span>
      </button>
      <span class="page-name">{{ currentConversation }}</span>
      <button
        type="button"
        class="device-indicator"
        :class="{ paired: homeConfigured, unconfirmed: homeFault }"
        :aria-label="
          t(
            homeFault
              ? 'deviceStatusUnknown'
              : homeConfigured
                ? 'devicePairedDetails'
                : 'connectDeviceDetails',
          )
        "
        @click="openDevices"
      >
        <svg v-if="deviceCapabilities?.can_scan" viewBox="0 0 24 24" aria-hidden="true">
          <rect x="3" y="3" width="18" height="13" rx="2" />
          <path d="M8 21h8M12 16v5" />
        </svg>
        <svg v-else viewBox="0 0 24 24" aria-hidden="true">
          <rect x="6" y="2" width="12" height="20" rx="2" />
          <path d="M10 18h4" />
        </svg>
        <span class="device-dot" aria-hidden="true"></span>
      </button>
      <button
        type="button"
        class="icon-button settings-toggle"
        :aria-label="t('settings')"
        :aria-expanded="settingsOpen"
        @click="toggleSettingsHome"
      >
        ⋯
      </button>
    </header>

    <section
      v-show="settingsOpen"
      ref="settingsPanel"
      class="settings-panel"
      role="dialog"
      aria-modal="true"
      :aria-label="t(settingsView === 'device' ? 'deviceAndConnection' : 'settings')"
      @keydown="settingsKeydown"
    >
      <div class="settings-heading">
        <h2>{{ t(settingsView === 'device' ? 'deviceAndConnection' : 'settings') }}</h2>
        <div class="settings-heading-actions">
          <button
            type="button"
            :aria-label="t('closeSettings')"
            @click="settingsOpen = false"
          >
            ×
          </button>
          <button
            v-if="settingsView === 'device'"
            type="button"
            class="back-settings-btn"
            :aria-label="t('backToSettings')"
            @click="settingsView = 'home'"
          >
            ← {{ t('settings') }}
          </button>
        </div>
      </div>

      <div v-show="settingsView === 'home'" class="settings-home-grid">
        <div class="settings-card">
          <div class="settings-card-info">
            <span class="settings-card-title">{{ t('language') }}</span>
          </div>
          <label class="language">
            <span class="visually-hidden">{{ t('language') }}</span>
            <select v-model="locale">
              <option value="zh-CN">{{ t('chinese') }}</option>
              <option value="en-US">{{ t('english') }}</option>
            </select>
          </label>
        </div>

        <div class="settings-card">
          <div class="settings-card-info">
            <span class="settings-card-title">{{ t('themeLabel') }}</span>
          </div>
          <label class="theme-select">
            <span class="visually-hidden">{{ t('themeLabel') }}</span>
            <select v-model="theme">
              <option value="system">{{ t('themeSystem') }}</option>
              <option value="light">{{ t('themeLight') }}</option>
              <option value="dark">{{ t('themeDark') }}</option>
            </select>
          </label>
        </div>

        <div class="settings-card">
          <div class="settings-card-info">
            <span class="settings-card-title">{{ t('deviceAndConnection') }}</span>
            <p class="settings-card-desc">{{ t('deviceSubpageDesc') }}</p>
          </div>
          <button type="button" class="open-device-subpage" @click="openDevices">
            {{ t('openDeviceSubpage') }}
          </button>
        </div>

        <div class="settings-card">
          <div class="settings-card-info">
            <span class="settings-card-title">{{ t('aboutVersion') }}</span>
            <p class="settings-card-desc">{{ t('versionValue') }}</p>
          </div>
        </div>
      </div>

      <div v-show="settingsView === 'device'" class="settings-device-subpage">
        <HomeConnection
          ref="devicePanel"
          v-if="sessionId"
          :paired="homeConfigured"
          @changed="reload"
          @connected="
            settingsOpen = false;
            focusPairedComposer();
          "
        />
      </div>
    </section>

    <div v-if="navigationOpen" class="navigation-overlay" @keydown.esc="navigationOpen = false">
      <button
        type="button"
        class="navigation-backdrop"
        :aria-label="t('closeNavigation')"
        @click="navigationOpen = false"
      ></button>
      <nav class="navigation-drawer" :aria-label="t('conversations')">
        <div class="settings-heading">
          <h2>{{ t('conversations') }}</h2>
          <button
            type="button"
            :aria-label="t('closeNavigation')"
            @click="navigationOpen = false"
          >
            ×
          </button>
        </div>

        <div class="sidebar-mode-switch" role="group">
          <button
            type="button"
            class="sidebar-mode-btn"
            :class="{ active: sidebarGroupMode === 'project' }"
            @click="switchSidebarMode('project')"
          >
            {{ t('groupByProject') }}
          </button>
          <button
            type="button"
            class="sidebar-mode-btn"
            :class="{ active: sidebarGroupMode === 'device' }"
            @click="switchSidebarMode('device')"
          >
            {{ t('groupByDevice') }}
          </button>
        </div>

        <div class="sidebar-actions">
          <button type="button" @click="startCreateConversation(null)">
            + {{ t('newConversation') }}
          </button>
          <button type="button" @click="startCreateProject">
            + {{ t('newProject') }}
          </button>
        </div>

        <form v-if="creatingKind" class="sidebar-create-form" @submit.prevent="submitCreateItem">
          <input
            v-model="newItemTitle"
            type="text"
            :placeholder="
              t(
                creatingKind === 'project'
                  ? 'projectTitlePlaceholder'
                  : 'conversationTitlePlaceholder',
              )
            "
          />
          <div class="sidebar-create-buttons">
            <button type="button" @click="creatingKind = null">{{ t('cancelAction') }}</button>
            <button type="submit">{{ t('create') }}</button>
          </div>
        </form>

        <template v-if="sidebarGroupMode === 'project'">
          <section class="sidebar-section">
            <h2>{{ t('standaloneConversations') }}</h2>
            <div class="conversation-list">
              <button
                v-for="conv in standaloneList"
                :key="conv.id"
                type="button"
                class="conversation-link"
                :aria-current="conv.id === sessionId ? 'page' : undefined"
                @click="selectConversation(conv.id)"
              >
                <span class="conversation-link-title">{{ conv.title }}</span>
                <span v-if="conv.unread_count > 0" class="unread-badge">
                  {{ t('unreadBadge', { count: conv.unread_count }) }}
                </span>
              </button>
            </div>
          </section>

          <section class="sidebar-section">
            <h2>{{ t('projects') }}</h2>
            <p v-if="!projectList.length" class="navigation-note">{{ t('noProjectsHint') }}</p>
            <div v-for="proj in projectList" :key="proj.id" class="project-card">
              <div class="project-card-header">
                <span class="project-title">{{ proj.title }}</span>
                <button
                  type="button"
                  class="project-new-conv"
                  :aria-label="t('newProjectConversation')"
                  @click="startCreateConversation(proj.id)"
                >
                  + {{ t('newConversation') }}
                </button>
              </div>
              <div class="conversation-list">
                <button
                  v-for="conv in projectConversations(proj.id)"
                  :key="conv.id"
                  type="button"
                  class="conversation-link"
                  :aria-current="conv.id === sessionId ? 'page' : undefined"
                  @click="selectConversation(conv.id)"
                >
                  <span class="conversation-link-title">{{ conv.title }}</span>
                  <span v-if="conv.unread_count > 0" class="unread-badge">
                    {{ t('unreadBadge', { count: conv.unread_count }) }}
                  </span>
                </button>
              </div>
            </div>
          </section>
        </template>

        <template v-else>
          <section class="sidebar-section">
            <h2>{{ t('groupByDevice') }}</h2>
            <div v-for="group in deviceGroupList" :key="group.device_id" class="device-group-card">
              <div class="device-group-header">
                <span class="device-group-title">{{ group.label }}</span>
                <span
                  class="device-status-pill"
                  :class="{ online: group.immediate_execution_available }"
                >
                  {{
                    t(
                      group.availability_state === 'online'
                        ? 'deviceOnline'
                        : group.availability_state === 'sleeping'
                          ? 'deviceSleeping'
                          : 'deviceOffline',
                    )
                  }}
                </span>
              </div>
              <p class="device-status-reason">{{ group.status_reason }}</p>
              <div class="conversation-list">
                <button
                  v-for="conv in group.conversations"
                  :key="conv.id"
                  type="button"
                  class="conversation-link"
                  :aria-current="conv.id === sessionId ? 'page' : undefined"
                  @click="selectConversation(conv.id)"
                >
                  <span class="conversation-link-title">{{ conv.title }}</span>
                  <span v-if="conv.unread_count > 0" class="unread-badge">
                    {{ t('unreadBadge', { count: conv.unread_count }) }}
                  </span>
                </button>
              </div>
            </div>
          </section>
        </template>
      </nav>
    </div>

    <main class="main">
      <div
        v-if="!homeConfigured || readiness?.pending_messages > 0"
        class="connection-note"
        :class="{ 'home-paired': homeConfigured }"
        role="status"
      >
        <span>{{ t(connectionKey) }}</span>
        <span v-if="readiness && readiness.pending_messages > 0" class="storage-summary">
          {{
            t(executionEnabled ? 'localExecutionStatus' : 'localStatus', {
              saved: readiness.saved_messages,
              pending: readiness.pending_messages,
            })
          }}
        </span>
      </div>
      <div
        v-if="activeConversationItem && !activeConversationItem.target_device_online"
        class="notice"
        role="status"
      >
        <span>{{ activeConversationItem.status_reason || t('offlineImmediateDisabled') }}</span>
      </div>
      <div v-if="fault" class="notice" role="alert">
        <span>{{ t(fault) }}</span>
        <button type="button" @click="sessionId ? reload() : startup()">{{ t('reload') }}</button>
      </div>
      <div v-if="homeFault" class="notice" role="status">{{ t('homeSyncUnconfirmed') }}</div>
      <div v-if="executionFault" class="notice" role="alert">
        <span>{{ t(executionFault) }}</span>
        <button type="button" @click="reload">{{ t('checkStatus') }}</button>
      </div>
      <section
        ref="historyRegion"
        class="history"
        :aria-label="t('conversation')"
        aria-live="polite"
        :aria-busy="loading"
        @scroll.passive="onHistoryScroll"
      >
        <div v-if="loading" class="empty loading-brand">
          <img :src="brandLogo" :alt="t('brand')" />
          <p>{{ t('loading') }}</p>
        </div>
        <div v-else-if="!messages.length" class="empty">
          <h1>{{ t('startTitle') }}</h1>
          <p>
            {{
              t(
                homeConfigured
                  ? 'homeStartHint'
                  : executionEnabled
                    ? 'startExecutionHint'
                    : 'startHint',
              )
            }}
          </p>
        </div>
        <article
          v-for="message in messages"
          :key="message.id"
          class="message"
          :class="{ 'assistant-message': message.role === 'assistant' }"
        >
          <span v-if="message.role === 'assistant'" class="message-state">
            {{
              t(
                ['codex_execution', 'home_codex_execution'].includes(message.source)
                  ? 'assistantLiveReply'
                  : 'assistantReply',
              )
            }}
          </span>
          <p class="message-body">{{ message.content }}</p>
          <button
            v-if="message.role === 'assistant'"
            type="button"
            class="copy-reply"
            @click="copyReply(message)"
          >
            {{ t(copyState[message.id] || 'copyReply') }}
          </button>
          <span class="message-state">{{ t(messageStatus(message)) }}</span>
          <button
            v-if="canCancelExecution(messageExecution(message))"
            type="button"
            class="cancel-execution"
            :disabled="cancelPending.has(messageExecution(message).id)"
            @click="cancel(messageExecution(message))"
          >
            {{
              t(
                cancelPending.has(messageExecution(message).id)
                  ? 'requestingStop'
                  : 'stopExecution',
              )
            }}
          </button>
          <span v-if="messageExecution(message)?.status === 'unknown'" class="message-state">
            {{ t('noAutomaticReplay') }}
          </span>
        </article>
      </section>
      <form class="composer" @submit.prevent="send">
        <label class="visually-hidden" for="message-input">{{ t('message') }}</label>
        <textarea
          id="message-input"
          ref="input"
          :value="draft"
          :disabled="!sessionId"
          :placeholder="t('placeholder')"
          rows="2"
          @input="updateDraft"
          @focus="focusComposer"
          @keydown="keydown"
        />
        <div class="composer-actions">
          <span>
            {{
              t(
                dispatchPending
                  ? 'requestingExecution'
                  : saving
                    ? 'saving'
                    : executionBlocked
                      ? 'executionBusyHint'
                      : homeConfigured
                        ? 'homeSendHint'
                        : executionEnabled
                          ? 'executeHint'
                          : 'localFirstHint',
              )
            }}
          </span>
          <button
            v-if="activeExecution"
            type="button"
            :disabled="cancelPending.has(activeExecution.id)"
            @click="cancel(activeExecution)"
          >
            {{ t(cancelPending.has(activeExecution.id) ? 'requestingStop' : 'stopExecution') }}
          </button>
          <button v-else type="submit" :disabled="!canSend">
            {{ t(sendLabel ? 'sendAndExecute' : 'send') }}
          </button>
        </div>
      </form>
    </main>
  </div>
</template>
