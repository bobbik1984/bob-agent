<script setup>
import { computed, inject, ref, watch } from 'vue';
import CustomSelect from '../components/CustomSelect.vue';
import ProjectContextPane from '../components/ProjectContextPane.vue';
import HomeConnection from '../HomeConnection.vue';
import WorkView from './WorkView.vue';
import SiteSessionVault from '../components/SiteSessionVault.vue';

const shell = inject('bobAppShell');
const {
  t,
  locale,
  theme,
  available,
  readiness,
  catalog,
  sessionId,
  messages,
  projectList,
  devicePaired,
  deviceOnline,
  openDevices,
  settingsOpen,
  settingsPanel,
  settingsView,
  settingsTab,
  settingsKeydown,
  switchSettingsTab,
  devicePanel,
  deviceCapabilities,
  securityLock,
  onlineState,
  handleOnlineUpdateOrigin,
  handleOnlineSaveNode,
  handleOnlineBeginPairing,
  handleOnlineExchangePairing,
  handleOnlineRefresh,
  handleOnlineSelectProject,
  handleOnlineLoadMoreMessages,
  handleOnlineSubmitOperation,
  handleOnlineDisconnect,
  handleOnlineRevoke,
  handleWorkOpenConversation,
  handleWorkCreateProject,
  handleWorkCreateConversation,
  handleWorkOpenSettingsTab,
  handleCreateProjectTodo,
  handleUpdateScheduleStatus,
  handleStartTodoSession,
  handleUpdateMemory,
  reload,
  focusPairedComposer,
  modelHub,
  activeMainRole,
  mobileModels,
  localRepoExpanded,
  localMirrorSource,
  apiKeysVaultExpanded,
  registryExpanded,
  editingRole,
  selectorProviderId,
  modelSearchQuery,
  discoveringModels,
  keyInputs,
  customModelDraft,
  modelProviderKeyList,
  toolServiceKeyList,
  totalAvailableModelsCount,
  roleCards,
  selectorProviders,
  currentSelectorProvider,
  currentSelectorModels,
  currentSelectorHiddenMatches,
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
  getProviderLogo,
  personalOps,
  jarvisHome,
  kernelBridge,
} = shell;

const proactivityLevels = [
  { value: 0.0, labelKey: 'proactivitySilent', descKey: 'proactivitySilentDesc' },
  { value: 0.35, labelKey: 'proactivityMorningBrief', descKey: 'proactivityMorningBriefDesc' },
  { value: 0.7, labelKey: 'proactivityTimely', descKey: 'proactivityTimelyDesc' },
  { value: 1.0, labelKey: 'proactivityCare', descKey: 'proactivityCareDesc' },
];

const currentProactivityLevel = computed(() => {
  return jarvisHome?.proactivityState?.value?.level ?? 0.35;
});

function setProactivity(level) {
  if (typeof jarvisHome?.saveProactivity === 'function') {
    jarvisHome.saveProactivity(level);
  }
}

function onProactivitySliderInput(e) {
  const raw = Number(e.target.value);
  let snapped = 0.35;
  if (raw <= 0.17) snapped = 0.0;
  else if (raw <= 0.52) snapped = 0.35;
  else if (raw <= 0.85) snapped = 0.7;
  else snapped = 1.0;
  setProactivity(snapped);
}

const monthlyCapInput = ref(10);
const capSavedNotice = ref(false);

watch(
  () => jarvisHome?.budgetState?.value?.monthly_cap_usd,
  val => {
    if (val !== undefined && val !== null) {
      monthlyCapInput.value = Number(val);
    }
  },
  { immediate: true },
);

async function saveMonthlySpendCap(val) {
  const cap = Number(val ?? monthlyCapInput.value);
  if (Number.isNaN(cap) || cap < 0) return;
  monthlyCapInput.value = cap;
  if (typeof kernelBridge?.jarvisBudgetSet === 'function') {
    await kernelBridge.jarvisBudgetSet(cap).catch(() => null);
  }
  if (jarvisHome?.budgetState?.value) {
    jarvisHome.budgetState.value.monthly_cap_usd = cap;
  }
  capSavedNotice.value = true;
  setTimeout(() => {
    capSavedNotice.value = false;
  }, 2000);
}
</script>

