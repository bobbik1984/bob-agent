<script setup>
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue';
import HomeConnection from './HomeConnection.vue';
import ScheduleView from './views/ScheduleView.vue';
import WorkView from './views/WorkView.vue';
import KnowledgeView from './views/KnowledgeView.vue';
import { bindViewport } from './viewport.mjs';
import { createLayoutState, getProviderLogo } from './layout-mode.mjs';
import { createOnlineWorkClient } from './online-client.mjs';
import brandLogo from './assets/bob-logo.svg';
import defaultRegistryData from './assets/model_providers.json';
import mobileModelsData from './assets/mobile_models.json';
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
const settingsView = ref('home'); // 'home' | 'device' | 'provider'
const settingsTab = ref('model'); // 'model' | 'connection' | 'workspace' | 'tasks' | 'appearance' | 'about'
const navigationOpen = ref(false);
const devicePanel = ref(null);
const deviceCapabilities = ref(null);
const personalOps = ref({
  schedules: [],
  notes: [],
  captures: [],
  memories: [],
  decisions: [],
  pending_sync_count: 0,
});
const personalBusy = ref(false);
const personalFault = ref(null);

const onlineClient = createOnlineWorkClient();
const onlineState = reactive({
  origin: 'https://agent.bobbik.org',
  clientInstanceId: `bob-client-${Math.random().toString(36).slice(2, 10)}-instance`,
  pairing: null,
  token: null,
  connected: false,
  capabilities: null,
  projects: [],
  selectedProjectId: null,
  tasks: [],
  messages: [],
  busy: false,
  error: null,
});

// Legacy-aligned ModelHub state (13 providers, 199 visible models)
function cloneDefaultRegistry() {
  try {
    return JSON.parse(JSON.stringify(defaultRegistryData));
  } catch {
    return { version: '1.1.0', providers: [] };
  }
}

const modelHub = ref({
  registry: cloneDefaultRegistry(),
  state: {
    active_roles: {
      main: {
        provider_id: 'deepseek',
        model_id: 'deepseek-chat',
        model_name: 'DeepSeek V4 Flash',
        provider_name: 'DeepSeek',
      },
      clerk: {
        provider_id: 'doubao',
        model_id: 'doubao-seed-1-6-flash-250828',
        model_name: 'Doubao Seed 1.6 Flash',
        provider_name: '字节火山',
      },
      vision: {
        provider_id: 'gemini',
        model_id: 'gemini-2.5-flash',
        model_name: 'Gemini 2.5 Flash',
        provider_name: 'Google Gemini',
      },
    },
    provider_variants: {
      qwen: 'default',
      minimax: 'default',
      zhipu: 'default',
      kimi: 'default',
    },
    custom_models: [],
  },
  configured_keys: {
    deepseek: false,
    minimax: false,
    qwen: false,
    zhipu: false,
    kimi: false,
    doubao: false,
    openai: false,
    gemini: false,
    anthropic: false,
    modelscope: false,
    openrouter: false,
    tavily: false,
    tinyfish: false,
  },
  available_model_count: 199,
  active_profile: null,
});

const mobileModels = ref(Array.isArray(mobileModelsData) ? mobileModelsData : []);
const localRepoExpanded = ref(false);
const localMirrorSource = ref('hf'); // 'hf' | 'cn'
const apiKeysVaultExpanded = ref(true);
const registryExpanded = ref(false);
const editingRole = ref(null); // 'main' | 'clerk' | 'vision' | null
const selectorProviderId = ref('deepseek');
const modelSearchQuery = ref('');
const quickSwitcherOpen = ref(false);
const quickSwitcherProviderId = ref('deepseek');
const keyInputs = reactive({});
const customModelDraft = reactive({
  name: '',
  model_id: '',
  base_url: '',
  api_key: '',
});

const modelProviderKeyList = [
  { id: 'deepseek', name: 'DeepSeek' },
  { id: 'minimax', name: 'MiniMax' },
  { id: 'qwen', name: '阿里云百炼' },
  { id: 'zhipu', name: '智谱 AI' },
  { id: 'kimi', name: '月之暗面' },
  { id: 'doubao', name: '字节火山' },
  { id: 'openai', name: 'OpenAI' },
  { id: 'gemini', name: 'Google Gemini' },
  { id: 'anthropic', name: 'Anthropic Claude' },
  { id: 'modelscope', name: 'ModelScope' },
  { id: 'openrouter', name: 'OpenRouter' },
];

const toolServiceKeyList = [
  { id: 'tavily', name: 'Tavily Search' },
  { id: 'tinyfish', name: 'TinyFish Web Agent' },
];

const totalAvailableModelsCount = computed(() => {
  const providers = modelHub.value?.registry?.providers || [];
  let total = 0;
  for (const p of providers) {
    for (const m of p.models || []) {
      if (m.visible !== false) total += 1;
    }
  }
  total += (modelHub.value?.state?.custom_models || []).length;
  return total || modelHub.value?.available_model_count || 199;
});

const roleCards = computed(() => {
  const roles = modelHub.value?.state?.active_roles || {};
  return [
    {
      key: 'main',
      title: t('roleMainTitle'),
      desc: t('roleMainDesc'),
      icon: 'monitor',
      assignment: roles.main || {
        provider_id: 'deepseek',
        model_id: 'deepseek-chat',
        model_name: 'DeepSeek V4 Flash',
        provider_name: 'DeepSeek',
      },
    },
    {
      key: 'clerk',
      title: t('roleClerkTitle'),
      desc: t('roleClerkDesc'),
      icon: 'clerk',
      assignment: roles.clerk || {
        provider_id: 'doubao',
        model_id: 'doubao-seed-1-6-flash-250828',
        model_name: 'Doubao Seed 1.6 Flash',
        provider_name: '字节火山',
      },
    },
    {
      key: 'vision',
      title: t('roleVisionTitle'),
      desc: t('roleVisionDesc'),
      icon: 'vision',
      assignment: roles.vision || {
        provider_id: 'gemini',
        model_id: 'gemini-2.5-flash',
        model_name: 'Gemini 2.5 Flash',
        provider_name: 'Google Gemini',
      },
    },
  ];
});

const activeMainRole = computed(
  () =>
    modelHub.value?.state?.active_roles?.main || {
      provider_id: 'deepseek',
      model_id: 'deepseek-chat',
      model_name: 'DeepSeek V4 Flash',
      provider_name: 'DeepSeek',
    },
);

const selectorProviders = computed(() => {
  const list = [...(modelHub.value?.registry?.providers || [])];
  if ((modelHub.value?.state?.custom_models || []).length > 0) {
    list.push({
      id: 'custom',
      name: t('customModelsSectionTitle'),
      base_url: '',
      models: modelHub.value.state.custom_models.map(cm => ({
        id: cm.model_id,
        name: `${cm.name} (${cm.model_id})`,
        visible: true,
        custom_id: cm.id,
      })),
    });
  }
  return list;
});

const currentSelectorProvider = computed(
  () =>
    selectorProviders.value.find(p => p.id === selectorProviderId.value) ||
    selectorProviders.value[0] ||
    null,
);

const currentSelectorModels = computed(() => {
  const prov = currentSelectorProvider.value;
  if (!prov) return [];
  const q = modelSearchQuery.value.trim().toLowerCase();
  return (prov.models || []).filter(m => {
    if (m.visible === false) return false;
    if (!q) return true;
    return (
      (m.name && m.name.toLowerCase().includes(q)) ||
      (m.id && m.id.toLowerCase().includes(q))
    );
  });
});

const quickSwitcherProvider = computed(
  () =>
    selectorProviders.value.find(p => p.id === quickSwitcherProviderId.value) ||
    selectorProviders.value[0] ||
    null,
);

const quickSwitcherModels = computed(() => {
  const prov = quickSwitcherProvider.value;
  if (!prov) return [];
  return (prov.models || []).filter(m => m.visible !== false);
});

function isKeyConfigured(providerId) {
  return Boolean(modelHub.value?.configured_keys?.[providerId]);
}

