import { reactive } from 'vue';
import { createOnlineWorkClient } from '../online-client.mjs';

export function useOnlineWorkNode(ctx) {
  const {
    t,
    available,
    kernelBridge,
    sessionId,
    locallyAccepted,
    draft,
    controller,
    activeDrawer,
    previousNonSettingsDrawer,
    settingsOpen,
    navigationOpen,
    reload,
    flushCurrentViewState,
    selectConversation,
    applyCatalogPreferences,
    switchSettingsTab,
  } = ctx;
  const applyModelHubSnapshot = (...args) => ctx.applyModelHubSnapshot(...args);
  const loadModelHub = (...args) => ctx.loadModelHub(...args);

const onlineClient = createOnlineWorkClient();
const onlineState = reactive({
  origin: 'https://bob-online.bobbik.org',
  clientInstanceId: `bob-client-${Math.random().toString(36).slice(2, 10)}-instance`,
  pairing: null,
  token: null,
  hasToken: false,
  connected: false,
  offlineCached: false,
  capabilities: null,
  projects: [],
  selectedProjectId: null,
  tasks: [],
  messages: [],
  messagesNextAfter: 0,
  messagesHasMore: false,
  taskSnapshots: {},
  busy: false,
  error: null,
  statusNotice: null,
});

function handleOnlineUpdateOrigin(val) {
  onlineState.origin = val;
}

async function persistOnlineNodeState(overrides = {}) {
  if (!available || !kernelBridge.remoteNodeSave) return;
  const baseUrl = (overrides.baseUrl ?? onlineState.origin ?? '').trim();
  if (!baseUrl) return;
  const snap = await kernelBridge.remoteNodeSave({
    id: overrides.id || null,
    label: overrides.label || baseUrl,
    baseUrl,
    token: overrides.token !== undefined ? overrides.token : null,
    clearToken: Boolean(overrides.clearToken),
    selectedProjectId:
      overrides.selectedProjectId !== undefined
        ? overrides.selectedProjectId
        : onlineState.selectedProjectId,
    enabled: overrides.enabled !== undefined ? overrides.enabled : true,
  });
  if (snap) {
    applyModelHubSnapshot(snap);
  }
}

async function handleOnlineSaveNode({ origin, token }) {
  const cleanOrigin = (origin || onlineState.origin || '').trim();
  if (!cleanOrigin) return;
  onlineState.origin = cleanOrigin;
  onlineState.error = null;
  onlineState.statusNotice = null;
  const cleanToken = (token || '').trim();
  try {
    await persistOnlineNodeState({
      baseUrl: cleanOrigin,
      token: cleanToken || null,
      enabled: true,
    });
    if (cleanToken) {
      onlineState.hasToken = true;
      onlineState.connected = true;
    }
    if (onlineState.hasToken || onlineState.token) {
      await handleOnlineRefresh();
    }
    void reload();
  } catch (err) {
    onlineState.error = String(err?.message || err || 'ONLINE_SAVE_FAILED');
  }
}

async function handleOnlineBeginPairing() {
  if (onlineState.busy) return;
  onlineState.busy = true;
  onlineState.error = null;
  onlineState.statusNotice = null;
  try {
    await persistOnlineNodeState({ baseUrl: onlineState.origin, enabled: true });
    if (available && kernelBridge.onlineBeginPairing) {
      onlineState.pairing = await kernelBridge.onlineBeginPairing({
        origin: onlineState.origin,
        clientInstanceId: onlineState.clientInstanceId,
        clientName: 'Bob Cross-Platform Client',
      });
    } else {
      onlineState.pairing = await onlineClient.beginPairing({
        origin: onlineState.origin,
        clientInstanceId: onlineState.clientInstanceId,
        clientName: 'Bob Cross-Platform Client',
      });
    }
  } catch (err) {
    onlineState.error = String(err?.message || err || 'ONLINE_PAIRING_FAILED');
  } finally {
    onlineState.busy = false;
  }
}

async function handleOnlineExchangePairing() {
  if (onlineState.busy || !onlineState.pairing) return;
  onlineState.busy = true;
  onlineState.error = null;
  try {
    if (available && kernelBridge.onlineExchangePairing) {
      const res = await kernelBridge.onlineExchangePairing({
        origin: onlineState.pairing.origin || onlineState.origin,
        pairingId: onlineState.pairing.pairing_id,
        clientInstanceId: onlineState.clientInstanceId,
        verifier: onlineState.pairing.verifier || null,
      });
      if (!res.pending && (res.connected || res.hasToken)) {
        onlineState.hasToken = true;
        onlineState.connected = true;
        onlineState.pairing = null;
        await loadModelHub();
        await handleOnlineRefresh();
        void reload();
      }
    } else {
      const res = await onlineClient.exchangePairing({
        origin: onlineState.pairing.origin || onlineState.origin,
        pairingId: onlineState.pairing.pairing_id,
        clientInstanceId: onlineState.clientInstanceId,
        verifier: onlineState.pairing.verifier,
      });
      if (!res.pending && res.access_token) {
        onlineState.token = res.access_token;
        onlineState.hasToken = true;
        onlineState.connected = true;
        onlineState.pairing = null;
        await persistOnlineNodeState({
          baseUrl: onlineState.origin,
          token: res.access_token,
          enabled: true,
        });
        await handleOnlineRefresh();
        void reload();
      }
    }
  } catch (err) {
    onlineState.error = String(err?.message || err || 'ONLINE_EXCHANGE_FAILED');
  } finally {
    onlineState.busy = false;
  }
}

function applyOnlineSyncPayload(syncData) {
  if (!syncData) return;
  onlineState.connected = Boolean(syncData.connected);
  onlineState.offlineCached = Boolean(syncData.offlineCached);
  onlineState.capabilities = syncData.capabilities || null;
  onlineState.projects = Array.isArray(syncData.projects) ? syncData.projects : [];
  if (syncData.selectedProjectId) {
    onlineState.selectedProjectId = syncData.selectedProjectId;
  }
  onlineState.tasks = Array.isArray(syncData.tasks) ? syncData.tasks : [];
  onlineState.messages = Array.isArray(syncData.messages) ? syncData.messages : [];
  onlineState.messagesNextAfter = Number(syncData.messagesNextAfter) || 0;
  onlineState.messagesHasMore = Boolean(syncData.messagesHasMore);
  onlineState.taskSnapshots = syncData.taskSnapshots || {};
  if (syncData.syncError) {
    onlineState.error = String(syncData.syncError);
  }
}

async function handleOnlineRefresh() {
  if (!onlineState.hasToken && !onlineState.token) return;
  onlineState.busy = true;
  onlineState.error = null;
  try {
    if (available && kernelBridge.onlineSync) {
      const syncData = await kernelBridge.onlineSync({
        origin: onlineState.origin,
        projectId: onlineState.selectedProjectId || null,
        after: 0,
      });
      applyOnlineSyncPayload(syncData);
    } else if (onlineState.token) {
      const [caps, projs] = await Promise.all([
        onlineClient.fetchCapabilities({ origin: onlineState.origin, token: onlineState.token }),
        onlineClient.fetchProjects({ origin: onlineState.origin, token: onlineState.token }),
      ]);
      onlineState.capabilities = caps;
      onlineState.projects = Array.isArray(projs?.items) ? projs.items : [];
      if (onlineState.projects.length > 0 && !onlineState.selectedProjectId) {
        await handleOnlineSelectProject(onlineState.projects[0].id);
      }
    }
  } catch (err) {
    const msg = String(err?.message || err || 'ONLINE_REFRESH_FAILED');
    onlineState.error = msg;
    if (msg.includes('CLIENT_CREDENTIAL_INVALID') || msg.includes('401') || msg.includes('403')) {
      onlineState.connected = false;
      onlineState.hasToken = false;
    }
  } finally {
    onlineState.busy = false;
  }
}

async function handleOnlineSelectProject(projectId) {
  if ((!onlineState.hasToken && !onlineState.token) || !projectId) return;
  onlineState.selectedProjectId = projectId;
  void persistOnlineNodeState({ selectedProjectId: projectId }).catch(() => {});
  try {
    if (available && kernelBridge.onlineSync) {
      const syncData = await kernelBridge.onlineSync({
        origin: onlineState.origin,
        projectId,
        after: 0,
      });
      applyOnlineSyncPayload(syncData);
    } else if (onlineState.token) {
      const [tasksRes, msgsRes] = await Promise.all([
        onlineClient.fetchProjectTasks({
          origin: onlineState.origin,
          token: onlineState.token,
          projectId,
        }),
        onlineClient.fetchProjectMessages({
          origin: onlineState.origin,
          token: onlineState.token,
          projectId,
        }),
      ]);
      onlineState.tasks = Array.isArray(tasksRes?.items) ? tasksRes.items : [];
      onlineState.messages = Array.isArray(msgsRes?.items) ? msgsRes.items : [];
    }
  } catch (err) {
    onlineState.error = String(err?.message || err || 'ONLINE_PROJECT_READ_FAILED');
  }
}

async function handleOnlineLoadMoreMessages({ projectId, after }) {
  if ((!onlineState.hasToken && !onlineState.token) || !projectId) return;
  try {
    if (available && kernelBridge.onlineSync) {
      const prevMsgs = [...onlineState.messages];
      const syncData = await kernelBridge.onlineSync({
        origin: onlineState.origin,
        projectId,
        after: Number(after) || onlineState.messagesNextAfter || 0,
      });
      const nextBatch = Array.isArray(syncData?.messages) ? syncData.messages : [];
      const seenSeq = new Set(prevMsgs.map(m => m.sequence));
      for (const m of nextBatch) {
        if (!seenSeq.has(m.sequence)) prevMsgs.push(m);
      }
      onlineState.messages = prevMsgs;
      onlineState.messagesNextAfter = Number(syncData?.messagesNextAfter) || onlineState.messagesNextAfter;
      onlineState.messagesHasMore = Boolean(syncData?.messagesHasMore);
    }
  } catch (err) {
    onlineState.error = String(err?.message || err || 'ONLINE_MESSAGES_PAGE_FAILED');
  }
}

async function handleOnlineSubmitOperation(payload) {
  if (!payload || onlineState.busy) return;
  onlineState.busy = true;
  onlineState.error = null;
  try {
    if (available && kernelBridge.onlineSubmitOperation) {
      await kernelBridge.onlineSubmitOperation({
        origin: onlineState.origin,
        operationKind: payload.operationKind || 'continue_task',
        projectId: payload.projectId || onlineState.selectedProjectId,
        taskId: payload.taskId || null,
        prompt: payload.prompt || null,
        parentVersionId: payload.parentVersionId || null,
      });
      await handleOnlineRefresh();
      await reload();
    }
  } catch (err) {
    onlineState.error = String(err?.message || err || 'ONLINE_OPERATION_FAILED');
  } finally {
    onlineState.busy = false;
  }
}

async function handleOnlineRevoke(localOnly = false) {
  onlineState.busy = true;
  onlineState.error = null;
  try {
    if (available && kernelBridge.onlineRevoke) {
      const res = await kernelBridge.onlineRevoke({
        origin: onlineState.origin,
        localOnly: Boolean(localOnly),
      });
      if (!localOnly && res && !res.remoteRevoked && res.revokeError) {
        onlineState.statusNotice = `Local credential cleared; remote revoke reported: ${res.revokeError}`;
      } else {
        onlineState.statusNotice = null;
      }
    } else if (!localOnly && onlineState.token) {
      await onlineClient.revokeSelf({ origin: onlineState.origin, token: onlineState.token });
    }
  } catch (err) {
    onlineState.statusNotice = String(err?.message || err || 'ONLINE_REVOKE_NOTICE');
  } finally {
    void persistOnlineNodeState({ clearToken: true, enabled: false }).catch(() => {});
    onlineState.token = null;
    onlineState.hasToken = false;
    onlineState.connected = false;
    onlineState.offlineCached = false;
    onlineState.pairing = null;
    onlineState.projects = [];
    onlineState.tasks = [];
    onlineState.messages = [];
    onlineState.taskSnapshots = {};
    onlineState.busy = false;
  }
}
const handleOnlineDisconnect = () => handleOnlineRevoke(false);

async function handleWorkCreateProject(title) {
  if (!title) return;
  if (available && kernelBridge.createProject) {
    const pid = `proj-${crypto.randomUUID().slice(0, 8)}`;
    const snap = await kernelBridge.createProject(pid, title).catch(() => null);
    if (snap) applyCatalogPreferences(snap);
  }
}

async function handleWorkCreateConversation(projectId) {
  if (!available || !kernelBridge.createConversation) return;
  await flushCurrentViewState();
  const cid = `conv-${crypto.randomUUID().slice(0, 8)}`;
  const snap = await kernelBridge
    .createConversation(cid, projectId, t('newConversation'))
    .catch(() => null);
  if (snap) {
    applyCatalogPreferences(snap);
    locallyAccepted.clear();
    sessionId.value = cid;
    draft.value = '';
    controller.loadDraft('', 0);
    activeDrawer.value = 'chat';
    settingsOpen.value = false;
    navigationOpen.value = false;
    await reload();
  }
}

async function handleWorkOpenConversation(convId) {
  activeDrawer.value = 'chat';
  settingsOpen.value = false;
  navigationOpen.value = false;
  await selectConversation(convId);
}

async function handleWorkOpenSettingsTab(tab) {
  previousNonSettingsDrawer.value = activeDrawer.value === 'settings' ? 'chat' : activeDrawer.value;
  activeDrawer.value = 'settings';
  navigationOpen.value = false;
  settingsOpen.value = true;
  await switchSettingsTab(tab);
}


  return Object.assign(ctx, {
    onlineClient,
    onlineState,
    handleOnlineUpdateOrigin,
    persistOnlineNodeState,
    handleOnlineSaveNode,
    handleOnlineBeginPairing,
    handleOnlineExchangePairing,
    applyOnlineSyncPayload,
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
  });
}