<template>
    <section
      v-show="settingsOpen"
      ref="settingsPanel"
      class="settings-panel"
      :aria-label="
        t(
          settingsView === 'provider'
            ? 'directModelProvider'
            : settingsTab === 'connection'
              ? 'deviceAndConnection'
              : 'settings',
        )
      "
      @keydown="settingsKeydown"
    >
      <!-- Legacy-aligned Flush Top 5-Tab Bar: 模型 | 连接 | 工作间 | 任务 | 外观 -->
      <nav class="settings-top-tabs" role="tablist">
        <button
          type="button"
          class="settings-top-tab"
          :class="{ active: settingsTab === 'model' }"
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
          :class="{ active: settingsTab === 'connection' }"
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
      </nav>

      <div class="settings-scroll-container">
        <!-- Tab 1: 模型 (Model Center + Local Models + API Key Management + Registry) -->
        <div
          v-show="settingsView === 'home' && settingsTab === 'model'"
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
                class="mini-ghost-btn"
                :disabled="discoveringModels"
                :title="t('refreshRegistry')"
                @click="discoverLatestModels(null)"
              >
                {{ discoveringModels ? t('discoveringModelsBtn') : t('discoverLatestModelsBtn') }}
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
                  <svg v-else viewBox="0 0 24 24" aria-hidden="true">
                    <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" fill="currentColor" />
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
                  <CustomSelect
                    v-model="selectorProviderId"
                    :options="selectorProviderOptions"
                  />
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
                  <CustomSelect
                    :model-value="modelHub.state.provider_variants?.[selectorProviderId] || 'default'"
                    :options="variantOptions"
                    @change="setProviderVariant(selectorProviderId, $event)"
                  />
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
                    <strong>
                      {{ mdl.name || mdl.id }}
                      <span v-if="mdl.discovered" class="model-tag-pill">{{
                        t('discoveredModelBadge')
                      }}</span>
                    </strong>
                    <small>{{ mdl.id }}</small>
                  </div>
                </button>
              </div>
              <div v-if="currentSelectorHiddenMatches.length" class="catalog-matches-section">
                <div class="catalog-matches-label">{{ t('catalogHiddenMatchesLabel') }}</div>
                <div class="model-pool-grid">
                  <button
                    v-for="mdl in currentSelectorHiddenMatches"
                    :key="mdl.id"
                    type="button"
                    class="model-pool-chip catalog-hidden-chip"
                    @click="selectRoleModel(editingRole, currentSelectorProvider, mdl)"
                  >
                    <img
                      :src="getProviderLogo(selectorProviderId)"
                      alt=""
                      class="chip-logo"
                    />
                    <div class="chip-text">
                      <strong>
                        {{ mdl.name || mdl.id }}
                        <span class="model-tag-pill">{{ t('enableCatalogModelBadge') }}</span>
                      </strong>
                      <small>{{ mdl.id }}</small>
                    </div>
                  </button>
                </div>
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
              <svg class="chevron-svg" viewBox="0 0 24 24" aria-hidden="true">
                <path :d="localRepoExpanded ? 'm18 15-6-6-6 6' : 'm6 9 6 6 6-6'" />
              </svg>
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
                {{ t('physicalModelDir') }} ({{ t('comingSoonBadge') }})
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
              <svg class="chevron-svg" viewBox="0 0 24 24" aria-hidden="true">
                <path :d="apiKeysVaultExpanded ? 'm18 15-6-6-6 6' : 'm6 9 6 6 6-6'" />
              </svg>
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
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path d="M18 6 6 18M6 6l12 12" />
                    </svg>
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
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                      <path d="M18 6 6 18M6 6l12 12" />
                    </svg>
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
                  <button type="button" @click="removeCustomModel(cm.id)">×</button>
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
              <svg class="chevron-svg" viewBox="0 0 24 24" aria-hidden="true">
                <path :d="registryExpanded ? 'm18 15-6-6-6 6' : 'm6 9 6 6 6-6'" />
              </svg>
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
          <ProjectContextPane :active="settingsOpen && settingsTab === 'workspace'" />
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
          <!-- 4-Level Proactivity Slider Card -->
          <section class="hub-card proactivity-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <svg class="hub-icon" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
                  <path d="M13.73 21a2 2 0 0 1-3.46 0" />
                </svg>
                <h3>{{ t('proactivitySettingsTitle') }}</h3>
                <span class="hub-count-pill">
                  {{
                    currentProactivityLevel <= 0.17
                      ? t('proactivitySilent')
                      : currentProactivityLevel <= 0.52
                        ? t('proactivityMorningBrief')
                        : currentProactivityLevel <= 0.85
                          ? t('proactivityTimely')
                          : t('proactivityCare')
                  }}
                </span>
              </div>
            </div>
            <p class="view-card-desc">{{ t('proactivitySettingsDesc') }}</p>

            <div class="proactivity-slider-box">
              <input
                type="range"
                min="0"
                max="1"
                step="0.01"
                :value="currentProactivityLevel"
                class="proactivity-range-slider"
                :aria-label="t('proactivitySettingsTitle')"
                @input="onProactivitySliderInput"
              />
              <div class="proactivity-tiers-grid">
                <button
                  v-for="tier in proactivityLevels"
                  :key="tier.value"
                  type="button"
                  class="proactivity-tier-btn"
                  :class="{ active: Math.abs(currentProactivityLevel - tier.value) < 0.18 }"
                  @click="setProactivity(tier.value)"
                >
                  <span class="tier-label">{{ t(tier.labelKey) }}</span>
                  <span class="tier-desc">{{ t(tier.descKey) }}</span>
                </button>
              </div>
            </div>

            <div class="quiet-hours-badge">
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
              </svg>
              <span>{{ t('proactivityQuietHoursNote') }}</span>
            </div>
          </section>

          <!-- Monthly Spend Cap Card -->
          <section class="hub-card spend-cap-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <svg class="hub-icon" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="12" y1="1" x2="12" y2="23" />
                  <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
                </svg>
                <h3>{{ t('spendCapTitle') }}</h3>
                <span class="hub-count-pill">${{ monthlyCapInput }}</span>
              </div>
              <span v-if="capSavedNotice" class="save-feedback-pill">{{ t('spendCapSaved') }}</span>
            </div>
            <p class="view-card-desc">{{ t('spendCapDesc') }}</p>

            <div class="spend-cap-controls">
              <div class="spend-cap-presets-row">
                <button
                  v-for="preset in [5, 10, 20, 50]"
                  :key="preset"
                  type="button"
                  class="spend-preset-btn"
                  :class="{ active: monthlyCapInput === preset }"
                  @click="saveMonthlySpendCap(preset)"
                >
                  ${{ preset }}
                </button>
              </div>

              <div class="spend-cap-input-row">
                <span class="currency-prefix">$</span>
                <input
                  v-model.number="monthlyCapInput"
                  type="number"
                  min="0"
                  step="1"
                  class="spend-cap-number-input"
                  :placeholder="t('spendCapMonthly')"
                  @keydown.enter.prevent="saveMonthlySpendCap(monthlyCapInput)"
                />
                <button
                  type="button"
                  class="mini-ghost-btn spend-save-btn"
                  @click="saveMonthlySpendCap(monthlyCapInput)"
                >
                  {{ t('spendCapSaveBtn') }}
                </button>
              </div>

              <div class="current-spend-bar">
                <span>{{ t('spendCapCurrentSpend', { spend: (jarvisHome?.budgetState?.value?.current_spend_usd ?? 0).toFixed(2) }) }}</span>
              </div>
            </div>
          </section>

          <!-- Site Session Center -->
          <SiteSessionVault />

          <section class="hub-card">
            <div class="hub-card-header">
              <div class="hub-card-title-group">
                <h3>{{ t('tasksSettingsTitle') }}</h3>
              </div>
            </div>
            <p class="view-card-desc">{{ t('tasksSettingsDesc') }}</p>
            <label class="provider-field">
              <span>{{ t('executionRouteLabel') }}</span>
              <CustomSelect
                v-model="executionRoute"
                :options="executionRouteOptions"
                @change="changeExecutionRoute"
              />
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
                <div class="theme-select">
                  <CustomSelect
                    v-model="theme"
                    :options="themeOptions"
                    :aria-label="t('themeLabel')"
                  />
                </div>
              </div>

              <div class="settings-card">
                <div class="settings-card-info">
                  <span class="settings-card-title">{{ t('language') }}</span>
                </div>
                <div class="language">
                  <CustomSelect
                    v-model="locale"
                    :options="languageOptions"
                    :aria-label="t('language')"
                  />
                </div>
              </div>
            </div>

            <div v-if="securityLock?.state?.value?.supported" class="settings-card security-lock-setting-card">
              <div class="settings-card-info">
                <span class="settings-card-title">{{ t('securityLockSettingsTitle') }}</span>
                <p class="view-card-desc">{{ t('securityLockSettingsDesc') }}</p>
              </div>
              <button
                type="button"
                role="switch"
                class="security-lock-switch"
                :class="{ on: securityLock.state.value.enabled }"
                :aria-checked="securityLock.state.value.enabled"
                :aria-label="t('securityLockSettingsTitle')"
                :disabled="securityLock.busy.value"
                @click="
                  securityLock.toggleInSettings(!securityLock.state.value.enabled, {
                    title: t('securityLockPromptTitle'),
                    subtitle: t('securityLockPromptSubtitle'),
                  })
                "
              >
                <span class="security-lock-switch-thumb" aria-hidden="true"></span>
              </button>
            </div>
          </section>
        </div>

        <!-- Direct Provider Protocol Override & Connection Test Form (preserved for subpage & tests) -->
        <div v-show="settingsView === 'provider'" class="settings-provider-subpage">
          <div class="settings-subpage-bar">
            <button
              type="button"
              class="back-settings-btn"
              :aria-label="t('backToSettings')"
              @click="
                settingsView = 'home';
                settingsTab = 'model';
              "
            >
              ← {{ t('backToSettings') }}
            </button>
          </div>
          <form class="provider-form" @submit.prevent="saveProviderSettings(false)">
            <h3 class="provider-form-heading">{{ t('advancedProtocolOverrideTitle') }}</h3>
            <label class="provider-field">
              <span>{{ t('executionRouteLabel') }}</span>
              <CustomSelect
                v-model="executionRoute"
                :options="executionRouteOptions"
                @change="changeExecutionRoute"
              />
            </label>

            <label class="provider-field">
              <span>{{ t('providerKindLabel') }}</span>
              <CustomSelect
                v-model="providerForm.providerKind"
                :options="providerKindOptions"
                @change="onProviderKindChange"
              />
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

        <div
          v-show="settingsView === 'home' && settingsTab === 'connection'"
          class="settings-device-subpage"
        >
    <WorkView
      :settings-only="true"
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
          <HomeConnection
            ref="devicePanel"
            v-if="sessionId"
            :paired="devicePaired"
            @changed="reload"
            @connected="
              settingsOpen = false;
              focusPairedComposer();
            "
          />
        </div>

        <!-- Low-Profile About & System Diagnostics Footer (demoted from primary top tabs, always at bottom) -->
        <details v-show="settingsView === 'home'" class="about-footer-details">
          <summary class="about-footer-summary">
            <span>{{ t('aboutFooterTitle') }} · {{ t('versionValue') }}</span>
          </summary>
          <div class="about-footer-body">
            <p class="view-card-desc">{{ t('aboutKernelArchDesc') }}</p>
            <div class="settings-kv-list">
              <div class="settings-kv-row">
                <span>{{ t('executionRouteLabel') }}</span>
                <code>{{ readiness?.mode || 'local_first' }}</code>
              </div>
              <div class="settings-kv-row">
                <span>{{ t('modelCenterTitle') }}</span>
                <code>{{ totalAvailableModelsCount }} models</code>
              </div>
              <div class="settings-kv-row">
                <span>{{ t('northStarTitle') }}</span>
                <code>
                  {{
                    t('northStarStatsText', {
                      memories: personalOps.memories.filter(m => m.state === 'active').length,
                      decisions: personalOps.decisions.filter(d => d.status === 'accepted').length,
                      pendingSync: personalOps.pending_sync_count || 0,
                    })
                  }}
                </code>
              </div>
            </div>
          </div>
        </details>
      </div>
    </section>