function toggleRoleEditor(roleKey) {
  if (editingRole.value === roleKey) {
    editingRole.value = null;
    return;
  }
  editingRole.value = roleKey;
  const current = modelHub.value?.state?.active_roles?.[roleKey];
  if (current?.provider_id) {
    selectorProviderId.value = current.provider_id;
  }
  modelSearchQuery.value = '';
}

const providerProfile = ref(null);
const providerForm = ref({
  providerKind: 'openai_compatible',
  label: 'OpenAI',
  baseUrl: 'https://api.openai.com/v1',
  modelId: 'gpt-4o-mini',
  apiKey: '',
  enabled: true,
});
const executionRoute = ref('auto');
const providerBusy = ref(false);

function syncProviderForm(profile) {
  providerProfile.value = profile || null;
  if (!profile) return;
  providerForm.value.providerKind = profile.provider_kind || 'openai_compatible';
  providerForm.value.label = profile.label || 'OpenAI';
  providerForm.value.baseUrl = profile.base_url || 'https://api.openai.com/v1';
  providerForm.value.modelId = profile.model_id || 'gpt-4o-mini';
  providerForm.value.apiKey = '';
  providerForm.value.enabled = Boolean(profile.enabled);
}

function resolveEntryToRoleObj(entryOrObj, registryProviders) {
  if (!entryOrObj) return null;
  if (typeof entryOrObj === 'object' && entryOrObj.provider_id) {
    return entryOrObj;
  }
  const str = String(entryOrObj);
  const [provider_id, model_id] = str.includes('::')
    ? str.split('::', 2)
    : ['deepseek', str];
  const prov = (registryProviders || []).find(p => p.id === provider_id);
  const mdl = (prov?.models || []).find(m => m.id === model_id);
  return {
    provider_id,
    model_id,
    model_name: mdl?.name || model_id,
    provider_name: prov?.name || provider_id,
  };
}

function applyModelHubSnapshot(snap) {
  if (!snap) return;
  if (snap.registry?.providers?.length) {
    modelHub.value.registry = snap.registry;
  }
  const providers = modelHub.value.registry?.providers || [];
  if (snap.state) {
    modelHub.value.state = snap.state;
  }
  const rawRoles = snap.activeRoles || snap.state?.active_roles;
  if (rawRoles) {
    for (const rKey of ['main', 'clerk', 'vision']) {
      const resolved = resolveEntryToRoleObj(rawRoles[rKey], providers);
      if (resolved) {
        modelHub.value.state.active_roles[rKey] = resolved;
      }
    }
  }
  if (snap.providerVariants) {
    modelHub.value.state.provider_variants = {
      ...modelHub.value.state.provider_variants,
      ...snap.providerVariants,
    };
  }
  if (Array.isArray(snap.customModels)) {
    modelHub.value.state.custom_models = snap.customModels.map(cm => ({
      id: cm.id,
      name: cm.displayName || cm.name || cm.modelId,
      model_id: cm.modelId || cm.model_id,
      base_url: cm.baseUrl || cm.base_url,
    }));
  }
  if (snap.configured_keys || snap.providerKeys || snap.toolKeys) {
    modelHub.value.configured_keys = {
      ...modelHub.value.configured_keys,
      ...(snap.configured_keys || {}),
      ...(snap.providerKeys || {}),
      ...(snap.toolKeys || {}),
    };
  }
  if (typeof snap.totalAvailableCount === 'number') {
    modelHub.value.available_model_count = snap.totalAvailableCount;
  } else if (typeof snap.available_model_count === 'number') {
    modelHub.value.available_model_count = snap.available_model_count;
  }
  if (snap.active_profile) {
    modelHub.value.active_profile = snap.active_profile;
    syncProviderForm(snap.active_profile);
  }
}

async function loadModelHub() {
  if (available && kernelBridge.modelHubGet) {
    const snap = await kernelBridge.modelHubGet().catch(() => null);
    if (snap) {
      applyModelHubSnapshot(snap);
      return;
    }
  }
  if (available && kernelBridge.providerGet) {
    const p = await kernelBridge.providerGet().catch(() => null);
    if (p) {
      syncProviderForm(p);
      if (p.api_key_configured) {
        modelHub.value.configured_keys.openai = true;
      }
    }
  }
}

async function saveProviderKey(providerId) {
  const rawKey = (keyInputs[providerId] || '').trim();
  if (!rawKey || providerBusy.value) return;
  providerBusy.value = true;
  try {
    if (available && kernelBridge.providerKeySet) {
      const snap = await kernelBridge
        .providerKeySet(providerId, rawKey, false)
        .catch(() => null);
      if (snap) {
        applyModelHubSnapshot(snap);
      } else {
        modelHub.value.configured_keys[providerId] = true;
      }
    } else {
      modelHub.value.configured_keys[providerId] = true;
    }
    keyInputs[providerId] = '';
    await reload();
  } finally {
    providerBusy.value = false;
  }
}

async function clearProviderKey(providerId) {
  if (providerBusy.value) return;
  providerBusy.value = true;
  try {
    if (available && kernelBridge.providerKeySet) {
      const snap = await kernelBridge
        .providerKeySet(providerId, null, true)
        .catch(() => null);
      if (snap) {
        applyModelHubSnapshot(snap);
      } else {
        modelHub.value.configured_keys[providerId] = false;
      }
    } else {
      modelHub.value.configured_keys[providerId] = false;
    }
    keyInputs[providerId] = '';
    await reload();
  } finally {
    providerBusy.value = false;
  }
}

async function selectRoleModel(roleKey, providerObj, modelObj) {
  if (!providerObj || !modelObj) return;
  const payload = {
    role: roleKey,
    providerId: providerObj.id,
    modelId: modelObj.id,
    modelName: modelObj.name || modelObj.id,
    providerName: providerObj.name || providerObj.id,
  };
  modelHub.value.state.active_roles[roleKey] = {
    provider_id: payload.providerId,
    model_id: payload.modelId,
    model_name: payload.modelName,
    provider_name: payload.providerName,
  };
  if (available && kernelBridge.modelRoleAssign) {
    const snap = await kernelBridge.modelRoleAssign(payload).catch(() => null);
    if (snap) applyModelHubSnapshot(snap);
  }
  editingRole.value = null;
  quickSwitcherOpen.value = false;
  await reload();
}

async function setProviderVariant(providerId, variant) {
  if (!modelHub.value.state.provider_variants) {
    modelHub.value.state.provider_variants = {};
  }
  modelHub.value.state.provider_variants[providerId] = variant;
  if (available && kernelBridge.providerVariantSet) {
    const snap = await kernelBridge.providerVariantSet(providerId, variant).catch(() => null);
    if (snap) applyModelHubSnapshot(snap);
  }
}

async function addCustomModel() {
  const name = customModelDraft.name.trim();
  const model_id = customModelDraft.model_id.trim();
  const base_url = customModelDraft.base_url.trim();
  const api_key = customModelDraft.api_key.trim();
  if (!name || !model_id || !base_url) return;
  const id = `custom-${Date.now()}`;
  if (available && kernelBridge.customModelSave) {
    const snap = await kernelBridge
      .customModelSave({
        id,
        name,
        modelId: model_id,
        baseUrl: base_url,
        apiKey: api_key || null,
      })
      .catch(() => null);
    if (snap) applyModelHubSnapshot(snap);
  } else {
    modelHub.value.state.custom_models.push({ id, name, model_id, base_url });
  }
  customModelDraft.name = '';
  customModelDraft.model_id = '';
  customModelDraft.base_url = '';
  customModelDraft.api_key = '';
}

async function removeCustomModel(id) {
  if (available && kernelBridge.customModelDelete) {
    const snap = await kernelBridge.customModelDelete(id).catch(() => null);
    if (snap) applyModelHubSnapshot(snap);
  } else {
    modelHub.value.state.custom_models = (modelHub.value.state.custom_models || []).filter(
      m => m.id !== id,
    );
  }
}

