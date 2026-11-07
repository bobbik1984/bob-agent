import { compactQuota } from './quota-display.mjs';
import { computed, reactive, ref, onMounted, onUnmounted } from 'vue';
import defaultRegistryData from '../assets/model_providers.json';
import mobileModelsData from '../assets/mobile_models.json';

export function useModelHub(ctx) {
  const {
    t,
    available,
    kernelBridge,
    sessionId,
    fault,
    settingsView,
    settingsTab,
    settingsOpen,
    remoteConversation,
    restoreRemoteSelection,
    onlineState,
    handleOnlineRefresh,
    reload,
  } = ctx;

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
        model_id: 'deepseek-v4.1-flash-1017',
        model_name: 'DeepSeek V4.1 Flash 1017',
        provider_name: 'DeepSeek',
      },
      flash: {
        provider_id: 'deepseek',
        model_id: 'deepseek-v4.1-flash-1017',
        model_name: 'DeepSeek V4.1 Flash',
        provider_name: 'DeepSeek',
      },
    },
    provider_variants: {
      qwen: 'default',
      minimax: 'default',
      zhipu: 'default',
      kimi: 'default',
    },
    custom_models: [],
    remote_nodes: [],
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
const quickSwitcherSearchQuery = ref('');
const discoveringModels = ref(false);
const quickPopoverRef = ref(null);
const quickPillRef = ref(null);
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

