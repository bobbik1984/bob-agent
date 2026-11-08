import { computed, nextTick, ref, watch } from 'vue';
import { isBobNodeRuntime } from '../http-bridge.mjs';
import { loadClientState, saveClientState, chooseStartupConversation } from '../client-view-state.mjs';
import { createLayoutState } from '../layout-mode.mjs';
import { kernelBridge } from '../native-bridge.mjs';
import { createSendController } from '../send-controller.mjs';
import {
  createExecutionController,
  canCancelExecution,
  executionStatusKey,
  singleFlight,
  blocksNewExecution,
} from '../execution-controller.mjs';

export function useChatSession(ctx) {
  const { t, locale } = ctx;

const sessionId = ref(null);
const assistantConversationId = ref(null);
const assistantLinks = ref([]);
const assistantPollFailed = ref(false);
let assistantPolling = false;
let assistantNextPoll = 0;
async function refreshAssistantResults() {
  if (!browserClient || assistantPolling || sessionId.value !== assistantConversationId.value || Date.now() < assistantNextPoll) return;
  const current = sessionId.value;
  assistantPolling = true;
  try {
    const result = await kernelBridge.assistantPoll(current);
    if (current === sessionId.value) {
      assistantLinks.value = result.links || [];
      assistantPollFailed.value = Boolean(result.errors?.length);
    }
  } catch { if (current === sessionId.value) assistantPollFailed.value = true; }
  finally { assistantPolling = false; assistantNextPoll = Date.now() + (assistantPollFailed.value ? 30000 : 8000); }
}
function assistantLink(message) { return assistantLinks.value.find(link => link.request_id === message.request_id); }
function openAssistantTarget(link) {
  const node = (ctx.modelHub.value.state.remote_nodes || []).find(n => n.id === link.connection_id);
  if (node) void selectRemoteConversation({node,session:{id:link.native_session_id,title:link.target_title}});
}
const browserClient = isBobNodeRuntime();
const newConversationPending = ref(false);
const clientState = loadClientState();
const messages = ref([]);
const readiness = ref(null);
const executions = ref([]);
const executionFault = ref(null);
const homeFault = ref(false);
const cancelPending = ref(new Set());
const dispatchPending = ref(false);
const catalog = ref(null);
const directorySyncRevision = ref(0);
const sidebarGroupMode = ref('project');
const sidebarSearch = ref('');
const remoteConversation = ref(null);
async function selectRemoteConversation(target) {
  restoreRemoteSelection.value = false;
  remoteConversation.value = target;
  try {
    localStorage.setItem(
      'bob-active-remote',
      JSON.stringify({ nodeId: target.node.id, session: { id: target.session.id, title: target.session.title } }),
    );
  } catch {}
  activeDrawer.value = 'chat';
  settingsOpen.value = false;
  navigationOpen.value = false;
  void flushCurrentViewState();
}

const restoreRemoteSelection = ref(true);
const expandedProjects = ref({});
const showLegacy = ref(false);
const matchesSidebar = item => !sidebarSearch.value.trim() || String(item.title || '').toLocaleLowerCase().includes(sidebarSearch.value.trim().toLocaleLowerCase());
const recentFirst = items => [...items].sort((a,b) => (Number(b.updated_at)||0)-(Number(a.updated_at)||0) || String(a.id).localeCompare(String(b.id)));
const isLegacyProject = item => item.source_kind === 'legacy_import' || (catalog.value?.imported_project_ids || []).includes(item.id);
const projectExpanded = id => expandedProjects.value[id] ?? (Boolean(sidebarSearch.value.trim()) || activeConversationItem.value?.project_id === id);
function toggleProject(id) { expandedProjects.value[id] = !projectExpanded(id); }
function activityText(item) { return Number(item.updated_at) > 0 ? new Date(Number(item.updated_at) * (Number(item.updated_at) < 1e12 ? 1000 : 1)).toLocaleDateString() : t('sidebarTimeUnknown'); }
const theme = ref('system');
const creatingKind = ref(null); // 'project' | 'conversation' | null
const creatingForProjectId = ref(null);
const newItemTitle = ref('');

const layoutState = ref(
  createLayoutState(
    typeof window !== 'undefined' ? window : { innerWidth: 1180, innerHeight: 757 },
    typeof navigator !== 'undefined' ? navigator : { userAgent: '' },
  ),
);
function updateLayoutState() {
  if (typeof window === 'undefined') return;
  layoutState.value = createLayoutState(window, navigator);
}

let poll = null;
let disposed = false;
let releaseViewport = () => {};
let draftSaveTimer = null;
let scrollSaveTimer = null;
const locallyAccepted = new Map();
let unconfirmedRequest = null;

const executionController = createExecutionController(kernelBridge);
const directProviderEnabled = computed(
  () =>
    readiness.value?.mode === 'direct_provider' &&
    readiness.value?.execution_available === true,
);
const executionEnabled = computed(
  () =>
    (readiness.value?.mode === 'codex_readonly' ||
      readiness.value?.mode === 'direct_provider') &&
    readiness.value?.execution_available === true,
);
const homeConfigured = computed(() => readiness.value?.mode === 'home_configured');
const pairingStatusState = ref(null);
const hasOnlineDeviceGroup = computed(() =>
  (catalog.value?.device_groups || []).some(
    g => g.availability_state === 'online' || g.immediate_execution_available,
  ),
);
const devicePaired = computed(
  () =>
    homeConfigured.value ||
    Boolean(ctx.onlineState?.connected) ||
    ['connected', 'confirmed', 'paired', 'home_ready'].includes(
      pairingStatusState.value?.state,
    ) ||
    Boolean(pairingStatusState.value?.paired_home) ||
    Boolean(pairingStatusState.value?.online_node) ||
    hasOnlineDeviceGroup.value,
);
const homeReachable = ref(false);
const deviceOnline = computed(() => homeReachable.value && !homeFault.value);
// The badge identifies the connected target, never infers it from this screen's size.
const pairedHomeIdentity = computed(() => catalog.value?.device_groups?.find(group => group.role === 'paired_home')?.device_id);
const connectionTarget = computed(() => browserClient
  ? { kind: 'server', label: t('connectionWebNode') }
  : homeConfigured.value || pairingStatusState.value?.role === 'replica'
    ? { kind: 'home', label: pairedHomeIdentity.value ? t('connectionHome') + ' · ' + pairedHomeIdentity.value.slice(0, 6) : t('connectionHome') }
    : { kind: 'local', label: t('connectionLocal') });
const connectionLabel = computed(() => t('connectionTargetDetails', {
  target: connectionTarget.value.label,
  status: t(deviceOnline.value ? 'connectionStatusOnline' : 'connectionStatusUnknown'),
}));

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
    return [
      'codex_execution',
      'home_codex_execution',
      'direct_provider',
      'replica_direct_provider',
      'native_session_result',
    ].includes(message.source)
      ? 'replySaved'
      : 'probeReplySaved';
  }
  return executionStatusKey(messageExecution(message)) || statusKeys[message.send_state] || 'unknown';
}