function onProviderKindChange() {
  const kind = providerForm.value.providerKind;
  if (kind === 'anthropic_messages') {
    if (providerForm.value.baseUrl.includes('openai') || providerForm.value.baseUrl.includes('googleapis')) {
      providerForm.value.baseUrl = 'https://api.anthropic.com/v1';
    }
    if (providerForm.value.modelId.startsWith('gpt-') || providerForm.value.modelId.startsWith('gemini-')) {
      providerForm.value.modelId = 'claude-3-5-sonnet-latest';
    }
    if (providerForm.value.label === 'OpenAI' || providerForm.value.label === 'Gemini') {
      providerForm.value.label = 'Anthropic';
    }
  } else if (kind === 'gemini_generate_content') {
    if (providerForm.value.baseUrl.includes('openai') || providerForm.value.baseUrl.includes('anthropic')) {
      providerForm.value.baseUrl = 'https://generativelanguage.googleapis.com/v1beta';
    }
    if (providerForm.value.modelId.startsWith('gpt-') || providerForm.value.modelId.startsWith('claude-')) {
      providerForm.value.modelId = 'gemini-2.5-flash';
    }
    if (providerForm.value.label === 'OpenAI' || providerForm.value.label === 'Anthropic') {
      providerForm.value.label = 'Gemini';
    }
  } else {
    if (providerForm.value.baseUrl.includes('anthropic') || providerForm.value.baseUrl.includes('googleapis')) {
      providerForm.value.baseUrl = 'https://api.openai.com/v1';
    }
    if (providerForm.value.modelId.startsWith('claude-') || providerForm.value.modelId.startsWith('gemini-')) {
      providerForm.value.modelId = 'gpt-4o-mini';
    }
    if (providerForm.value.label === 'Anthropic' || providerForm.value.label === 'Gemini') {
      providerForm.value.label = 'OpenAI';
    }
  }
}

async function openProviderSettings() {
  settingsView.value = 'provider';
  settingsTab.value = 'model';
  settingsOpen.value = true;
  await loadModelHub();
  if (available && sessionId.value && kernelBridge.routeGet) {
    const r = await kernelBridge.routeGet(sessionId.value).catch(() => null);
    if (r) executionRoute.value = r;
  }
}

async function saveProviderSettings(clearKey = false) {
  if (!available || !kernelBridge.providerSave || providerBusy.value) return;
  providerBusy.value = true;
  try {
    const saved = await kernelBridge.providerSave({
      providerKind: providerForm.value.providerKind,
      label: providerForm.value.label,
      baseUrl: providerForm.value.baseUrl,
      modelId: providerForm.value.modelId,
      apiKey: clearKey ? null : providerForm.value.apiKey || null,
      clearKey,
      enabled: providerForm.value.enabled,
    });
    syncProviderForm(saved);
    if (sessionId.value && kernelBridge.routeSet) {
      await kernelBridge.routeSet(sessionId.value, executionRoute.value).catch(() => {});
    }
    await reload();
  } finally {
    providerBusy.value = false;
  }
}

async function testProviderConnection() {
  if (!available || !kernelBridge.providerTest || providerBusy.value) return;
  await saveProviderSettings(false);
  providerBusy.value = true;
  try {
    const checked = await kernelBridge.providerTest();
    syncProviderForm(checked);
    await reload();
  } finally {
    providerBusy.value = false;
  }
}

async function changeExecutionRoute() {
  if (!available || !sessionId.value || !kernelBridge.routeSet) return;
  await kernelBridge.routeSet(sessionId.value, executionRoute.value).catch(() => {});
  await reload();
}

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

async function loadPersonalOps() {
  if (!available || !kernelBridge.personalSnapshot) return;
  const snap = await kernelBridge.personalSnapshot(null).catch(() => null);
  if (snap) {
    personalOps.value = {
      schedules: Array.isArray(snap.schedules) ? snap.schedules : [],
      notes: Array.isArray(snap.notes) ? snap.notes : [],
      captures: Array.isArray(snap.captures) ? snap.captures : [],
      memories: Array.isArray(snap.memories) ? snap.memories : [],
      decisions: Array.isArray(snap.decisions) ? snap.decisions : [],
      pending_sync_count: Number(snap.pending_sync_count) || 0,
    };
  }
}

async function handleCreateSchedule(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.scheduleCreate) {
      await kernelBridge.scheduleCreate({
        id: payload.id,
        title: payload.title,
        kind: payload.kind ?? 'event',
        startAt: payload.startAt ?? payload.dueAtMs ?? Date.now(),
        notes: payload.notes,
      });
      await loadPersonalOps();
    } else {
      personalOps.value.schedules.unshift({
        id: payload.id,
        title: payload.title,
        kind: payload.kind ?? 'event',
        start_at: payload.startAt ?? payload.dueAtMs ?? Date.now(),
        status: 'scheduled',
        notes: payload.notes,
      });
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'SCHEDULE_CREATE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleUpdateScheduleStatus(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.scheduleStatus) {
      await kernelBridge.scheduleStatus(payload.id, payload.status);
      await loadPersonalOps();
    } else {
      const target = personalOps.value.schedules.find(s => s.id === payload.id);
      if (target) target.status = payload.status;
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'SCHEDULE_UPDATE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleSaveNote(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.noteUpsert) {
      await kernelBridge.noteUpsert({
        id: payload.id,
        title: payload.title,
        body: payload.body,
        tags: payload.tags,
      });
      await loadPersonalOps();
    } else {
      const idx = personalOps.value.notes.findIndex(n => n.id === payload.id);
      if (idx >= 0) personalOps.value.notes[idx] = { ...payload };
      else personalOps.value.notes.unshift({ ...payload });
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'NOTE_SAVE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleCreateCapture(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.captureCreate) {
      await kernelBridge.captureCreate({
        id: payload.id,
        conversationId: sessionId.value,
        messageId: null,
        content: payload.content,
      });
      await loadPersonalOps();
    } else {
      personalOps.value.captures.unshift({
        id: payload.id,
        content_ref: payload.content,
      });
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'CAPTURE_SAVE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleUpdateMemory(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.memoryState) {
      await kernelBridge.memoryState(payload.id, payload.state);
      await loadPersonalOps();
    } else {
      const target = personalOps.value.memories.find(m => m.id === payload.id);
      if (target) target.state = payload.state;
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'MEMORY_UPDATE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleCreateDecision(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.decisionRecord) {
      await kernelBridge.decisionRecord({
        id: payload.id,
        body: payload.body,
        status: payload.status ?? 'accepted',
      });
      await loadPersonalOps();
    } else {
      personalOps.value.decisions.unshift({
        id: payload.id,
        project_id: 'proj-local',
        body: payload.body,
        status: payload.status ?? 'accepted',
      });
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'DECISION_SAVE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function handleUpdateDecision(payload) {
  if (personalBusy.value) return;
  personalBusy.value = true;
  personalFault.value = null;
  try {
    if (available && kernelBridge.decisionStatus) {
      await kernelBridge.decisionStatus(payload.id, payload.status);
      await loadPersonalOps();
    } else {
      const target = personalOps.value.decisions.find(d => d.id === payload.id);
      if (target) target.status = payload.status;
    }
  } catch (err) {
    personalFault.value = String(err?.message || err || 'DECISION_UPDATE_FAILED');
  } finally {
    personalBusy.value = false;
  }
}

async function promoteConversationToWork() {
  if (!available || !kernelBridge.createProject) {
    activeDrawer.value = 'work';
    return;
  }
  personalFault.value = null;
  try {
    const title = currentConversation.value || t('newProject');
    const pid = `proj-${crypto.randomUUID().slice(0, 8)}`;
    const snap = await kernelBridge.createProject(pid, title);
    if (snap) applyCatalogPreferences(snap);
    if (kernelBridge.createConversation) {
      const cid = `conv-${crypto.randomUUID().slice(0, 8)}`;
      const snap2 = await kernelBridge.createConversation(cid, pid, title);
      if (snap2) applyCatalogPreferences(snap2);
    }
    activeDrawer.value = 'work';
  } catch (err) {
    personalFault.value = String(err?.message || err || 'PROJECT_CREATE_FAILED');
  }
}

