<template>
  <section class="settings-section card model-hub">
    <h3 class="section-title">
      <Cpu :size="16" class="section-icon" />
      {{ $t('model_hub.title') }}
      <span class="pool-badge" v-if="pool.length">{{ $t('model_hub.models_available', { count: pool.length }) }}</span>
      <button class="btn-icon rescan-btn" @click="rescan" :disabled="isScanning" :title="$t('model_hub.rescan')">
        <RefreshCw :size="14" :class="{ 'animate-spin': isScanning }" />
      </button>
    </h3>
    <p class="section-desc">{{ $t('model_hub.desc') }}</p>

    <!-- 角色分配卡片 -->
    <div class="role-cards">
      <div class="role-card" :class="{ active: expandedRole === 'main' }" @click="toggleRole('main')">
        <div class="role-header">
          <Monitor :size="20" class="role-icon" style="color: var(--accent-primary);" />
          <div class="role-info">
            <span class="role-label">{{ $t('model_hub.main_model') }}</span>
            <span class="role-model-name">{{ getModelDisplay(activeMain) }}</span>
          </div>
        </div>
      </div>
      <div class="role-card" :class="{ active: expandedRole === 'clerk' }" @click="toggleRole('clerk')">
        <div class="role-header">
          <Tractor :size="20" class="role-icon" style="color: var(--accent-primary); opacity: 0.8;" />
          <div class="role-info">
            <span class="role-label">{{ $t('model_hub.clerk_model') }}</span>
            <span class="role-model-name">{{ getModelDisplay(activeClerk) }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- 模型池列表 -->
    <div class="model-list-container" v-if="pool.length > 0 && expandedRole">
      <div class="model-list-hint">
        {{ expandedRole === 'main' ? $t('model_hub.assign_hint_main') : $t('model_hub.assign_hint_clerk') }}
      </div>
      <div class="provider-selector">
        <label>{{ $t('model_hub.select_provider') }}</label>
        <select class="input provider-select" v-model="activeProvider">
          <option v-for="p in providerList" :key="p.id" :value="p.id" :disabled="!apiKeys[p.id] && p.id !== 'offline'">
            {{ p.name }} ({{ p.count }}) {{ (!apiKeys[p.id] && p.id !== 'offline') ? '(未配置密钥)' : '' }}
          </option>
        </select>
      </div>
      <div class="model-list">
        <div
          v-for="m in currentProviderModels"
          :key="m.id"
          class="model-row"
          :class="{
            'is-main': m.id === activeMain,
            'is-clerk': m.id === activeClerk,
            'is-selectable': expandedRole,
          }"
          @click="expandedRole ? assign(m.id, expandedRole) : null"
        >
          <div class="model-name-col">
            <span class="model-display-name">{{ m.displayName }}</span>
            <span class="model-id-tag">{{ m.modelId }}</span>
          </div>
          <div class="model-actions-col">
            <span v-if="m.id === activeMain" class="role-tag main">{{ $t('model_hub.role_main') }}</span>
            <span v-if="m.id === activeClerk" class="role-tag clerk">{{ $t('model_hub.role_clerk') }}</span>
          </div>
        </div>
      </div>
    </div>
    
    <div v-if="pool.length === 0" class="empty-hint">
      {{ $t('model_hub.no_models') }}
    </div>
  </section>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue';
import { Cpu, RefreshCw, Monitor, Tractor } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';

const { t } = useI18n();

const emit = defineEmits(['model-changed']);

const pool = ref([]);
const activeMain = ref('');
const activeClerk = ref('');
const activeProvider = ref('');
const expandedRole = ref(null);
const isScanning = ref(false);

const groupedModels = computed(() => {
  const groups = {};
  for (const m of pool.value) {
    if (!groups[m.provider]) groups[m.provider] = [];
    groups[m.provider].push(m);
  }
  return groups;
});

const providerList = computed(() => {
  return Object.keys(groupedModels.value).map(p => ({
    id: p,
    name: groupedModels.value[p][0]?.providerName || p,
    count: groupedModels.value[p].length
  }));
});

const currentProviderModels = computed(() => {
  if (!activeProvider.value) return [];
  return groupedModels.value[activeProvider.value] || [];
});

function getModelEntry(id) {
  return pool.value.find(m => m.id === id);
}

function getModelDisplay(id) {
  const entry = getModelEntry(id);
  if (!entry) return id || t('model_hub.not_set');
  return `${entry.providerName} / ${entry.displayName}`;
}

function toggleRole(role) {
  if (expandedRole.value === role) {
    expandedRole.value = null;
  } else {
    expandedRole.value = role;
    const activeModelId = role === 'main' ? activeMain.value : activeClerk.value;
    const entry = getModelEntry(activeModelId);
    if (entry) {
      activeProvider.value = entry.provider;
    }
  }
}

async function assign(modelId, role) {
  const result = await window.electronAPI.assignModelRole(modelId, role);
  if (result?.ok) {
    if (role === 'main') activeMain.value = modelId;
    else activeClerk.value = modelId;
    expandedRole.value = null;
    emit('model-changed');
  }
}

async function rescan() {
  isScanning.value = true;
  try {
    await window.electronAPI.rescanModels();
    pool.value = await window.electronAPI.getModelPool();
    if (providerList.value.length > 0 && !activeProvider.value) {
      activeProvider.value = providerList.value[0].id;
    }
  } finally {
    isScanning.value = false;
  }
}