</template>

<style scoped>
.proactivity-card,
.spend-cap-card {
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
}

.proactivity-slider-box {
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
  margin-top: var(--gap-3xs);
}

.proactivity-range-slider {
  width: 100%;
  accent-color: var(--accent-primary);
  cursor: pointer;
}

.proactivity-tiers-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
  gap: var(--gap-xs);
}

.proactivity-tier-btn {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  text-align: left;
  gap: 0.2rem;
  padding: 0.5rem 0.65rem;
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: border-color 0.15s ease, background 0.15s ease;
}

.proactivity-tier-btn:hover {
  background: var(--bg-tertiary);
}

.proactivity-tier-btn.active {
  background: var(--accent-soft);
  border-color: var(--accent-primary);
}

.tier-label {
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--text-primary);
}

.proactivity-tier-btn.active .tier-label {
  color: var(--accent-primary);
}

.tier-desc {
  font-size: var(--text-2xs);
  color: var(--text-secondary);
  line-height: 1.3;
}

.quiet-hours-badge {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  font-size: var(--text-xs);
  color: var(--text-secondary);
  background: var(--bg-secondary);
  padding: 0.25rem 0.6rem;
  border-radius: var(--radius-pill);
  width: fit-content;
  border: 1px solid var(--line);
}

.spend-cap-controls {
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
  margin-top: var(--gap-3xs);
}