function handleOnlineUpdateOrigin(val) {
  onlineState.origin = val;
}

async function handleOnlineBeginPairing() {
  if (onlineState.busy) return;
  onlineState.busy = true;
  onlineState.error = null;
  try {
    onlineState.pairing = await onlineClient.beginPairing({
      origin: onlineState.origin,
      clientInstanceId: onlineState.clientInstanceId,
      clientName: 'Bob Cross-Platform Client',
    });
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
    const res = await onlineClient.exchangePairing({
      origin: onlineState.pairing.origin || onlineState.origin,
      pairingId: onlineState.pairing.pairing_id,
      clientInstanceId: onlineState.clientInstanceId,
      verifier: onlineState.pairing.verifier,
    });
    if (!res.pending && res.access_token) {
      onlineState.token = res.access_token;
      onlineState.connected = true;
      await handleOnlineRefresh();
    }
  } catch (err) {
    onlineState.error = String(err?.message || err || 'ONLINE_EXCHANGE_FAILED');
  } finally {
    onlineState.busy = false;
  }
}

async function handleOnlineRefresh() {
  if (!onlineState.token) return;
  onlineState.busy = true;
  onlineState.error = null;
  try {
    const [caps, projs] = await Promise.all([
      onlineClient.fetchCapabilities({ origin: onlineState.origin, token: onlineState.token }),
      onlineClient.fetchProjects({ origin: onlineState.origin, token: onlineState.token }),
    ]);
    onlineState.capabilities = caps;
    onlineState.projects = Array.isArray(projs?.items) ? projs.items : [];
    if (onlineState.projects.length > 0 && !onlineState.selectedProjectId) {
      await handleOnlineSelectProject(onlineState.projects[0].id);
    }
  } catch (err) {
    onlineState.error = String(err?.message || err || 'ONLINE_REFRESH_FAILED');
  } finally {
    onlineState.busy = false;
  }
}

async function handleOnlineSelectProject(projectId) {
  if (!onlineState.token || !projectId) return;
  onlineState.selectedProjectId = projectId;
  try {
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
  } catch (err) {
    onlineState.error = String(err?.message || err || 'ONLINE_PROJECT_READ_FAILED');
  }
}

async function handleOnlineRevoke() {
  if (!onlineState.token) return;
  onlineState.busy = true;
  try {
    await onlineClient.revokeSelf({ origin: onlineState.origin, token: onlineState.token });
  } catch {
    // Clear local session token even if remote already expired/revoked
  } finally {
    onlineState.token = null;
    onlineState.connected = false;
    onlineState.pairing = null;
    onlineState.projects = [];
    onlineState.tasks = [];
    onlineState.messages = [];
    onlineState.busy = false;
  }
}
const handleOnlineDisconnect = handleOnlineRevoke;

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

async function switchPrimaryDrawer(drawer) {
  navigationOpen.value = false;
  if (drawer === 'settings') {
    toggleSettingsHome();
    return;
  }
  previousNonSettingsDrawer.value = drawer;
  activeDrawer.value = drawer;
  settingsOpen.value = false;
  if (drawer === 'schedule' || drawer === 'knowledge') {
    await loadPersonalOps();
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
    void loadModelHub();
    void loadPersonalOps();
  }
}

async function switchSettingsTab(tab) {
  settingsTab.value = tab;
  if (tab === 'connection') {
    await openDevices();
  } else if (tab === 'model') {
    settingsView.value = 'home';
    await loadModelHub();
  } else if (tab === 'workspace') {
    settingsView.value = 'home';
    await loadPersonalOps();
  } else if (tab === 'tasks') {
    settingsView.value = 'home';
    if (available && sessionId.value && kernelBridge.routeGet) {
      const r = await kernelBridge.routeGet(sessionId.value).catch(() => null);
      if (r) executionRoute.value = r;
    }
  } else {
    settingsView.value = 'home';
  }
}

async function openDevices() {
  if (activeDrawer.value !== 'settings') {
    previousNonSettingsDrawer.value = activeDrawer.value;
  }
  activeDrawer.value = 'settings';
  settingsView.value = 'device';
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
    await loadModelHub();
    await loadPersonalOps();
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
  updateLayoutState();
  window.addEventListener('resize', updateLayoutState, { passive: true });
  await startup();
  if (available) {
    deviceCapabilities.value = await kernelBridge.pairingCapabilities().catch(() => null);
  }
  if (!disposed && available) {
    let idleTick = 0;
    poll = setInterval(() => {
      if (typeof document !== 'undefined' && document.hidden) return;
      const isIdleView =
        activeDrawer.value !== 'chat' && !activeExecution.value && !dispatchPending.value;
      if (isIdleView) {
        idleTick = (idleTick + 1) % 4;
        if (idleTick !== 0) return;
      } else {
        idleTick = 0;
      }
      void reload();
    }, 1500);
  }
});