const apiKeys = ref({});

async function refreshKeyStatus() {
  if (window.electronAPI.getApiKeys) {
    apiKeys.value = await window.electronAPI.getApiKeys() || {};
  }
}

// Expose so SettingsView can call it if needed, though they already communicate via emit
defineExpose({
  refreshKeyStatus
});

onMounted(async () => {
  pool.value = await window.electronAPI.getModelPool();
  const active = await window.electronAPI.getActiveModels();
  activeMain.value = active?.main || '';
  activeClerk.value = active?.clerk || '';
  await refreshKeyStatus();
  
  if (providerList.value.length > 0) {
    // 默认选中包含主力模型的服务商，或第一个
    const mainEntry = getModelEntry(activeMain.value);
    activeProvider.value = mainEntry ? mainEntry.provider : providerList.value[0].id;
  }
});
</script>

<style scoped>
.model-hub {
  /* Uses parent .settings-section.card styling */
}

.pool-badge {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  background: var(--bg-tertiary);
  padding: 2px 8px;
  border-radius: 999px;
  margin-left: 8px;
  font-weight: 400;
}

.rescan-btn {
  margin-left: auto;
  color: var(--text-secondary);
  transition: color 0.2s;
}
.rescan-btn:hover { color: var(--accent-primary); }

.section-desc {
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  margin-bottom: var(--space-4);
}

/* ── Role Cards ──────────────────────────────── */
.role-cards {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-3);
  margin-bottom: var(--space-4);
}

.role-card {
  background: var(--bg-secondary);
  border: 1.5px solid var(--border-primary);
  border-radius: var(--radius-lg);
  padding: var(--space-3) var(--space-4);
  cursor: pointer;
  transition: all 0.2s;
}
.role-card:hover {
  border-color: var(--accent-primary);
  background: color-mix(in srgb, var(--accent-primary) 5%, var(--bg-secondary));
}
.role-card.active {
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 1px var(--accent-primary);
}

.role-header {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.role-icon {
  font-size: 20px;
  flex-shrink: 0;
}

.role-info {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1;
}

.role-label {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.role-model-name {
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ── Model List & Provider Selector ────────────────────── */
.model-list-container {
  border: 1px solid var(--border-primary);
  border-radius: var(--radius-md);
  overflow: hidden;
}

.provider-selector {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 8px 12px;
  background: var(--bg-tertiary);
  border-bottom: 1px solid var(--border-primary);
  font-size: var(--text-sm);
  color: var(--text-secondary);
}

.provider-selector label {
  white-space: nowrap;
}

.provider-select {
  padding: 4px 8px;
  height: 28px;
  min-width: 160px;
  background: var(--bg-primary);
}

.model-list {
  max-height: 380px;
  overflow-y: auto;
}

.model-list-hint {
  font-size: var(--text-sm);
  color: var(--text-primary);
  padding: 8px 12px;
  background: color-mix(in srgb, var(--accent-primary) 15%, transparent);
  border-radius: var(--radius-sm);
  margin-bottom: var(--space-3);
  font-weight: 500;
  display: flex;
  align-items: center;
}

.model-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-2);
  padding: 6px 12px;
  border-bottom: 1px solid color-mix(in srgb, var(--border-primary) 50%, transparent);
  transition: background 0.15s;
}
.model-row:last-child { border-bottom: none; }
.model-row:hover { background: var(--bg-secondary); }
.model-row.is-selectable { cursor: pointer; }
.model-row.is-selectable:hover { background: color-mix(in srgb, var(--accent-primary) 12%, var(--bg-primary)); }
.model-row.is-main { background: color-mix(in srgb, var(--accent-primary) 8%, var(--bg-primary)); }
.model-row.is-clerk { background: color-mix(in srgb, var(--accent-primary) 5%, var(--bg-primary)); }

.model-name-col {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.model-display-name {
  font-size: var(--text-sm);
  font-weight: 500;
  color: var(--text-primary);
}

.model-id-tag {
  font-size: 10px;
  color: var(--text-tertiary);
  font-family: 'JetBrains Mono', monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.model-actions-col {
  display: flex;
  gap: 4px;
  align-items: center;
}

.btn-micro {
  width: 24px;
  height: 24px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-primary);
  background: transparent;
  cursor: pointer;
  font-size: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s;
  opacity: 0;
}
.model-row:hover .btn-micro { opacity: 1; }
.btn-micro:hover {
  background: var(--accent-primary);
  border-color: var(--accent-primary);
}
.btn-primary-micro {
  opacity: 1 !important;
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 1px var(--accent-primary);
}

.role-tag {
  font-size: 10px;
  padding: 2px 6px;
  border-radius: 999px;
  font-weight: 600;
  letter-spacing: 0.5px;
}
.role-tag.main {
  background: color-mix(in srgb, var(--accent-primary) 20%, transparent);
  color: var(--accent-primary);
}
.role-tag.clerk {
  background: color-mix(in srgb, var(--accent-primary) 15%, transparent);
  color: var(--accent-primary);
  opacity: 0.9;
}

.empty-hint {
  padding: var(--space-6);
  text-align: center;
  color: var(--text-tertiary);
  font-size: var(--text-sm);
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
.animate-spin { animation: spin 1s linear infinite; }
</style>