const available = kernelBridge.available();
const connectionKey = computed(() =>
  !available
    ? 'nativeRequired'
    : directProviderEnabled.value
      ? 'directProviderReady'
      : executionEnabled.value
        ? 'codexReadOnlyReady'
        : homeConfigured.value
          ? 'homeConfiguredHint'
          : readiness.value
            ? 'providerUnconfigured'
            : 'loading',
);

const activeDrawer = ref('chat'); // 'chat' | 'schedule' | 'work' | 'knowledge' | 'settings'
const previousNonSettingsDrawer = ref('chat');
const settingsOpen = ref(false);
const immersive = ref(false);
try { immersive.value = localStorage.getItem('bob-immersive') === 'true'; } catch {}
watch(immersive, value => { try { localStorage.setItem('bob-immersive', String(value)); } catch {} });
const immersiveActive = computed(() => immersive.value && activeDrawer.value === 'chat' && !settingsOpen.value);
const settingsView = ref('home'); // 'home' | 'device' | 'provider'
const settingsTab = ref('model'); // 'model' | 'connection' | 'workspace' | 'tasks' | 'appearance' | 'about'
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
  () => sessionId.value === assistantConversationId.value ? t('defaultAssistant') : (messages.value.length ? (activeConversationItem.value?.title || fallbackConversationTitle.value) : t('conversation')),
);
const totalUnreadCount = computed(() =>
  (catalog.value?.conversations || []).reduce((sum, c) => sum + (c.unread_count || 0), 0),
);
const standaloneList = computed(() => {
  if (catalog.value?.standalone_conversations) return recentFirst(catalog.value.standalone_conversations.filter(matchesSidebar));
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
const projectList = computed(() => recentFirst((catalog.value?.projects || []).map(p => ({...p, updated_at: Math.max(Number(p.updated_at)||0, ...(catalog.value?.conversations || []).filter(c => c.project_id === p.id).map(c => Number(c.updated_at)||0))}))).filter(p => (showLegacy.value || sidebarSearch.value.trim() || !isLegacyProject(p)) && (matchesSidebar(p) || projectConversations(p.id).length)));
const legacyCount = computed(() => (catalog.value?.projects || []).filter(isLegacyProject).length);
const deviceGroupList = computed(() => (catalog.value?.device_groups || []).map(g => ({...g, conversations: recentFirst((g.conversations || []).filter(c => !(catalog.value?.imported_project_ids || []).includes(c.project_id) && matchesSidebar(c)))})));
function projectConversations(projectId) {
  return (recentFirst(catalog.value?.conversations || [])).filter(c => c.project_id === projectId && (matchesSidebar(c) || matchesSidebar((catalog.value?.projects || []).find(p => p.id === projectId) || {})));
}

async function switchPrimaryDrawer(drawer) {
  navigationOpen.value = false;
  if (drawer === 'settings') {
    toggleSettingsHome();
    return;
  }
  previousNonSettingsDrawer.value = drawer;
  activeDrawer.value = drawer;
  settingsOpen.value = false;
  if (drawer === 'schedule' || drawer === 'knowledge' || drawer === 'work' || drawer === 'inbox') {
    if (available && kernelBridge.syncHome) {
      void kernelBridge.syncHome().then(async status => {
        if (status !== 'sync_checked' || disposed) return;
        directorySyncRevision.value += 1;
        const snap = await kernelBridge.catalog().catch(() => null);
        if (snap && !disposed) applyCatalogPreferences(snap);
        void ctx.loadModelHub();
        void ctx.loadPersonalOps();
        if (ctx.inbox?.loadInbox) void ctx.inbox.loadInbox();
      }).catch(() => {});
    }
    await ctx.loadPersonalOps();
    if (drawer === 'inbox' && ctx.inbox?.loadInbox) {
      await ctx.inbox.loadInbox();
      void ctx.inbox.loadTriageDraft?.(false);
    }
    if (drawer === 'work') {
      await ctx.loadModelHub();
    }
  }
}

function toggleSettingsHome() {
  if (settingsOpen.value && settingsView.value === 'home') {
    settingsOpen.value = false;
    activeDrawer.value = previousNonSettingsDrawer.value || 'chat';
  } else {
    if (activeDrawer.value !== 'settings') {
      previousNonSettingsDrawer.value = activeDrawer.value;
    }
    activeDrawer.value = 'settings';
    navigationOpen.value = false;
    settingsView.value = 'home';
    settingsTab.value = 'model';
    settingsOpen.value = true;
    void ctx.loadModelHub();
    void ctx.loadPersonalOps();
  }
}

async function switchSettingsTab(tab) {
  settingsTab.value = tab;
  settingsView.value = 'home';
  if (tab === 'connection') {
    await nextTick();
    await devicePanel.value?.openDetails();
  } else if (tab === 'model') {
    await ctx.loadModelHub();
  } else if (tab === 'workspace') {
    await ctx.loadPersonalOps();
  } else if (tab === 'tasks') {
    if (available && sessionId.value && kernelBridge.routeGet) {
      const r = await kernelBridge.routeGet(sessionId.value).catch(() => null);
      if (r) ctx.executionRoute.value = r;
    }
  }
}

async function openDevices() {
  if (activeDrawer.value !== 'settings') {
    previousNonSettingsDrawer.value = activeDrawer.value;
  }
  activeDrawer.value = 'settings';
  settingsView.value = 'home';
  settingsTab.value = 'connection';
  settingsOpen.value = true;
  await nextTick();
  await devicePanel.value?.openDetails();
}

const settingsPanel = ref(null);
let settingsTrigger = null;
watch(settingsOpen, async open => {
  if (open) settingsTrigger = document.activeElement;
  else if (activeDrawer.value === 'settings') {
    activeDrawer.value = previousNonSettingsDrawer.value || 'chat';
  }
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

function formatMessageTime(ts) {
  if (ts === null || ts === undefined || ts === '' || typeof ts === 'boolean') return '';
  const raw = typeof ts === 'number' ? ts : Number(ts);
  if (Number.isFinite(raw) && raw <= 0) return '';
  const ms = Number.isFinite(raw) ? (raw < 1e12 && raw > 1e9 ? raw * 1000 : raw) : Date.parse(String(ts));
  if (!Number.isFinite(ms) || ms < 946684800000) return '';
  const d = new Date(ms);
  const now = new Date();
  const p = n => String(n).padStart(2, '0');
  const hm = `${p(d.getHours())}:${p(d.getMinutes())}`;
  if (d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth() && d.getDate() === now.getDate()) return hm;
  if (d.getFullYear() === now.getFullYear()) return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${hm}`;
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${hm}`;
}

const copyState = ref({});
async function copyReply(message) {
  try {
    await navigator.clipboard.writeText(message.content);
    copyState.value = { ...copyState.value, [message.id]: 'copied' };
    setTimeout(() => {
      if (copyState.value[message.id] === 'copied') {
        const next = { ...copyState.value };
        delete next[message.id];
        copyState.value = next;
      }
    }, 1600);
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
const composerAttachments = ref([]);
const composerFileInput = ref(null);
const historyPullUpDistance = ref(0);
let historyTouchStartY = null;
let historyWheelOverscroll = 0;
let historyWheelResetTimer = null;

function openComposerFilePicker() {
  composerFileInput.value?.click?.();
}

function isTextComposerFile(f) {
  const m = String(f?.type || '').toLowerCase();
  const n = String(f?.name || '').toLowerCase();
  if (m.startsWith('image/') || m.startsWith('audio/') || m.startsWith('video/') || m === 'application/pdf') return false;
  if (m.startsWith('text/') || m.includes('json') || m.includes('yaml') || m.includes('xml') || m.includes('javascript')) return true;
  return /\.(txt|md|json|ya?ml|csv|tsv|py|rs|js|mjs|cjs|ts|tsx|jsx|vue|html|css|scss|sh|bat|ps1|toml|ini|cfg|log|sql|gd|tscn)$/i.test(n);
}

async function handleComposerFiles(event) {
  const files = Array.from(event?.target?.files || []);
  for (const f of files) {
    let content = '';
    try {
      if (isTextComposerFile(f) && typeof f.text === 'function' && (f.size || 0) <= 256 * 1024) {
        content = await f.text();
      }
    } catch {}
    composerAttachments.value.push({
      id: crypto.randomUUID(),
      name: f.name || 'file',
      size: f.size || 0,
      type: f.type || '',
      content,
    });
  }
  if (event?.target) event.target.value = '';
}

function removeComposerAttachment(id) {
  composerAttachments.value = composerAttachments.value.filter(a => a.id !== id);
}

function formatComposerAttachments(list) {
  if (!list?.length) return '';
  return list
    .map(a =>
      a.content
        ? `[Attachment: ${a.name}]\n\`\`\`\n${String(a.content).slice(0, 16384)}\n\`\`\``
        : `[Attachment: ${a.name} (${a.size || 0} bytes)]`,
    )
    .join('\n\n');
}

function onHistoryTouchStart(event) {
  const el = historyRegion.value;
  const atBottom = !el || el.scrollHeight - el.scrollTop - el.clientHeight <= 16;
  historyTouchStartY = atBottom ? (event?.touches?.[0]?.clientY ?? null) : null;
  historyPullUpDistance.value = 0;
}

function onHistoryTouchMove(event) {
  if (historyTouchStartY === null) return;
  const y = event?.touches?.[0]?.clientY;
  if (typeof y !== 'number') return;
  const delta = historyTouchStartY - y;
  const el = historyRegion.value;
  const atBottom = !el || el.scrollHeight - el.scrollTop - el.clientHeight <= 20;
  if (delta > 8 && atBottom) {
    historyPullUpDistance.value = Math.min(72, Math.round((delta - 8) * 0.75));
  } else if (delta <= 0) {
    historyPullUpDistance.value = 0;
  }
}

function onHistoryTouchEnd() {
  const dist = historyPullUpDistance.value;
  historyTouchStartY = null;
  historyPullUpDistance.value = 0;
  if (dist >= 36) {
    void forceRefreshActiveConversation();
  }
}

function onHistoryWheel(event) {
  if (!(event?.deltaY > 24)) return;
  const el = historyRegion.value;
  const atBottom = !el || el.scrollHeight - el.scrollTop - el.clientHeight <= 8;
  if (!atBottom) {
    historyWheelOverscroll = 0;
    return;
  }
  historyWheelOverscroll += event.deltaY;
  if (historyWheelResetTimer) clearTimeout(historyWheelResetTimer);
  historyWheelResetTimer = setTimeout(() => {
    historyWheelOverscroll = 0;
  }, 450);
  if (historyWheelOverscroll >= 140) {
    historyWheelOverscroll = 0;
    void forceRefreshActiveConversation();
  }
}

const remoteSyncPhase = ref('');
const mainSyncPhase = ref('synced');
let mainSyncPulseTimer = null;
globalThis.__bobSetRemoteSyncPhase = phase => {
  remoteSyncPhase.value = phase || '';
};

const deviceSyncPhase = computed(() => {
  if (remoteConversation.value) {
    return remoteSyncPhase.value || (deviceOnline.value ? 'synced' : 'offline');
  }
  if (saving.value || dispatchPending.value || activeExecution.value || mainSyncPhase.value === 'syncing') {
    return 'syncing';
  }
  if (loading.value || mainSyncPhase.value === 'checking') {
    return 'checking';
  }
  if (homeFault.value || (devicePaired.value && !deviceOnline.value)) {
    return 'offline';
  }
  return 'synced';
});

async function forceRefreshActiveConversation() {
  if (remoteConversation.value && typeof globalThis.__bobForceRefreshDsh === 'function') {
    await globalThis.__bobForceRefreshDsh();
    return;
  }
  mainSyncPhase.value = 'checking';
  resetSyncBackoffAndReload();
  void ctx.loadModelHub?.();
}

const executionBlocked = computed(() => blocksNewExecution(executions.value));
const activeExecution = computed(() => executions.value.find(run => canCancelExecution(run)));
const canSend = computed(
  () =>
    !!sessionId.value &&
    (!!draft.value.trim() || composerAttachments.value.length > 0) &&
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
  try { localStorage.setItem('bob-sidebar-mode', mode); } catch {}
  if (available && kernelBridge.savePreferences && (mode === 'project' || mode === 'device')) {
    await kernelBridge.savePreferences({ sidebarGroupMode: mode }).catch(() => {});
  }
}

async function organizeSessionsViaAgent() {
  await openAssistant();
  const promptText = t('smartGroupAgentPrompt');
  draft.value = promptText;
  controller.setDraft(promptText);
  await send();
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
  if (newConversationPending.value) { clientState.blankDraft = text; saveClientState(clientState); return; }
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
  if (newConversationPending.value || !available || !sessionId.value || !historyRegion.value || !kernelBridge.saveViewState) return;
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
  if (JSON.stringify(catalog.value) !== JSON.stringify(snap)) catalog.value = snap;
  let localMode = null;
  try { localMode = localStorage.getItem('bob-sidebar-mode'); } catch {}
  if (localMode === 'smart') {
    sidebarGroupMode.value = 'smart';
  } else if (snap.preferences?.sidebar_group_mode) {
    sidebarGroupMode.value = snap.preferences.sidebar_group_mode;
  }
  if (snap.preferences?.locale === 'zh-CN' || snap.preferences?.locale === 'en-US') {
    locale.value = snap.preferences.locale;
  }
  if (snap.preferences?.theme) {
    theme.value = snap.preferences.theme;
    applyTheme(theme.value);
  }
}

let syncFailureStreak = 0;
let nextAllowedSyncAt = 0;

const reload = singleFlight(async () => {
  if (!sessionId.value || disposed) return;
  const loadingSession = sessionId.value;
  const prevMsgCount = messages.value.length;
  mainSyncPhase.value = 'checking';
  void refreshAssistantResults();
  if (Date.now() >= nextAllowedSyncAt) {
    void kernelBridge
      .syncHome()
      .then(async status => {
        if (disposed) return;
        if (status !== 'sync_in_progress') {
          syncFailureStreak = 0;
          nextAllowedSyncAt = 0;
          homeFault.value = false;
          if (status === 'sync_checked') {
            homeReachable.value = true;
            directorySyncRevision.value += 1;
            void ctx.loadModelHub();
            void ctx.loadPersonalOps();
            if (activeDrawer.value === 'inbox') void ctx.inbox?.loadInbox();
            const snap = await kernelBridge.catalog().catch(() => null);
            if (snap && !disposed) applyCatalogPreferences(snap);
          }
        }
      })
      .catch(() => {
        syncFailureStreak += 1;
        const backoffMs = Math.min(60000, 3000 * Math.pow(2, syncFailureStreak - 1));
        nextAllowedSyncAt = Date.now() + backoffMs;
        homeFault.value = true;
      });
  }
  const tasks = [
    newConversationPending.value ? Promise.resolve([]) : kernelBridge.history(loadingSession),
    kernelBridge.readiness(),
    newConversationPending.value ? Promise.resolve([]) : kernelBridge.executions(loadingSession),
    kernelBridge.catalog ? kernelBridge.catalog() : Promise.resolve(null),
    kernelBridge.pairingStatus ? kernelBridge.pairingStatus() : Promise.resolve(null),
  ];
  const results = await Promise.allSettled(tasks);
  if (disposed || loadingSession !== sessionId.value) return;
  const follow = nearBottom();
  if (results[0].status === 'fulfilled') {
    const stored = results[0].value;
    for (const message of stored) locallyAccepted.delete(message.id);
    messages.value = [...stored, ...locallyAccepted.values()];
    fault.value = null;
  } else {
    fault.value = 'historyFailed';
  }
  if (results[1].status === 'fulfilled') {
    readiness.value = results[1].value;
    if (browserClient) homeReachable.value = true;
  }
  else {
    readiness.value = null;
    homeReachable.value = false;
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
  if (results[4]?.status === 'fulfilled' && results[4].value) {
    pairingStatusState.value = results[4].value;
  }
  if (fault.value || homeFault.value) {
    mainSyncPhase.value = 'offline';
  } else if (messages.value.length > prevMsgCount && prevMsgCount > 0) {
    mainSyncPhase.value = 'syncing';
    if (mainSyncPulseTimer) clearTimeout(mainSyncPulseTimer);
    mainSyncPulseTimer = setTimeout(() => {
      mainSyncPhase.value = 'synced';
    }, 700);
  } else {
    mainSyncPhase.value = 'synced';
  }
  await nextTick();
  if (follow && historyRegion.value) {
    historyRegion.value.scrollTop = historyRegion.value.scrollHeight;
  }
});

async function flushCurrentViewState() {
  if (newConversationPending.value || !available || !sessionId.value || !kernelBridge.saveViewState) return;
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

async function openAssistant() {
  navigationOpen.value = false;
  settingsOpen.value = false;
  activeDrawer.value = 'chat';
  remoteConversation.value = null;
  try { localStorage.removeItem('bob-active-remote'); } catch {}
  if (assistantConversationId.value) {
    await selectConversation(assistantConversationId.value);
    return;
  }
  try {
    const entry = await kernelBridge.assistantConversation();
    assistantConversationId.value = entry.conversation_id;
    await selectConversation(entry.conversation_id);
  } catch { fault.value = 'startupFailed'; }
}

async function selectConversation(targetId) {
  remoteConversation.value = null;
  try { localStorage.removeItem('bob-active-remote'); } catch {}
  activeDrawer.value = 'chat';
  settingsOpen.value = false;
  navigationOpen.value = false;
  if (!targetId) return;
  if (targetId === sessionId.value) {
    return;
  }
  void flushCurrentViewState();
  locallyAccepted.clear();
  messages.value = [];
  executions.value = [];
  sessionId.value = targetId;
  newConversationPending.value = false;
  clientState.selectedConversation = targetId;
  saveClientState(clientState);
  if (available && kernelBridge.selectConversation) {
    const snap = await kernelBridge.selectConversation(targetId).catch(() => null);
    if (snap) applyCatalogPreferences(snap);
  }
  const targetItem = catalog.value?.conversations?.find(c => c.id === targetId);
  const restoredDraft = targetItem?.view_state?.draft ?? '';
  const restoredRev = targetItem?.view_state?.draft_revision ?? 0;

  draft.value = restoredDraft;
  controller.loadDraft(restoredDraft, restoredRev);
  navigationOpen.value = false;
  await reload();
  await nextTick();
  if (historyRegion.value) historyRegion.value.scrollTop = historyRegion.value.scrollHeight;
}

function startCreateProject() {
  creatingKind.value = 'project';
  creatingForProjectId.value = null;
  newItemTitle.value = '';
}

function startCreateConversation(projectId = null) {
  remoteConversation.value=null;try{localStorage.removeItem('bob-active-remote')}catch{};
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
  await ctx.loadModelHub?.();
  await ctx.loadPersonalOps?.();
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
    const initialCatalog = await kernelBridge.catalog();
    applyCatalogPreferences(initialCatalog);
    const entry = await kernelBridge.assistantConversation();
    assistantConversationId.value = entry.conversation_id;
    const selected = chooseStartupConversation(clientState, initialCatalog);
    newConversationPending.value = false;
    sessionId.value = selected || entry.conversation_id;
    if (newConversationPending.value) {
      draft.value = clientState.blankDraft || '';
      controller.loadDraft(draft.value, 0);
    }
    await reload();
    await ctx.loadModelHub();
    await ctx.loadPersonalOps();
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
  if (!executionEnabled.value && !homeConfigured.value) {
    fault.value = 'sendNotConfigured';
    input.value?.focus();
    return;
  }
  saving.value = true;
  fault.value = null;
  const attBlock = formatComposerAttachments(composerAttachments.value);
  const sentText = [draft.value.trim(), attBlock].filter(Boolean).join('\n\n') || draft.value;
  if (attBlock) {
    controller.setDraft(sentText);
  }
  const shouldExecute = executionEnabled.value;
  try {
    if (newConversationPending.value) {
      await kernelBridge.createConversation(sessionId.value, null, sentText.trim().slice(0, 60));
      newConversationPending.value = false;
      clientState.selectedConversation = sessionId.value;
      saveClientState(clientState);
    }
    const receipt = await controller.send();
    if (receipt) {
      composerAttachments.value = [];
      clientState.blankDraft = '';
      saveClientState(clientState);
    }
    if (receipt && !messages.value.some(message => message.id === receipt.message_id)) {
      const message = {
        id: receipt.message_id,
        request_id: receipt.request_id,
        role: 'user',
        content: sentText,
        send_state: 'local_saved',
        created_at: Date.now(),
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
        const execution = browserClient && sessionId.value === assistantConversationId.value && directProviderEnabled.value
          ? await kernelBridge.assistantExecute(receipt.request_id)
          : await executionController.dispatch(receipt);
        assistantNextPoll = 0;
        void refreshAssistantResults();
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
    void ctx.loadPersonalOps();
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

  function resetSyncBackoffAndReload() {
    if (!disposed && available) {
      nextAllowedSyncAt = 0;
      void reload();
    }
  }

  function startPolling() {
    if (!disposed && available) {
      let idleTick = 0;
      poll = setInterval(() => {
        if (typeof document !== 'undefined' && document.hidden) return;
        const isActiveExecution = Boolean(
          activeExecution.value || dispatchPending.value || saving.value,
        );
        if (!isActiveExecution) {
          idleTick = (idleTick + 1) % 40;
          if (idleTick !== 0) return;
        } else {
          idleTick = 0;
        }
        void reload();
      }, 1500);
    }
  }

  function disposeChatSession() {
    disposed = true;
    if (draftSaveTimer) clearTimeout(draftSaveTimer);
    if (scrollSaveTimer) clearTimeout(scrollSaveTimer);
    if (historyWheelResetTimer) clearTimeout(historyWheelResetTimer);
    if (mainSyncPulseTimer) clearTimeout(mainSyncPulseTimer);
    if (poll) clearInterval(poll);
  }

  return Object.assign(ctx, {
    available,
    browserClient,
    kernelBridge,
    clientState,
    saveClientState,
    sessionId,
    assistantConversationId,
    assistantLinks,
    assistantPollFailed,
    assistantLink,
    openAssistantTarget,
    newConversationPending,
    restoreRemoteSelection,
    messages,
    readiness,
    executions,
    executionFault,
    homeFault,
    cancelPending,
    dispatchPending,
    catalog,
    directorySyncRevision,
    sidebarGroupMode,
    sidebarSearch,
    remoteConversation,
    selectRemoteConversation,
    expandedProjects,
    showLegacy,
    projectExpanded,
    toggleProject,
    activityText,
    theme,
    creatingKind,
    creatingForProjectId,
    newItemTitle,
    layoutState,
    updateLayoutState,
    locallyAccepted,
    directProviderEnabled,
    executionEnabled,
    homeConfigured,
    pairingStatusState,
    devicePaired,
    homeReachable,
    deviceOnline,
    connectionTarget,
    connectionLabel,
    sendLabel,
    messageExecution,
    messageStatus,
    connectionKey,
    activeDrawer,
    previousNonSettingsDrawer,
    settingsOpen,
    immersive,
    immersiveActive,
    settingsView,
    settingsTab,
    navigationOpen,
    devicePanel,
    deviceCapabilities,
    fallbackConversationTitle,
    activeConversationItem,
    currentConversation,
    totalUnreadCount,
    standaloneList,
    projectList,
    legacyCount,
    deviceGroupList,
    projectConversations,
    switchPrimaryDrawer,
    toggleSettingsHome,
    switchSettingsTab,
    openDevices,
    settingsPanel,
    settingsKeydown,
    formatMessageTime,
    copyState,
    copyReply,
    draft,
    saving,
    loading,
    fault,
    input,
    historyRegion,
    composerAttachments,
    composerFileInput,
    openComposerFilePicker,
    handleComposerFiles,
    removeComposerAttachment,
    historyPullUpDistance,
    onHistoryTouchStart,
    onHistoryTouchMove,
    onHistoryTouchEnd,
    onHistoryWheel,
    deviceSyncPhase,
    forceRefreshActiveConversation,
    executionBlocked,
    activeExecution,
    canSend,
    switchSidebarMode,
    organizeSessionsViaAgent,
    controller,
    updateDraft,
    focusComposer,
    onHistoryScroll,
    applyCatalogPreferences,
    reload,
    flushCurrentViewState,
    openAssistant,
    selectConversation,
    startCreateProject,
    startCreateConversation,
    submitCreateItem,
    cancel,
    focusPairedComposer,
    startup,
    send,
    keydown,
    resetSyncBackoffAndReload,
    startPolling,
    disposeChatSession,
  });
}