onUnmounted(() => {
  disposed = true;
  if (draftSaveTimer) clearTimeout(draftSaveTimer);
  if (scrollSaveTimer) clearTimeout(scrollSaveTimer);
  if (typeof window !== 'undefined') {
    window.removeEventListener('resize', updateLayoutState);
  }
  releaseViewport();
  if (poll) clearInterval(poll);
});
</script>
<template>
  <div
    class="app-shell"
    :class="[
      `layout-${layoutState.layoutMode}`,
      `terminal-${layoutState.terminalKind}`,
      `aspect-${layoutState.aspect}`,
      { 'is-mobile': layoutState.isCompact },
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
        ☰
        <span v-if="totalUnreadCount > 0" class="unread-dot" aria-hidden="true"></span>
      </button>
      <span class="page-name">{{ currentConversation }}</span>
      <button
        v-if="activeDrawer === 'chat'"
        type="button"
        class="mini-ghost-btn"
        :title="t('promoteToWorkBtn')"
        @click="promoteConversationToWork"
      >
        💼 {{ t('promoteToWorkBtn') }}
      </button>
      <span class="layout-mode-badge" aria-hidden="true">
        {{
          t(
            layoutState.layoutMode === 'desktop-wide'
              ? 'layoutBadgeDesktop'
              : 'layoutBadgeMobile',
          )
        }}
      </span>
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
      :aria-label="
        t(
          settingsView === 'device'
            ? 'deviceAndConnection'
            : settingsView === 'provider'
              ? 'directModelProvider'
              : 'settings',
        )
      "
      @keydown="settingsKeydown"
    >
      <div class="settings-heading">
        <div class="settings-brand-row">
          <h2>
            {{
              t(
                settingsView === 'device'
                  ? 'deviceAndConnection'
                  : settingsView === 'provider'
                    ? 'directModelProvider'
                    : 'settings',
              )
            }}
          </h2>
        </div>
        <div class="settings-heading-actions">
          <button
            type="button"
            class="settings-close-btn"
            :aria-label="t('closeSettings')"
            @click="settingsOpen = false"
          >
            ×
          </button>
          <button
            v-if="settingsView !== 'home'"
            type="button"
            class="back-settings-btn"
            :aria-label="t('backToSettings')"
            @click="
              settingsView = 'home';
              settingsTab = 'model';
            "
          >
            ← {{ t('settings') }}
          </button>
        </div>
      </div>

      <!-- Legacy-aligned Top 6-Tab Bar: 模型 | 连接 | 工作间 | 任务 | 外观 | 关于 -->
      <nav class="settings-top-tabs" role="tablist">
        <button
          type="button"
          class="settings-top-tab"
          :class="{ active: settingsTab === 'model' && settingsView !== 'device' }"
          @click="switchSettingsTab('model')"
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <rect x="6" y="6" width="12" height="12" rx="2" />
            <path d="M9 1v3M15 1v3M9 20v3M15 20v3M20 9h3M20 14h3M1 9h3M1 14h3" />
          </svg>
          <span>{{ t('tabModel') }}</span>
        </button>
        <button
          type="button"
          class="settings-top-tab"
          :class="{ active: settingsTab === 'connection' || settingsView === 'device' }"
          @click="switchSettingsTab('connection')"
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 22v-5M9 8V2M15 8V2M18 8v5a6 6 0 0 1-12 0V8Z" />
          </svg>
          <span>{{ t('tabConnection') }}</span>
        </button>
        <button
          type="button"
          class="settings-top-tab"
          :class="{ active: settingsTab === 'workspace' && settingsView === 'home' }"
          @click="switchSettingsTab('workspace')"
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
          </svg>
          <span>{{ t('tabWorkspace') }}</span>
        </button>
        <button
          type="button"
          class="settings-top-tab"
          :class="{ active: settingsTab === 'tasks' && settingsView === 'home' }"
          @click="switchSettingsTab('tasks')"
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 2L15 8l6 1-4.5 4.5L18 20l-6-3-6 3 1.5-6.5L3 9l6-1z" />
          </svg>
          <span>{{ t('tabTasks') }}</span>
        </button>
        <button
          type="button"
          class="settings-top-tab"
          :class="{ active: settingsTab === 'appearance' && settingsView === 'home' }"
          @click="switchSettingsTab('appearance')"
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="9" />
            <circle cx="9" cy="10" r="1.2" />
            <circle cx="15" cy="10" r="1.2" />
            <circle cx="12" cy="15" r="1.2" />
          </svg>
          <span>{{ t('tabAppearance') }}</span>
        </button>
        <button
          type="button"
          class="settings-top-tab"
          :class="{ active: settingsTab === 'about' && settingsView === 'home' }"
          @click="switchSettingsTab('about')"
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="10" />
            <path d="M12 16v-4M12 8h.01" />
          </svg>
          <span>{{ t('tabAbout') }}</span>
        </button>
      </nav>

      <div class="settings-scroll-container">
        <!-- Tab 1: 模型 (Model Center + Local Models + API Key Management + Registry) -->
        <div
          v-show="(settingsView === 'home' && settingsTab === 'model') || settingsView === 'provider'"
          class="model-hub-stack"
        >
          <!-- Card 1: 模型中心 (199 个可用模型) -->
          <section class="hub-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <svg class="hub-icon" viewBox="0 0 24 24" aria-hidden="true">
                  <rect x="5" y="5" width="14" height="14" rx="2" />
                  <path d="M9 1v4M15 1v4M9 19v4M15 19v4M1 9h4M1 15h4M19 9h4M19 15h4" />
                </svg>
                <h3>{{ t('modelCenterTitle') }}</h3>
                <span class="hub-count-pill">
                  {{ t('availableModelsBadge', { count: totalAvailableModelsCount }) }}
                </span>
              </div>
              <button
                type="button"
                class="hub-icon-btn"
                :title="t('refreshRegistry')"
                @click="loadModelHub"
              >
                ↻
              </button>
            </div>

            <div class="role-cards-grid">
              <button
                v-for="role in roleCards"
                :key="role.key"
                type="button"
                class="role-card"
                :class="{ active: editingRole === role.key }"
                @click="toggleRoleEditor(role.key)"
              >
                <div class="role-card-icon">
                  <svg v-if="role.icon === 'monitor'" viewBox="0 0 24 24" aria-hidden="true">
                    <rect x="3" y="4" width="18" height="12" rx="2" />
                    <path d="M8 20h8M12 16v4" />
                  </svg>
                  <svg v-else-if="role.icon === 'clerk'" viewBox="0 0 24 24" aria-hidden="true">
                    <circle cx="8" cy="17" r="3" />
                    <circle cx="17" cy="17" r="3" />
                    <path d="M5 17H3V9l5-3h9v11" />
                  </svg>
                  <svg v-else viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6-10-6-10-6Z" />
                    <circle cx="12" cy="12" r="3" />
                  </svg>
                </div>
                <div class="role-card-body">
                  <span class="role-card-label">{{ role.title }}</span>
                  <span class="role-card-provider">
                    <img
                      :src="getProviderLogo(role.assignment.provider_id)"
                      alt=""
                      class="role-mini-logo"
                    />
                    {{ role.assignment.provider_name || role.assignment.provider_id }}
                  </span>
                  <strong class="role-card-model">
                    {{ role.assignment.model_name || role.assignment.model_id }}
                  </strong>
                </div>
              </button>
            </div>

            <!-- Interactive Role Model Pool Selector -->
            <div v-if="editingRole" class="role-selector-drawer">
              <div class="role-selector-toolbar">
                <label class="role-selector-field">
                  <span>{{ t('selectProviderLabel') }}</span>
                  <select v-model="selectorProviderId">
                    <option
                      v-for="prov in selectorProviders"
                      :key="prov.id"
                      :value="prov.id"
                    >
                      {{ prov.name }}
                      ({{ (prov.models || []).filter(m => m.visible !== false).length }})
                      {{ isKeyConfigured(prov.id) ? '●' : '' }}
                    </option>
                  </select>
                </label>

                <label
                  v-if="['qwen', 'minimax', 'zhipu', 'kimi'].includes(selectorProviderId)"
                  class="role-selector-field"
                >
                  <span>
                    {{
                      selectorProviderId === 'qwen'
                        ? t('variantCodingLabel')
                        : t('variantRegionLabel')
                    }}
                  </span>
                  <select
                    :value="modelHub.state.provider_variants?.[selectorProviderId] || 'default'"
                    @change="setProviderVariant(selectorProviderId, $event.target.value)"
                  >
                    <option value="default">{{ t('variantDefault') }}</option>
                    <option value="international">{{ t('variantInternational') }}</option>
                    <option value="coding">{{ t('variantCoding') }}</option>
                  </select>
                </label>

                <label class="role-selector-field role-search-field">
                  <span>{{
                    t('selectModelFromPool', { count: currentSelectorModels.length })
                  }}</span>
                  <input
                    v-model="modelSearchQuery"
                    type="search"
                    :placeholder="t('modelFilterPlaceholder')"
                  />
                </label>
              </div>

              <div class="model-pool-grid">
                <button
                  v-for="mdl in currentSelectorModels"
                  :key="mdl.id"
                  type="button"
                  class="model-pool-chip"
                  :class="{
                    selected:
                      modelHub.state.active_roles?.[editingRole]?.model_id === mdl.id &&
                      modelHub.state.active_roles?.[editingRole]?.provider_id ===
                        selectorProviderId,
                  }"
                  @click="selectRoleModel(editingRole, currentSelectorProvider, mdl)"
                >
                  <img
                    :src="getProviderLogo(selectorProviderId)"
                    alt=""
                    class="chip-logo"
                  />
                  <div class="chip-text">
                    <strong>{{ mdl.name || mdl.id }}</strong>
                    <small>{{ mdl.id }}</small>
                  </div>
                </button>
              </div>
            </div>
          </section>

          <!-- Card 2: 本地模型 (Local Models) -->
          <section class="hub-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <svg class="hub-icon" viewBox="0 0 24 24" aria-hidden="true">
                  <rect x="3" y="4" width="18" height="6" rx="1.5" />
                  <rect x="3" y="14" width="18" height="6" rx="1.5" />
                  <circle cx="7" cy="7" r="1" />
                  <circle cx="7" cy="17" r="1" />
                </svg>
                <h3>{{ t('localModelsTitle') }}</h3>
              </div>
              <span class="local-engine-status">
                {{ t('localEngineSleeping') }} <span class="hollow-dot"></span>
              </span>
            </div>

            <button
              type="button"
              class="local-repo-bar"
              @click="localRepoExpanded = !localRepoExpanded"
            >
              <span class="local-repo-label">
                <svg class="hub-icon-sm" viewBox="0 0 24 24" aria-hidden="true">
                  <ellipse cx="12" cy="5" rx="8" ry="3" />
                  <path d="M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5" />
                  <path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" />
                </svg>
                {{ t('localModelRepoCount', { count: 0 }) }}
              </span>
              <span>{{ localRepoExpanded ? '⌃' : '⌄' }}</span>
            </button>

            <div v-if="localRepoExpanded" class="local-models-list">
              <div v-for="lm in mobileModels" :key="lm.id" class="local-model-item">
                <div class="local-model-info">
                  <strong>{{ lm.name }}</strong>
                  <span class="local-model-meta">{{ lm.size }} · {{ lm.ram }}</span>
                  <p>{{ lm.desc }}</p>
                </div>
                <span class="local-model-filename">{{ lm.filename }}</span>
              </div>
            </div>

            <div class="local-actions-row">
              <div class="local-action-pill">
                <strong :class="{ active: localMirrorSource === 'hf' }">HF</strong>
                <button
                  type="button"
                  class="mirror-switch"
                  :class="{ cn: localMirrorSource === 'cn' }"
                  @click="localMirrorSource = localMirrorSource === 'hf' ? 'cn' : 'hf'"
                >
                  <span class="mirror-knob"></span>
                </button>
                <strong :class="{ active: localMirrorSource === 'cn' }">CN</strong>
              </div>
              <button
                type="button"
                class="local-action-pill"
                disabled
                :title="t('comingSoonBadge')"
              >
                {{ t('customDownloadUrl') }} ({{ t('comingSoonBadge') }})
              </button>
              <button
                type="button"
                class="local-action-pill"
                disabled
                :title="t('comingSoonBadge')"
              >
                📂 {{ t('physicalModelDir') }} ({{ t('comingSoonBadge') }})
              </button>
            </div>
          </section>

          <!-- Card 3: API 密钥管理 (安全存储) -->
          <section class="hub-card">
            <div
              class="hub-card-header clickable"
              @click="apiKeysVaultExpanded = !apiKeysVaultExpanded"
            >
              <div class="hub-card-title-group">
                <svg class="hub-icon" viewBox="0 0 24 24" aria-hidden="true">
                  <circle cx="8" cy="15" r="4" />
                  <path d="M10.85 12.15 19 4M18 5l2 2M15 8l2 2" />
                </svg>
                <h3>{{ t('apiKeysVaultTitle') }}</h3>
              </div>
              <span class="collapse-chevron">{{ apiKeysVaultExpanded ? '⌃' : '⌄' }}</span>
            </div>

            <div v-show="apiKeysVaultExpanded" class="api-keys-vault-body">
              <div class="vault-section-subtitle">{{ t('modelProviderKeysSubtitle') }}</div>

              <div class="vault-key-list">
                <div
                  v-for="item in modelProviderKeyList"
                  :key="item.id"
                  class="vault-key-row"
                >
                  <div class="vault-key-brand">
                    <img
                      :src="getProviderLogo(item.id)"
                      alt=""
                      class="key-provider-logo"
                    />
                    <span class="key-provider-name">{{ item.name }}</span>
                  </div>

                  <span
                    class="key-status-dot"
                    :class="{ configured: isKeyConfigured(item.id) }"
                    :title="
                      isKeyConfigured(item.id)
                        ? t('keyInputConfigured')
                        : t('keyInputUnconfigured')
                    "
                  ></span>

                  <input
                    v-model="keyInputs[item.id]"
                    type="password"
                    class="vault-key-input"
                    autocomplete="off"
                    :placeholder="
                      isKeyConfigured(item.id)
                        ? t('keyInputConfigured')
                        : t('keyInputUnconfigured')
                    "
                    @keydown.enter.prevent="saveProviderKey(item.id)"
                  />

                  <button
                    v-if="isKeyConfigured(item.id) && !(keyInputs[item.id] || '').trim()"
                    type="button"
                    class="vault-key-action-btn delete"
                    :aria-label="`${t('deleteKeyAria')} ${item.name}`"
                    @click="clearProviderKey(item.id)"
                  >
                    ✕
                  </button>
                  <button
                    v-else
                    type="button"
                    class="vault-key-action-btn save"
                    :disabled="!(keyInputs[item.id] || '').trim() || providerBusy"
                    :aria-label="`${t('saveKeyAria')} ${item.name}`"
                    @click="saveProviderKey(item.id)"
                  >
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2Z" />
                      <polyline points="17 21 17 13 7 13 7 21" />
                      <polyline points="7 3 7 8 15 8" />
                    </svg>
                  </button>
                </div>
              </div>

              <div class="vault-section-subtitle">{{ t('toolServiceKeysSubtitle') }}</div>
              <div class="vault-key-list">
                <div
                  v-for="item in toolServiceKeyList"
                  :key="item.id"
                  class="vault-key-row"
                >
                  <div class="vault-key-brand">
                    <img
                      :src="getProviderLogo(item.id)"
                      alt=""
                      class="key-provider-logo"
                    />
                    <span class="key-provider-name">{{ item.name }}</span>
                  </div>

                  <span
                    class="key-status-dot"
                    :class="{ configured: isKeyConfigured(item.id) }"
                  ></span>

                  <input
                    v-model="keyInputs[item.id]"
                    type="password"
                    class="vault-key-input"
                    autocomplete="off"
                    :placeholder="
                      isKeyConfigured(item.id)
                        ? t('keyInputConfigured')
                        : t('keyInputUnconfigured')
                    "
                    @keydown.enter.prevent="saveProviderKey(item.id)"
                  />

                  <button
                    v-if="isKeyConfigured(item.id) && !(keyInputs[item.id] || '').trim()"
                    type="button"
                    class="vault-key-action-btn delete"
                    :aria-label="`${t('deleteKeyAria')} ${item.name}`"
                    @click="clearProviderKey(item.id)"
                  >
                    ✕
                  </button>
                  <button
                    v-else
                    type="button"
                    class="vault-key-action-btn save"
                    :disabled="!(keyInputs[item.id] || '').trim() || providerBusy"
                    :aria-label="`${t('saveKeyAria')} ${item.name}`"
                    @click="saveProviderKey(item.id)"
                  >
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2Z" />
                      <polyline points="17 21 17 13 7 13 7 21" />
                      <polyline points="7 3 7 8 15 8" />
                    </svg>
                  </button>
                </div>
              </div>

              <!-- Custom OpenAI-compatible Model Adder -->
              <div class="vault-section-subtitle">{{ t('customModelsSectionTitle') }}</div>
              <div class="custom-model-adder">
                <input
                  v-model="customModelDraft.name"
                  type="text"
                  :placeholder="t('customModelNamePlaceholder')"
                />
                <input
                  v-model="customModelDraft.model_id"
                  type="text"
                  :placeholder="t('customModelIdPlaceholder')"
                />
                <input
                  v-model="customModelDraft.base_url"
                  type="url"
                  :placeholder="t('customModelUrlPlaceholder')"
                />
                <input
                  v-model="customModelDraft.api_key"
                  type="password"
                  autocomplete="off"
                  :placeholder="t('customModelKeyPlaceholder')"
                />
                <button type="button" @click="addCustomModel">
                  {{ t('addCustomModelBtn') }}
                </button>
              </div>
              <div
                v-if="(modelHub.state.custom_models || []).length"
                class="custom-models-chips"
              >
                <span
                  v-for="cm in modelHub.state.custom_models"
                  :key="cm.id"
                  class="custom-model-tag"
                >
                  {{ cm.name }} ({{ cm.model_id }})
                  <button type="button" @click="removeCustomModel(cm.id)">✕</button>
                </span>
              </div>
            </div>
          </section>

          <!-- Card 4: 模型供应商注册表 (13 Providers & 199 Models) -->
          <section class="hub-card">
            <div
              class="hub-card-header clickable"
              @click="registryExpanded = !registryExpanded"
            >
              <div class="hub-card-title-group">
                <svg class="hub-icon" viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M4 6h16M4 12h16M4 18h16" />
                </svg>
                <h3>{{ t('providerRegistryTitle') }}</h3>
              </div>
              <span class="collapse-chevron">{{ registryExpanded ? '⌃' : '⌄' }}</span>
            </div>
            <div v-if="registryExpanded" class="registry-providers-list">
              <div
                v-for="prov in modelHub.registry.providers"
                :key="prov.id"
                class="registry-provider-item"
              >
                <div class="registry-provider-head">
                  <img :src="getProviderLogo(prov.id)" alt="" class="key-provider-logo" />
                  <strong>{{ prov.name }}</strong>
                  <span class="registry-url">{{ prov.base_url }}</span>
                  <span class="hub-count-pill">
                    {{ (prov.models || []).filter(m => m.visible !== false).length }}
                  </span>
                </div>
              </div>
            </div>
          </section>
        </div>

        <!-- Model Tab Quick Jump Cards (Protocol Override & Device Connection) -->
        <div
          v-show="settingsView === 'home' && settingsTab === 'model'"
          class="settings-home-grid"
        >
          <div class="settings-card">
            <div class="settings-card-info">
              <span class="settings-card-title">{{ t('directModelProvider') }}</span>
              <p class="settings-card-desc">{{ t('directModelSubpageDesc') }}</p>
            </div>
            <button type="button" class="open-provider-subpage" @click="openProviderSettings">
              {{ t('openProviderSubpage') }}
            </button>
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
        </div>

        <!-- Tab 3: 工作间 (SettingsWorkspace: Workspace Directory, Device Node, Memory & Kernel Stats) -->
        <div
          v-show="settingsView === 'home' && settingsTab === 'workspace'"
          class="model-hub-stack settings-tab-panel"
        >
          <section class="hub-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <h3>{{ t('workspaceSettingsTitle') }}</h3>
              </div>
            </div>
            <p class="view-card-desc">{{ t('workspaceSettingsDesc') }}</p>
            <div class="settings-kv-list">
              <div class="settings-kv-row">
                <span>{{ t('workspaceDirLabel') }}</span>
                <code>{{ catalog?.projects?.[0]?.directory_ref || t('unconfiguredValue') }}</code>
              </div>
              <div class="settings-kv-row">
                <span>{{ t('workspaceDeviceIdLabel') }}</span>
                <code>{{ catalog?.device_groups?.[0]?.label || t('unconfiguredValue') }}</code>
              </div>
            </div>
          </section>

          <section class="hub-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <h3>{{ t('workspaceStatsTitle') }}</h3>
              </div>
            </div>
            <div class="workspace-stats-grid">
              <div class="workspace-stat-box">
                <strong>{{ (catalog?.conversations || []).length || 1 }}</strong>
                <span>{{ t('conversations') }}</span>
              </div>
              <div class="workspace-stat-box">
                <strong>{{ readiness?.saved_messages ?? messages.length }}</strong>
                <span>{{ t('workspaceStatMessages') }}</span>
              </div>
              <div class="workspace-stat-box">
                <strong>{{ projectList.length }}</strong>
                <span>{{ t('projects') }}</span>
              </div>
              <div class="workspace-stat-box">
                <strong>{{ personalOps.notes.length }}</strong>
                <span>{{ t('knowledgeTabNotes') }}</span>
              </div>
              <div class="workspace-stat-box">
                <strong>{{ personalOps.schedules.length }}</strong>
                <span>{{ t('navSchedule') }}</span>
              </div>
            </div>
          </section>

          <section class="hub-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <h3>{{ t('workspaceMemoryTitle') }}</h3>
                <span class="hub-count-pill">{{ personalOps.memories.length }}</span>
              </div>
            </div>
            <p class="view-card-desc">{{ t('workspaceMemoryDesc') }}</p>
            <p v-if="!personalOps.memories.length" class="view-card-desc">
              {{ t('knowledgeEmptyMemoriesHint') }}
            </p>
            <div v-else class="captures-list">
              <div
                v-for="mem in personalOps.memories"
                :key="mem.id"
                class="memory-item-card"
              >
                <div class="memory-item-main">
                  <strong>{{ mem.body || mem.summary }}</strong>
                  <span class="schedule-kind-badge">{{ mem.state }}</span>
                </div>
                <div class="memory-item-actions">
                  <button
                    v-if="mem.state !== 'active'"
                    type="button"
                    class="mini-ghost-btn"
                    @click="handleUpdateMemory({ id: mem.id, state: 'active' })"
                  >
                    {{ t('knowledgeMemoryActivateBtn') }}
                  </button>
                  <button
                    v-if="mem.state !== 'forgotten'"
                    type="button"
                    class="mini-ghost-btn"
                    @click="handleUpdateMemory({ id: mem.id, state: 'forgotten' })"
                  >
                    {{ t('knowledgeMemoryForgetBtn') }}
                  </button>
                </div>
              </div>
            </div>
          </section>
        </div>

        <!-- Tab 4: 任务 (SettingsDailyRoutine: Execution Route & @daily_startup) -->
        <div
          v-show="settingsView === 'home' && settingsTab === 'tasks'"
          class="model-hub-stack settings-tab-panel"
        >
          <section class="hub-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <h3>{{ t('tasksSettingsTitle') }}</h3>
              </div>
            </div>
            <p class="view-card-desc">{{ t('tasksSettingsDesc') }}</p>
            <label class="provider-field">
              <span>{{ t('executionRouteLabel') }}</span>
              <select v-model="executionRoute" @change="changeExecutionRoute">
                <option value="auto">{{ t('routeAuto') }}</option>
                <option value="direct_byok">{{ t('routeDirectByok') }}</option>
                <option value="local_cli">{{ t('routeLocalCli') }}</option>
                <option value="remote_home">{{ t('routeRemoteHome') }}</option>
              </select>
            </label>
          </section>

          <section class="hub-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <h3>{{ t('dailyRoutineCardTitle') }}</h3>
                <span class="hub-count-pill">@daily_startup</span>
              </div>
            </div>
            <p class="view-card-desc">{{ t('dailyRoutineCardDesc') }}</p>
            <div class="routine-tag-box">
              <code>@daily_startup</code>
              <span>{{ t('dailyRoutineTagHint') }}</span>
            </div>
          </section>
        </div>

        <!-- Tab 5: 外观 (SettingsAppearance: Theme & Language) -->
        <div
          v-show="settingsView === 'home' && settingsTab === 'appearance'"
          class="model-hub-stack settings-tab-panel"
        >
          <section class="hub-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <h3>{{ t('appearanceSettingsTitle') }}</h3>
              </div>
            </div>
            <p class="view-card-desc">{{ t('appearanceSettingsDesc') }}</p>

            <div class="appearance-options-grid">
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
            </div>
          </section>
        </div>

        <!-- Tab 6: 关于 (SettingsAbout: Version & Kernel Diagnostics) -->
        <div
          v-show="settingsView === 'home' && settingsTab === 'about'"
          class="model-hub-stack settings-tab-panel"
        >
          <section class="hub-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <h3>{{ t('aboutVersion') }}</h3>
              </div>
            </div>
            <p class="view-card-desc">{{ t('versionValue') }}</p>
            <p class="view-card-desc">{{ t('aboutKernelArchDesc') }}</p>
          </section>

          <section class="hub-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <h3>{{ t('aboutDiagnosticsTitle') }}</h3>
              </div>
            </div>
            <div class="settings-kv-list">
              <div class="settings-kv-row">
                <span>{{ t('executionRouteLabel') }}</span>
                <code>{{ readiness?.mode || 'local_first' }}</code>
              </div>
              <div class="settings-kv-row">
                <span>{{ t('modelCenterTitle') }}</span>
                <code>{{ totalAvailableModelsCount }} models</code>
              </div>
            </div>
          </section>
        </div>

        <!-- Direct Provider Protocol Override & Connection Test Form (preserved for subpage & tests) -->
        <div v-show="settingsView === 'provider'" class="settings-provider-subpage">
          <form class="provider-form" @submit.prevent="saveProviderSettings(false)">
            <h3 class="provider-form-heading">{{ t('advancedProtocolOverrideTitle') }}</h3>
            <label class="provider-field">
              <span>{{ t('executionRouteLabel') }}</span>
              <select v-model="executionRoute" @change="changeExecutionRoute">
                <option value="auto">{{ t('routeAuto') }}</option>
                <option value="direct_byok">{{ t('routeDirectByok') }}</option>
                <option value="local_cli">{{ t('routeLocalCli') }}</option>
                <option value="remote_home">{{ t('routeRemoteHome') }}</option>
              </select>
            </label>

            <label class="provider-field">
              <span>{{ t('providerKindLabel') }}</span>
              <select v-model="providerForm.providerKind" @change="onProviderKindChange">
                <option value="openai_compatible">{{ t('providerOpenai') }}</option>
                <option value="anthropic_messages">{{ t('providerAnthropic') }}</option>
                <option value="gemini_generate_content">{{ t('providerGemini') }}</option>
              </select>
            </label>

            <label class="provider-field">
              <span>{{ t('providerNameLabel') }}</span>
              <input v-model="providerForm.label" type="text" required />
            </label>

            <label class="provider-field">
              <span>{{ t('providerBaseUrlLabel') }}</span>
              <input v-model="providerForm.baseUrl" type="url" required />
            </label>

            <label class="provider-field">
              <span>{{ t('providerModelIdLabel') }}</span>
              <input v-model="providerForm.modelId" type="text" required />
            </label>

            <label class="provider-field">
              <span>{{ t('providerApiKeyLabel') }}</span>
              <input
                v-model="providerForm.apiKey"
                type="password"
                autocomplete="off"
                :placeholder="
                  t(
                    providerProfile?.api_key_configured
                      ? 'providerApiKeyPlaceholderConfigured'
                      : 'providerApiKeyPlaceholderEmpty',
                  )
                "
              />
            </label>

            <label class="provider-toggle-row">
              <input v-model="providerForm.enabled" type="checkbox" />
              <span>{{ t('providerEnabledLabel') }}</span>
            </label>

            <div class="provider-actions">
              <button type="submit" :disabled="providerBusy">{{ t('providerSaveBtn') }}</button>
              <button
                type="button"
                :disabled="providerBusy"
                @click="testProviderConnection"
              >
                {{ t('providerTestBtn') }}
              </button>
              <button
                v-if="providerProfile?.api_key_configured"
                type="button"
                :disabled="providerBusy"
                @click="saveProviderSettings(true)"
              >
                {{ t('providerClearKeyBtn') }}
              </button>
            </div>

            <p v-if="providerProfile" class="provider-status-line" role="status">
              <template v-if="providerProfile.last_check_status === 'ok'">
                {{ t('providerStatusOk') }} ({{
                  providerProfile.last_check_reason || providerProfile.model_id
                }})
              </template>
              <template v-else-if="providerProfile.last_check_status === 'untested'">
                {{ t('providerStatusUntested') }}
              </template>
              <template v-else>
                {{ t('providerStatusFailed') }} {{ providerProfile.last_check_status }}
                {{ providerProfile.last_check_reason }}
              </template>
            </p>
          </form>
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

    <div v-if="personalFault" class="notice" role="alert">
      <span>{{ personalFault }}</span>
      <button type="button" @click="personalFault = null">×</button>
    </div>

    <!-- Primary View 2: 日程 (Schedule / InboxView) -->
    <ScheduleView
      v-if="activeDrawer === 'schedule'"
      :schedules="personalOps.schedules"
      :busy="personalBusy"
      @create-schedule="handleCreateSchedule"
      @update-status="handleUpdateScheduleStatus"
    />

    <!-- Primary View 3: 工作 (Work / RemoteAgent & Project Matrix) -->
    <WorkView
      v-if="activeDrawer === 'work'"
      :readiness="readiness"
      :catalog="catalog"
      :active-session-id="sessionId"
      :active-main-role="activeMainRole"
      :online-state="onlineState"
      @open-conversation="handleWorkOpenConversation"
      @create-project="handleWorkCreateProject"
      @create-conversation="handleWorkCreateConversation"
      @open-settings-tab="handleWorkOpenSettingsTab"
      @online-update-origin="handleOnlineUpdateOrigin"
      @online-begin-pairing="handleOnlineBeginPairing"
      @online-exchange-pairing="handleOnlineExchangePairing"
      @online-refresh="handleOnlineRefresh"
      @online-select-project="handleOnlineSelectProject"
      @online-disconnect="handleOnlineDisconnect"
    />

    <!-- Primary View 4: 知识 (Knowledge / Notes, Captures & Long-term Memory) -->
    <KnowledgeView
      v-if="activeDrawer === 'knowledge'"
      :notes="personalOps.notes"
      :captures="personalOps.captures"
      :memories="personalOps.memories"
      :decisions="personalOps.decisions"
      :busy="personalBusy"
      @save-note="handleSaveNote"
      @create-capture="handleCreateCapture"
      @update-memory="handleUpdateMemory"
      @create-decision="handleCreateDecision"
      @update-decision="handleUpdateDecision"
    />

    <!-- Primary View 1: 对话 (Chat) -->
    <main v-show="activeDrawer === 'chat' || activeDrawer === 'settings'" class="main">
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
                directProviderEnabled
                  ? 'startDirectHint'
                  : homeConfigured
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
                ['direct_provider', 'replica_direct_provider'].includes(message.source)
                  ? 'assistantDirectReply'
                  : ['codex_execution', 'home_codex_execution'].includes(message.source)
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

      <!-- Floating Quick Model Switcher Popover -->
      <div v-if="quickSwitcherOpen" class="quick-model-popover">
        <div class="quick-provider-col">
          <button
            v-for="prov in selectorProviders"
            :key="prov.id"
            type="button"
            class="quick-provider-item"
            :class="{ active: quickSwitcherProviderId === prov.id }"
            @click="quickSwitcherProviderId = prov.id"
          >
            <img :src="getProviderLogo(prov.id)" alt="" class="chip-logo" />
            <span>{{ prov.name }}</span>
            <span
              v-if="isKeyConfigured(prov.id)"
              class="key-status-dot configured"
            ></span>
          </button>
        </div>
        <div class="quick-model-col">
          <button
            v-for="mdl in quickSwitcherModels"
            :key="mdl.id"
            type="button"
            class="quick-model-item"
            :class="{
              selected:
                activeMainRole.model_id === mdl.id &&
                activeMainRole.provider_id === quickSwitcherProviderId,
            }"
            @click="selectRoleModel('main', quickSwitcherProvider, mdl)"
          >
            <strong>{{ mdl.name || mdl.id }}</strong>
            <small>{{ mdl.id }}</small>
          </button>
        </div>
      </div>

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
          <div class="composer-left-tools">
            <button
              type="button"
              class="composer-model-pill"
              :aria-label="t('quickSwitchModel')"
              @click="
                quickSwitcherProviderId = activeMainRole.provider_id || 'deepseek';
                quickSwitcherOpen = !quickSwitcherOpen;
              "
            >
              <img
                :src="getProviderLogo(activeMainRole.provider_id)"
                alt=""
                class="pill-logo"
              />
              <span class="pill-model-name">
                {{ activeMainRole.model_name || activeMainRole.model_id }}
              </span>
              <span aria-hidden="true">▾</span>
            </button>
            <span class="composer-hint-text">
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
          </div>
          <button
            v-if="activeExecution"
            type="button"
            class="composer-submit-btn"
            :disabled="cancelPending.has(activeExecution.id)"
            @click="cancel(activeExecution)"
          >
            {{ t(cancelPending.has(activeExecution.id) ? 'requestingStop' : 'stopExecution') }}
          </button>
          <button v-else type="submit" class="composer-submit-btn" :disabled="!canSend">
            {{ t(sendLabel ? 'sendAndExecute' : 'send') }}
          </button>
        </div>
      </form>
    </main>

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
        :class="{ active: activeDrawer === 'work' && !settingsOpen }"
        @click="switchPrimaryDrawer('work')"
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <rect x="2" y="7" width="20" height="14" rx="2" />
          <path d="M16 21V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16" />
        </svg>
        <span>{{ t('navWork') }}</span>
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
        :aria-label="t('navSettingsAria')"
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