.spend-cap-presets-row {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
  flex-wrap: wrap;
}

.spend-preset-btn {
  padding: 0.3rem 0.75rem;
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  border-radius: var(--radius-pill);
  color: var(--text-primary);
  font-size: var(--text-xs);
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.spend-preset-btn:hover {
  background: var(--bg-tertiary);
}

.spend-preset-btn.active {
  background: var(--accent-primary);
  border-color: var(--accent-primary);
  color: var(--text-on-accent);
}

.spend-cap-input-row {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
}

.currency-prefix {
  font-size: var(--text-md);
  font-weight: 600;
  color: var(--text-secondary);
}

.spend-cap-number-input {
  width: 120px;
  padding: 0.4rem 0.6rem;
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  font-size: var(--text-sm);
  outline: none;
  transition: border-color 0.15s ease;
}

.spend-cap-number-input:focus {
  border-color: var(--accent-primary);
}

.spend-save-btn {
  padding: 0.4rem 0.8rem;
}

.save-feedback-pill {
  font-size: var(--text-2xs);
  color: var(--color-success);
  font-weight: 600;
}

.current-spend-bar {
  font-size: var(--text-xs);
  color: var(--text-secondary);
}

.security-lock-setting-card {
  margin-top: var(--gap-sm);
}

.security-lock-switch {
  position: relative;
  width: 2.75rem;
  height: 1.55rem;
  flex-shrink: 0;
  border-radius: var(--radius-pill);
  border: 1px solid var(--line);
  background: var(--bg-secondary);
  padding: 0;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.security-lock-switch.on {
  background: var(--accent-primary);
  border-color: var(--accent-primary);
}

.security-lock-switch-thumb {
  position: absolute;
  top: 0.18rem;
  left: 0.18rem;
  width: 1.1rem;
  height: 1.1rem;
  border-radius: 9999px;
  background: var(--text-primary);
  transition: transform 0.15s ease, background 0.15s ease;
}

.security-lock-switch.on .security-lock-switch-thumb {
  transform: translateX(1.18rem);
  background: var(--text-on-accent);
}
</style>
