import { computed, ref, watch } from 'vue';
import { isBobNodeRuntime, streamJarvisTurn } from '../http-bridge.mjs';

const STORAGE_KEY = 'bob-jarvis-widgets-v1';
export const MAX_FEATURE_STORAGE_BYTES = 8192;

function utf8ByteLength(str) {
  if (typeof TextEncoder !== 'undefined') {
    return new TextEncoder().encode(str).length;
  }
  return Buffer.byteLength(str, 'utf8');
}

const DEFAULT_WIDGETS = [
  { id: 'pending', tier: 'l1', enabled: true, order: 0 },
  { id: 'today', tier: 'l1', enabled: true, order: 1 },
  { id: 'inbox', tier: 'l1', enabled: true, order: 2 },
  { id: 'routines', tier: 'l1', enabled: true, order: 3 },
  { id: 'quotas', tier: 'pro', enabled: false, order: 4 },
  { id: 'nodes', tier: 'pro', enabled: false, order: 5 },
];

function loadPersistedWidgets() {
  try {
    if (typeof localStorage !== 'undefined') {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) {
        const parsed = JSON.parse(raw);
        if (Array.isArray(parsed) && parsed.length > 0) {
          const map = new Map(parsed.map(w => [w.id, w]));
          const merged = DEFAULT_WIDGETS.map((def, idx) => {
            const existing = map.get(def.id);
            if (existing) {
              return { ...def, ...existing };
            }
            return { ...def, order: idx };
          });
          return merged.sort((a, b) => a.order - b.order);
        }
      }
    }
  } catch {}
  return DEFAULT_WIDGETS.map(w => ({ ...w }));
}