const assistantAutoRouting = ref(localStorage.getItem('bob-assistant-auto-routing') !== 'false');
function toggleAutoRouting() {
  assistantAutoRouting.value = !assistantAutoRouting.value;
  try { localStorage.setItem('bob-assistant-auto-routing', String(assistantAutoRouting.value)); } catch {}
}

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
        model_id: 'deepseek-v4.1-flash-1017',
        model_name: 'DeepSeek V4.1 Flash 1017',
        provider_name: 'DeepSeek',
      },
    },
    {
      key: 'flash',
      title: t('roleFlashTitle'),
      desc: t('roleFlashDesc'),
      icon: 'flash',
      assignment: roles.flash || roles.clerk || {
        provider_id: 'deepseek',
        model_id: 'deepseek-v4.1-flash-1017',
        model_name: 'DeepSeek V4.1 Flash',
        provider_name: 'DeepSeek',
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
  const list = (modelHub.value?.registry?.providers || []).filter(
    p => p.id !== 'offline' && p.id !== 'vertex_ai',
  );
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

const currentSelectorHiddenMatches = computed(() => {
  const prov = currentSelectorProvider.value;
  const q = modelSearchQuery.value.trim().toLowerCase();
  if (!prov || !q) return [];
  return (prov.models || [])
    .filter(m => {
      if (m.visible !== false) return false;
      return (
        (m.name && m.name.toLowerCase().includes(q)) ||
        (m.id && m.id.toLowerCase().includes(q))
      );
    })
    .slice(0, 12);
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
  const q = quickSwitcherSearchQuery.value.trim().toLowerCase();
  return (prov.models || []).filter(m => {
    if (m.visible === false) return false;
    if (!q) return true;
    return (
      (m.name && m.name.toLowerCase().includes(q)) ||
      (m.id && m.id.toLowerCase().includes(q))
    );
  });
});

const quickSwitcherHiddenMatches = computed(() => {
  const prov = quickSwitcherProvider.value;
  const q = quickSwitcherSearchQuery.value.trim().toLowerCase();
  if (!prov || !q) return [];
  return (prov.models || [])
    .filter(m => {
      if (m.visible !== false) return false;
      return (
        (m.name && m.name.toLowerCase().includes(q)) ||
        (m.id && m.id.toLowerCase().includes(q))
      );
    })
    .slice(0, 10);
});

function isKeyConfigured(providerId) {
  return Boolean(modelHub.value?.configured_keys?.[providerId]);
}

const selectorProviderOptions = computed(() =>
  selectorProviders.value.map(prov => ({
    value: prov.id,
    label: `${prov.name} (${(prov.models || []).filter(m => m.visible !== false).length})${
      isKeyConfigured(prov.id) ? ` [${t('keyInputConfigured')}]` : ''
    }`,
  })),
);

const variantOptions = computed(() => [
  { value: 'default', label: t('variantDefault') },
  { value: 'international', label: t('variantInternational') },
  { value: 'coding', label: t('variantCoding') },
]);

const executionRouteOptions = computed(() => [
  { value: 'auto', label: t('routeAuto') },
  { value: 'direct_byok', label: t('routeDirectByok') },
  { value: 'local_cli', label: t('routeLocalCli') },
  { value: 'remote_home', label: t('routeRemoteHome') },
]);

const themeOptions = computed(() => [
  { value: 'system', label: t('themeSystem') },
  { value: 'light', label: t('themeLight') },
  { value: 'dark', label: t('themeDark') },
]);

const languageOptions = computed(() => [
  { value: 'zh-CN', label: t('chinese') },
  { value: 'en-US', label: t('english') },
]);

const providerKindOptions = computed(() => [
  { value: 'openai_compatible', label: t('providerOpenai') },
  { value: 'anthropic_messages', label: t('providerAnthropic') },
  { value: 'gemini_generate_content', label: t('providerGemini') },
]);

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
    for (const rKey of ['main', 'flash']) {
      const sourceKey = (rKey === 'flash' && !rawRoles.flash && rawRoles.clerk) ? 'clerk' : rKey;
      const resolved = resolveEntryToRoleObj(rawRoles[sourceKey], providers);
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
  if (Array.isArray(snap.remoteNodes)) {
    modelHub.value.state.remote_nodes = snap.remoteNodes;
    if(restoreRemoteSelection.value){restoreRemoteSelection.value=false;try{const saved=JSON.parse(localStorage.getItem('bob-active-remote')||'null');const node=snap.remoteNodes.find(n=>n.id===saved?.nodeId&&n.enabled!==false);if(node&&saved?.session?.id)remoteConversation.value={node,session:saved.session};}catch{}}
    const activeNode =
      snap.remoteNodes.find(n => n.enabled !== false && (n.hasToken || n.token)) ||
      snap.remoteNodes.find(n => n.enabled !== false);
    if (activeNode && activeNode.baseUrl) {
      if (!onlineState.connected && !onlineState.busy) {
        onlineState.origin = activeNode.baseUrl;
        if (activeNode.selectedProjectId) {
          onlineState.selectedProjectId = activeNode.selectedProjectId;
        }
        if (activeNode.hasToken || activeNode.token) {
          onlineState.hasToken = true;
          if (activeNode.token) onlineState.token = activeNode.token;
          void handleOnlineRefresh();
        }
      }
    }
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
  if (snap.subscriptionQuotas || snap.subscription_quotas) {
    modelHub.value.state.subscription_quotas = snap.subscriptionQuotas || snap.subscription_quotas;
  }
  if (snap.prepaidUsage || snap.prepaid_usage) {
    modelHub.value.state.prepaid_usage = snap.prepaidUsage || snap.prepaid_usage;
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
  if (modelObj.visible === false) {
    modelObj.visible = true;
    if (available && kernelBridge.modelVisibilitySet) {
      await kernelBridge.modelVisibilitySet(providerObj.id, modelObj.id, true).catch(() => null);
    }
  }
  const payload = {
    role: roleKey,
    providerId: providerObj.id,
    modelId: modelObj.id,
    modelName: modelObj.name || modelObj.id,
    providerName: providerObj.name || providerObj.id,
  };
  try {
    if (!available || !kernelBridge.modelRoleAssign) throw new Error('UNAVAILABLE');
    const snap = await kernelBridge.modelRoleAssign(payload);
    if (!snap) throw new Error('SAVE_UNCONFIRMED');
    applyModelHubSnapshot(snap);
    fault.value = null;
  } catch {
    fault.value = 'modelSelectionSaveFailed';
    return;
  }
  if (roleKey === 'main') {
    assistantAutoRouting.value = false;
    try { localStorage.setItem('bob-assistant-auto-routing', 'false'); } catch {}
  }
  editingRole.value = null;
  quickSwitcherOpen.value = false;
  quickSwitcherSearchQuery.value = '';
  await reload();
}

async function discoverLatestModels(providerId = null) {
  if (discoveringModels.value) return;
  discoveringModels.value = true;
  try {
    if (available && kernelBridge.modelsDiscover) {
      const snap = await kernelBridge.modelsDiscover(providerId).catch(() => null);
      if (snap) {
        applyModelHubSnapshot(snap);
        return;
      }
    }
    await loadModelHub();
  } finally {
    discoveringModels.value = false;
  }
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

const showPrepaidCost = ref(false);
function togglePrepaidCost() {
  showPrepaidCost.value = !showPrepaidCost.value;
}

function resolveQuotaTone(pct) {
  const n = Number(pct) || 0;
  if (n >= 50) return 'high';
  if (n >= 20) return 'mid';
  return 'low';
}

const quotaNow = ref(Date.now() / 1000);
let quotaClock;
onMounted(() => { quotaClock = setInterval(() => { quotaNow.value = Date.now() / 1000; }, 1000); });
onUnmounted(() => clearInterval(quotaClock));
const subscriptionQuotas = computed(() => [
  { id: 'codex', label: 'Codex · ChatGPT', logo: '/logos/codex.png' },
  { id: 'antigravity', label: 'Antigravity', logo: '/logos/antigravity.png' },
  { id: 'claude', label: 'Claude Code', logo: '/logos/claude.png' },
].map(item => {
  const data = modelHub.value.state.subscription_quotas?.[item.id];
  const known = ['available', 'stale'].includes(data?.status);
  const windowLabels = {weekly:'quotaWeekly', five_hour:'quotaFiveHour', primary:'quotaPrimary', secondary:'quotaSecondary',
    'gemini-5h':'quotaGeminiFiveHour', 'gemini-weekly':'quotaGeminiWeekly', '3p-5h':'quotaThirdPartyFiveHour', '3p-weekly':'quotaThirdPartyWeekly'};
  const compact = compactQuota(data, quotaNow.value, item.id === 'antigravity' ? 'gemini-weekly' : 'weekly');
  return {...item, compact, status: known ? data.status : 'unavailable', source: data?.source_node || '',
    updated: known && data.observed_at ? new Date(data.observed_at * 1000).toLocaleString() : '',
    windows: known ? (data.windows || []).map(w => ({...w, label: windowLabels[w.id] ? t(windowLabels[w.id]) : w.id,
      remaining: Number(w.remaining_percent).toLocaleString(undefined, {maximumFractionDigits:1}),
      reset: w.resets_at ? new Date(w.resets_at * 1000).toLocaleString() : ''})) : []};
}));

const prepaidUsageList = computed(() => {
  const live = modelHub.value?.state?.prepaid_usage || {};
  return [
    {
      id: 'deepseek',
      label: 'DeepSeek',
      logo: '/logos/deepseek.png',
      tokensText: live.deepseek?.tokens_text || t('quotaUnavailable'),
      costText: live.deepseek?.cost_text || t('quotaUnavailable'),
    },
    {
      id: 'openrouter',
      label: 'OpenRouter',
      logo: '/logos/openrouter.png',
      tokensText: live.openrouter?.tokens_text || t('quotaUnavailable'),
      costText: live.openrouter?.cost_text || t('quotaUnavailable'),
    },
    {
      id: 'zhipu',
      label: 'GLM · 智谱',
      logo: '/logos/glm.svg',
      tokensText: live.zhipu?.tokens_text || t('quotaUnavailable'),
      costText: live.zhipu?.cost_text || t('quotaUnavailable'),
    },
    {
      id: 'qwen',
      label: 'Qwen · 百炼',
      logo: '/logos/qwen.png',
      tokensText: live.qwen?.tokens_text || t('quotaUnavailable'),
      costText: live.qwen?.cost_text || t('quotaUnavailable'),
    },
  ];
});

  return Object.assign(ctx, {
    subscriptionQuotas,
    prepaidUsageList,
    showPrepaidCost,
    togglePrepaidCost,
    modelHub,
    mobileModels,
    localRepoExpanded,
    localMirrorSource,
    apiKeysVaultExpanded,
    registryExpanded,
    editingRole,
    selectorProviderId,
    modelSearchQuery,
    quickSwitcherOpen,
    quickSwitcherProviderId,
    quickSwitcherSearchQuery,
    discoveringModels,
    quickPopoverRef,
    quickPillRef,
    keyInputs,
    customModelDraft,
    modelProviderKeyList,
    toolServiceKeyList,
    totalAvailableModelsCount,
    assistantAutoRouting,
    toggleAutoRouting,
    roleCards,
    activeMainRole,
    selectorProviders,
    currentSelectorProvider,
    currentSelectorModels,
    currentSelectorHiddenMatches,
    quickSwitcherProvider,
    quickSwitcherModels,
    quickSwitcherHiddenMatches,
    isKeyConfigured,
    selectorProviderOptions,
    variantOptions,
    executionRouteOptions,
    themeOptions,
    languageOptions,
    providerKindOptions,
    toggleRoleEditor,
    providerProfile,
    providerForm,
    executionRoute,
    providerBusy,
    syncProviderForm,
    resolveEntryToRoleObj,
    applyModelHubSnapshot,
    loadModelHub,
    saveProviderKey,
    clearProviderKey,
    selectRoleModel,
    discoverLatestModels,
    setProviderVariant,
    addCustomModel,
    removeCustomModel,
    onProviderKindChange,
    openProviderSettings,
    saveProviderSettings,
    testProviderConnection,
    changeExecutionRoute,
  });
}