export function useJarvisHome(ctx = {}) {
  // 1. Progressive Tier Detection (hasProCluster)
  const hasProCluster = computed(() => {
    const nodes =
      ctx.modelHub?.value?.state?.remote_nodes ||
      ctx.modelHub?.state?.remote_nodes ||
      [];
    const online = Boolean(
      ctx.onlineState?.value?.connected ||
        ctx.onlineState?.connected ||
        ctx.onlineState?.value?.paired ||
        ctx.onlineState?.paired,
    );
    return nodes.length > 0 || online;
  });

  // 2. Widget Grid Configuration
  const widgets = ref(loadPersistedWidgets());
  const installedFeatures = ref([]);
  const customFeatureWidgets = ref([]);
  const trialFeatures = computed(() =>
    installedFeatures.value.filter(
      f => f.status === 'trial' && (!f.trial_expires_at || f.trial_expires_at > 0),
    ),
  );
  const editingWidgets = ref(false);
  const widgetSheetOpen = ref(false);

  function saveWidgets() {
    try {
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(widgets.value));
      }
    } catch {}
  }

  function toggleWidgetEnabled(id) {
    const w = widgets.value.find(item => item.id === id);
    if (!w) return;
    w.enabled = !w.enabled;
    if (w.tier === 'pro' && w.enabled) {
      w.userExplicitlyEnabled = true;
    }
    saveWidgets();
  }

  function moveWidget(id, delta) {
    const list = [...widgets.value].sort((a, b) => a.order - b.order);
    const idx = list.findIndex(item => item.id === id);
    if (idx === -1) return;
    const targetIdx = idx + delta;
    if (targetIdx < 0 || targetIdx >= list.length) return;
    const temp = list[idx];
    list[idx] = list[targetIdx];
    list[targetIdx] = temp;
    list.forEach((item, index) => {
      item.order = index;
    });
    widgets.value = list;
    saveWidgets();
  }

  function resetWidgets() {
    widgets.value = DEFAULT_WIDGETS.map(w => ({ ...w }));
    try {
      if (typeof localStorage !== 'undefined') {
        localStorage.removeItem(STORAGE_KEY);
      }
    } catch {}
  }

  // 3. Jarvis Streaming, Cards & Approvals
  const jarvisStreaming = ref(false);
  const jarvisSpeaking = ref(false);
  const streamingReplyText = ref('');
  const streamingSessionId = ref(null);
  const messageCards = ref({});
  const pendingApprovals = ref([]);
  const budgetState = ref(null);
  const activeTakeover = ref(null);
  const siteSessions = ref([]);
  const proactivityState = ref({
    level: 0.35,
    quietHoursStart: '22:00',
    quietHoursEnd: '08:00',
  });

  const pendingApprovalCount = computed(
    () => pendingApprovals.value.filter(a => !a.status || a.status === 'pending').length,
  );

  const orbMood = computed(() => {
    if (pendingApprovals.value.some(a => !a.status || a.status === 'pending')) {
      return 'needs_human';
    }
    if (jarvisSpeaking.value) {
      return 'speaking';
    }
    if (jarvisStreaming.value) {
      return 'thinking';
    }
    return 'idle';
  });

  const visibleWidgets = computed(() => {
    return widgets.value
      .filter(w => {
        if (!w.enabled) return false;
        if (w.tier === 'pro' && !hasProCluster.value && !w.userExplicitlyEnabled) {
          return false;
        }
        if (w.id === 'pending' && pendingApprovalCount.value === 0 && !editingWidgets.value) {
          return false;
        }
        return true;
      })
      .sort((a, b) => a.order - b.order);
  });

  function cardsForMessage(message) {
    if (!message) return [];
    if (message.id && messageCards.value[message.id]?.length) {
      return messageCards.value[message.id];
    }
    if (message.request_id && messageCards.value[message.request_id]?.length) {
      return messageCards.value[message.request_id];
    }
    return [];
  }

  function upsertApproval(card) {
    const idx = pendingApprovals.value.findIndex(a => a.id === card.id);
    if (idx !== -1) {
      pendingApprovals.value.splice(idx, 1, { ...pendingApprovals.value[idx], ...card });
    } else {
      pendingApprovals.value.push(card);
    }
  }

  function updateCardStatus(cardId, newStatus) {
    const app = pendingApprovals.value.find(c => c.id === cardId);
    if (app) {
      app.status = newStatus;
    }
    for (const key of Object.keys(messageCards.value)) {
      const cards = messageCards.value[key];
      if (Array.isArray(cards)) {
        for (const c of cards) {
          if (c.id === cardId) {
            c.status = newStatus;
          }
        }
      }
    }
  }

  async function refreshFeatures() {
    if (!ctx.kernelBridge || typeof ctx.kernelBridge.featuresList !== 'function') return [];
    try {
      const list = await ctx.kernelBridge.featuresList().catch(() => null);
      if (Array.isArray(list)) {
        installedFeatures.value = [...list];
        customFeatureWidgets.value = list.filter(
          f =>
            (f.status === 'active' || f.status === 'trial') &&
            f.has_widget !== false &&
            Boolean(f.widget_html) &&
            f.enabled !== false,
        );
        return list;
      }
    } catch {}
    return [];
  }

  async function keepFeature(name) {
    if (typeof ctx.kernelBridge?.featureKeep === 'function') {
      await ctx.kernelBridge.featureKeep(name).catch(() => null);
    }
    const feat = installedFeatures.value.find(f => f.name === name);
    if (feat) {
      feat.status = 'active';
      feat.trial_expires_at = null;
    }
    installedFeatures.value = [...installedFeatures.value];
    await refreshFeatures();
  }

  async function rollbackFeature(name, targetVersion = null) {
    if (typeof ctx.kernelBridge?.featureRollback === 'function') {
      await ctx.kernelBridge.featureRollback(name, targetVersion).catch(() => null);
    }
    installedFeatures.value = installedFeatures.value.filter(f => f.name !== name);
    customFeatureWidgets.value = customFeatureWidgets.value.filter(f => f.name !== name);
    await refreshFeatures();
  }

  async function toggleFeature(name, enabled) {
    if (typeof ctx.kernelBridge?.featureToggle === 'function') {
      await ctx.kernelBridge.featureToggle(name, enabled).catch(() => null);
    }
    const feat = installedFeatures.value.find(f => f.name === name);
    if (feat) {
      feat.enabled = enabled;
    }
    installedFeatures.value = [...installedFeatures.value];
    await refreshFeatures();
  }

  async function callFeatureRoute(name, route, payload = {}) {
    if (!ctx.kernelBridge || typeof ctx.kernelBridge.featureCall !== 'function') {
      throw new Error('NATIVE_HOST_REQUIRED');
    }
    const res = await ctx.kernelBridge.featureCall(name, route, payload);
    if (res && res.cached_state !== undefined) {
      const feat = installedFeatures.value.find(f => f.name === name);
      if (feat) {
        feat.cached_state = res.cached_state;
      }
      const w = customFeatureWidgets.value.find(f => f.name === name);
      if (w) {
        w.cached_state = res.cached_state;
      }
    }
    return res;
  }

  async function saveFeatureStorage(name, storage) {
    const raw = JSON.stringify(storage ?? {});
    const bytes = utf8ByteLength(raw);
    if (bytes > MAX_FEATURE_STORAGE_BYTES) {
      throw new Error('FEATURE_STORAGE_LIMIT_EXCEEDED');
    }
    let res = null;
    if (typeof ctx.kernelBridge?.featureSaveStorage === 'function') {
      res = await ctx.kernelBridge.featureSaveStorage(name, storage);
    }
    const feat = installedFeatures.value.find(f => f.name === name);
    if (feat) {
      feat.storage = storage;
    }
    const w = customFeatureWidgets.value.find(f => f.name === name);
    if (w) {
      w.storage = storage;
    }
    return res;
  }

  async function refreshJarvisState(sessionId = null) {
    if (!ctx.kernelBridge) return;
    try {
      if (typeof ctx.kernelBridge.jarvisApprovalsList === 'function') {
        const list = await ctx.kernelBridge.jarvisApprovalsList(sessionId).catch(() => null);
        if (Array.isArray(list)) {
          for (const item of list) {
            upsertApproval(item);
          }
        }
      }
    } catch {}
    try {
      if (typeof ctx.kernelBridge.jarvisBudgetGet === 'function') {
        const budget = await ctx.kernelBridge.jarvisBudgetGet().catch(() => null);
        if (budget && typeof budget === 'object') {
          budgetState.value = budget;
        }
      }
    } catch {}
    try {
      await refreshFeatures();
    } catch {}
    try {
      await refreshSiteSessions();
    } catch {}
    try {
      await refreshProactivity();
    } catch {}
  }

  async function approveAction(cardId) {
    if (typeof ctx.kernelBridge?.jarvisApprove === 'function') {
      await ctx.kernelBridge.jarvisApprove(cardId).catch(() => null);
    }
    updateCardStatus(cardId, 'approved');
    if (typeof ctx.reload === 'function') {
      await ctx.reload();
    }
  }

  async function rejectAction(cardId) {
    if (typeof ctx.kernelBridge?.jarvisReject === 'function') {
      await ctx.kernelBridge.jarvisReject(cardId).catch(() => null);
    }
    updateCardStatus(cardId, 'rejected');
    if (typeof ctx.reload === 'function') {
      await ctx.reload();
    }
  }

  function openTakeover(cardIdOrObj) {
    let card = null;
    if (cardIdOrObj && typeof cardIdOrObj === 'object') {
      card = cardIdOrObj;
    } else {
      card = pendingApprovals.value.find(c => c.id === cardIdOrObj);
      if (!card) {
        for (const cards of Object.values(messageCards.value)) {
          if (Array.isArray(cards)) {
            const found = cards.find(c => c.id === cardIdOrObj);
            if (found) {
              card = found;
              break;
            }
          }
        }
      }
      if (!card) {
        card = { id: cardIdOrObj };
      }
    }
    activeTakeover.value = {
      id: card.id || ('takeover-' + Date.now()),
      title: card.title || '',
      summary: card.summary || '',
      url: card.url || '',
      width: card.width || 390,
      height: card.height || 844,
      desktopMode: Boolean(card.desktopMode),
    };
    return activeTakeover.value;
  }

  async function completeTakeover(takeoverId) {
    const id = takeoverId || activeTakeover.value?.id;
    if (id && typeof ctx.kernelBridge?.browserTakeoverComplete === 'function') {
      await ctx.kernelBridge.browserTakeoverComplete(id).catch(() => null);
    }
    if (id) {
      updateCardStatus(id, 'approved');
    }
    activeTakeover.value = null;
    if (typeof ctx.reload === 'function') {
      await ctx.reload().catch(() => null);
    }
  }

  function closeTakeover() {
    activeTakeover.value = null;
  }

  async function refreshSiteSessions() {
    if (typeof ctx.kernelBridge?.browserSessionsList === 'function') {
      try {
        const list = await ctx.kernelBridge.browserSessionsList().catch(() => null);
        if (Array.isArray(list)) {
          siteSessions.value = [...list];
          return siteSessions.value;
        }
      } catch {}
    }
    return siteSessions.value;
  }

  async function openSiteForLogin(targetUrl) {
    let res = null;
    if (typeof ctx.kernelBridge?.browserSessionOpen === 'function') {
      res = await ctx.kernelBridge.browserSessionOpen(targetUrl).catch(() => null);
    }
    const takeoverId = res?.takeoverId || res?.takeover_id || res?.id || (`site-login-${Date.now()}`);
    const resolvedUrl = res?.url || targetUrl;
    activeTakeover.value = {
      id: takeoverId,
      title: 'Site Login',
      summary: resolvedUrl,
      url: resolvedUrl,
      width: 390,
      height: 844,
      desktopMode: false,
    };
    return activeTakeover.value;
  }

  async function clearSiteSession(domain) {
    if (typeof ctx.kernelBridge?.browserSessionClear === 'function') {
      await ctx.kernelBridge.browserSessionClear(domain).catch(() => null);
    }
    await refreshSiteSessions();
  }

  async function refreshProactivity() {
    if (typeof ctx.kernelBridge?.proactivityGet === 'function') {
      try {
        const res = await ctx.kernelBridge.proactivityGet().catch(() => null);
        if (res && typeof res === 'object') {
          proactivityState.value = {
            ...proactivityState.value,
            ...res,
          };
          return proactivityState.value;
        }
      } catch {}
    }
    return proactivityState.value;
  }

  async function saveProactivity(levelOrObj, quietHoursStart, quietHoursEnd) {
    let level = proactivityState.value.level;
    let qStart = proactivityState.value.quietHoursStart || '22:00';
    let qEnd = proactivityState.value.quietHoursEnd || '08:00';

    if (levelOrObj && typeof levelOrObj === 'object') {
      if (levelOrObj.level !== undefined) level = Number(levelOrObj.level);
      if (levelOrObj.quietHoursStart !== undefined) qStart = levelOrObj.quietHoursStart;
      if (levelOrObj.quietHoursEnd !== undefined) qEnd = levelOrObj.quietHoursEnd;
    } else if (levelOrObj !== undefined) {
      level = Number(levelOrObj);
      if (quietHoursStart !== undefined) qStart = quietHoursStart;
      if (quietHoursEnd !== undefined) qEnd = quietHoursEnd;
    }

    proactivityState.value = {
      level,
      quietHoursStart: qStart,
      quietHoursEnd: qEnd,
    };

    if (typeof ctx.kernelBridge?.proactivitySet === 'function') {
      await ctx.kernelBridge.proactivitySet({
        level,
        quietHoursStart: qStart,
        quietHoursEnd: qEnd,
      }).catch(() => null);
    }
    return proactivityState.value;
  }

  async function saveRoutine(cardId) {
    updateCardStatus(cardId, 'approved');
    if (typeof ctx.reload === 'function') {
      await ctx.reload();
    }
  }

  async function runJarvisTurn({ sessionId, requestId, messageId, content, onDelta }) {
    jarvisStreaming.value = true;
    jarvisSpeaking.value = false;
    streamingReplyText.value = '';
    streamingSessionId.value = sessionId;

    try {
      if (isBobNodeRuntime()) {
        try {
          const res = await streamJarvisTurn(
            { sessionId, requestId, messageId, content },
            evt => {
              if (!evt || typeof evt !== 'object') return;
              const type = evt.type || evt.event;
              if (type === 'status') {
                if (evt.mood === 'speaking') {
                  jarvisSpeaking.value = true;
                } else if (evt.mood === 'thinking') {
                  jarvisSpeaking.value = false;
                }
              } else if (type === 'text_delta') {
                const delta = evt.text ?? evt.delta ?? '';
                streamingReplyText.value += delta;
                jarvisSpeaking.value = true;
                onDelta?.(streamingReplyText.value);
              } else if (type === 'card' && evt.card) {
                const targetKey = requestId || messageId;
                if (targetKey) {
                  if (!messageCards.value[targetKey]) {
                    messageCards.value[targetKey] = [];
                  }
                  messageCards.value[targetKey].push(evt.card);
                }
              } else if (type === 'approval_required' && evt.approval) {
                const norm = {
                  id: evt.approval.id,
                  kind: 'approval',
                  icon: 'shield',
                  title: evt.approval.title || 'Approval Required',
                  summary: evt.approval.summary || '',
                  riskLevel: evt.approval.risk_level || evt.approval.riskLevel || 'side_effect',
                  fields: evt.approval.fields || [],
                  technicalDetail: evt.approval.technical_detail || evt.approval.technicalDetail || '',
                  status: 'pending',
                };
                upsertApproval(norm);
              } else if (type === 'takeover_required' && evt.takeover) {
                const norm = {
                  id: evt.takeover.id,
                  kind: 'takeover',
                  icon: 'browser',
                  title: evt.takeover.title || 'Browser Takeover Required',
                  summary: evt.takeover.summary || '',
                  riskLevel: 'side_effect',
                  fields: evt.takeover.url ? [{ label: 'URL', value: evt.takeover.url }] : [],
                  status: 'pending',
                  url: evt.takeover.url,
                };
                upsertApproval(norm);
              } else if (type === 'done') {
                if (evt.message_id && requestId && messageCards.value[requestId]) {
                  messageCards.value[evt.message_id] = messageCards.value[requestId];
                }
              }
            },
          );
          return res || { id: requestId, ok: true };
        } catch (streamErr) {
          const msg = String(streamErr?.message || '');
          if (!msg.includes('HTTP_404') && !msg.includes('NOT_FOUND')) {
            throw streamErr;
          }
        }
      }

      // Fallback 1: kernelBridge.jarvisChat
      if (typeof ctx.kernelBridge?.jarvisChat === 'function') {
        try {
          return await ctx.kernelBridge.jarvisChat({ sessionId, requestId, messageId, content });
        } catch (chatErr) {
          const cmsg = String(chatErr?.message || '');
          if (!cmsg.includes('UNKNOWN_COMMAND') && !cmsg.includes('HTTP_400') && !cmsg.includes('NOT_FOUND')) {
            throw chatErr;
          }
        }
      }

      // Fallback 2: kernelBridge.assistantExecute
      if (typeof ctx.kernelBridge?.assistantExecute === 'function') {
        return await ctx.kernelBridge.assistantExecute(requestId);
      }

      return { id: requestId, ok: true };
    } finally {
      jarvisStreaming.value = false;
      jarvisSpeaking.value = false;
    }
  }

  const state = {
    hasProCluster,
    widgets,
    installedFeatures,
    customFeatureWidgets,
    trialFeatures,
    refreshFeatures,
    keepFeature,
    rollbackFeature,
    toggleFeature,
    callFeatureRoute,
    saveFeatureStorage,
    MAX_FEATURE_STORAGE_BYTES,
    editingWidgets,
    widgetSheetOpen,
    toggleWidgetEnabled,
    moveWidget,
    resetWidgets,
    visibleWidgets,
    jarvisStreaming,
    jarvisSpeaking,
    streamingReplyText,
    streamingSessionId,
    messageCards,
    pendingApprovals,
    pendingApprovalCount,
    budgetState,
    orbMood,
    cardsForMessage,
    refreshJarvisState,
    approveAction,
    rejectAction,
    openTakeover,
    completeTakeover,
    closeTakeover,
    activeTakeover,
    siteSessions,
    refreshSiteSessions,
    openSiteForLogin,
    clearSiteSession,
    proactivityState,
    refreshProactivity,
    saveProactivity,
    saveRoutine,
    runJarvisTurn,
  };

  ctx.jarvisHome = state;
  return state;
}
