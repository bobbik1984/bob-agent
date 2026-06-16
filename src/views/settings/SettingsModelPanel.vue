<template>
  <!-- 模型中心 (ModelHub) — 自动发现，替代旧的手填配置 -->
  <ModelHub ref="modelHubRef" @model-changed="emit('config-changed')" />

  <!-- 离线推理引擎 (Offline Engine) -->
  <section class="settings-section card">
    <h3 class="section-title">
      <Server :size="16" class="section-icon" />
      {{ $t('settings.offline_engine') }}
    </h3>

    
    <div class="form-group workspace-group">
      <input
        v-model="config.offlineModelPath"
        class="input"
        :placeholder="$t('settings.offline_model_placeholder')"
        readonly
      />
      <button class="btn btn-primary browse-btn" @click="selectOfflineModel">
        <FolderOpen :size="14" />
        <span>{{ $t('settings.browse') }}</span>
      </button>
    </div>

    <div style="margin-top: 8px; margin-bottom: 12px;">
      <button class="btn btn-ghost" style="font-size: 0.85em; padding: 4px 0; color: var(--accent-primary); display: flex; align-items: center; gap: 4px;" @click="showLlamaGuide = !showLlamaGuide">
        <Info :size="12" />
        <span>{{ showLlamaGuide ? $t('settings.hide_llama_guide') : $t('settings.show_llama_guide') }}</span>
      </button>
    </div>

    <Transition name="briefing-fade">
      <div v-if="showLlamaGuide" class="card" style="background: var(--bg-secondary); border: 1px dashed var(--border-subtle); padding: 16px; border-radius: var(--radius-md); margin-bottom: 16px; font-size: 0.9em; box-shadow: none;">
        <h4 style="font-size: 1.05em; font-weight: 600; color: var(--text-primary); margin-bottom: 8px;">{{ $t('settings.llama_guide_title') }}</h4>
        <p style="color: var(--text-secondary); margin-bottom: 8px; line-height: 1.5;" v-html="$t('settings.llama_guide_desc')"></p>

        <button class="btn btn-primary" style="display: flex; align-items: center; gap: 6px; font-size: 0.9em; padding: 6px 12px;" @click="openLlamaEngineDir">
          <FolderOpen :size="14" />
          <span>{{ $t('settings.open_llama_dir') }}</span>
        </button>
      </div>
    </Transition>
    
    <div style="display: flex; gap: 8px; align-items: center; margin-top: 12px;">
      <button 
        class="btn" 
        :class="offlineEngineStatus === 'running' ? 'btn-danger' : 'btn-primary'" 
        @click="toggleOfflineEngine"
        :disabled="!config.offlineModelPath"
      >
        <Server :size="14" />
        <span>{{ offlineEngineStatus === 'running' ? $t('settings.offline_engine_stop') : $t('settings.offline_engine_start') }}</span>
      </button>
      
      <span style="font-size: 0.85em; display: flex; align-items: center; gap: 6px;" :style="{ color: offlineEngineStatus === 'running' ? 'var(--accent-primary)' : 'var(--text-tertiary)' }">
        <span class="status-dot" :style="{ background: offlineEngineStatus === 'running' ? 'var(--accent-primary)' : 'var(--text-tertiary)' }" style="width: 8px; height: 8px; border-radius: 50%; display: inline-block;"></span>
        {{ offlineEngineStatus === 'running' ? $t('settings.offline_engine_running') : $t('settings.offline_engine_stopped') }}
      </span>
    </div>
  </section>

  <!-- API 密钥管理 (Credential Store) -->
  <details class="settings-section card custom-model-override">
    <summary class="section-title" style="cursor: pointer; display: flex; align-items: center; justify-content: space-between; margin-bottom: 0;">
      <div style="display: flex; align-items: center; gap: 8px;">
        <Key :size="16" class="section-icon" style="opacity: 0.6;" />
        {{ $t('settings.api_keys_title') }}
      </div>
      <ChevronDown :size="16" class="details-chevron" />
    </summary>


    <!-- 模型供应商密钥 -->
    <h4 style="margin-top: 16px; margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">{{ $t('settings.provider_keys_title') }}</h4>
    <div style="display: flex; flex-direction: column; margin-bottom: 20px;">
      <div class="form-group" v-for="provider in modelProviders" :key="provider.id" style="display: flex; align-items: center; gap: 12px; border-bottom: 1px solid var(--border-subtle); padding: 10px 0; margin-bottom: 0;">
        <label class="form-label" style="width: 160px; margin-bottom: 0; display: flex; align-items: center; gap: 8px;">
          <img v-if="getProviderLogo(provider.id)" :src="getProviderLogo(provider.id)" style="width: 16px; height: 16px; object-fit: contain; border-radius: 2px;" />
          <span style="flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;" :title="$te('providers.' + provider.id) ? $t('providers.' + provider.id) : provider.name">{{ $te('providers.' + provider.id) ? $t('providers.' + provider.id) : provider.name }}</span>
        </label>

        <!-- Vertex AI: 凭证文件上传模式 -->
        <template v-if="provider.id === 'vertex_ai'">
          <span class="status-dot" :style="{ background: gcpCredStatus.configured ? 'var(--accent-primary)' : 'transparent', border: gcpCredStatus.configured ? '2px solid var(--accent-primary)' : '2px solid var(--text-tertiary)' }" style="width: 10px; height: 10px; border-radius: 50%; display: inline-block; flex-shrink: 0;"></span>
          <div class="input" style="flex: 1; display: flex; align-items: center; cursor: default; user-select: none;">
            <span v-if="gcpCredStatus.configured" style="font-size: 0.9em; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;" :title="gcpCredStatus.client_email">
              ✅ {{ gcpCredStatus.project_id }} ({{ gcpCredStatus.client_email }})
            </span>
            <span v-else style="font-size: 0.9em; color: var(--text-tertiary);">{{ $t('settings.not_configured') }}</span>
          </div>
          <button class="btn btn-primary" @click="uploadGcpCredential" style="padding: 4px 10px; font-size: 0.9em; white-space: nowrap;">{{ gcpCredStatus.configured ? '更换凭证' : '上传凭证' }}</button>
          <button v-if="gcpCredStatus.configured" class="btn" @click="testGcpCredential" style="padding: 4px 10px; font-size: 0.9em; white-space: nowrap;">测试</button>
          <button class="btn-icon btn-remove-key" :style="{ visibility: gcpCredStatus.configured ? 'visible' : 'hidden' }" @click="removeGcpCredential" :title="$t('settings.delete_key')">
            <X :size="14" />
          </button>
        </template>

        <!-- 常规 API Key 输入模式 -->
        <template v-else>
          <span class="status-dot" :style="{ background: provider.hasKey ? 'var(--accent-primary)' : 'transparent', border: provider.hasKey ? '2px solid var(--accent-primary)' : '2px solid var(--text-tertiary)' }" style="width: 10px; height: 10px; border-radius: 50%; display: inline-block; flex-shrink: 0;"></span>
          <input 
            v-model="apiKeys[provider.id]" 
            type="password" 
            class="input" 
            :placeholder="provider.hasKey ? $t('settings.configured') : $t('settings.not_configured')" 
            style="flex: 1;" 
          />
          <button class="btn btn-primary" @click="saveApiKey(provider.id)" style="padding: 4px 10px; font-size: 0.9em;">{{ $t('settings.save') }}</button>
          <button class="btn-icon btn-remove-key" :style="{ visibility: provider.hasKey ? 'visible' : 'hidden' }" @click="deleteApiKey(provider.id)" :title="$t('settings.delete_key')">
            <X :size="14" />
          </button>
        </template>
      </div>
    </div>

    <!-- 插件/外部服务密钥 -->
    <h4 style="margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">{{ $t('settings.plugin_keys_title') }}</h4>
    <div style="display: flex; flex-direction: column;">
      <div class="form-group" v-for="provider in toolProviders" :key="provider.id" style="display: flex; align-items: center; gap: 12px; border-bottom: 1px solid var(--border-subtle); padding: 10px 0; margin-bottom: 0;">
        <label class="form-label" style="width: 160px; margin-bottom: 0;">{{ provider.name }}</label>
        <span class="status-dot" :style="{ background: provider.hasKey ? 'var(--accent-primary)' : 'transparent', border: provider.hasKey ? '2px solid var(--accent-primary)' : '2px solid var(--text-tertiary)' }" style="width: 10px; height: 10px; border-radius: 50%; display: inline-block;"></span>
        <input 
          v-model="apiKeys[provider.id]" 
          type="password" 
          class="input" 
          :placeholder="provider.hasKey ? $t('settings.configured') : $t('settings.not_configured')" 
          style="flex: 1;" 
        />
        <button class="btn btn-primary" @click="saveApiKey(provider.id)" style="padding: 4px 10px; font-size: 0.9em;">{{ $t('settings.save') }}</button>
        <button class="btn-icon btn-remove-key" :style="{ visibility: provider.hasKey ? 'visible' : 'hidden' }" @click="deleteApiKey(provider.id)" :title="$t('settings.delete_key')">
          <X :size="14" />
        </button>
      </div>
    </div>

    <!-- 自定义模型配置 -->
    <div style="margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--border-subtle);">
      <h4 style="margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">{{ $t('settings.custom_model_title') }}</h4>
      
      <div style="display: flex; flex-direction: column; gap: 8px; margin-bottom: 12px;">
        <div v-for="cm in customModels" :key="cm.id" style="display: flex; align-items: center; gap: 8px; padding: 6px 8px; background: var(--bg-tertiary); border-radius: 4px; border: 1px solid var(--border-subtle);">
          <span style="font-weight: bold; width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{{ cm.displayName }}</span>
          <span style="flex: 1; font-size: 0.85em; color: var(--text-tertiary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{{ cm.baseUrl }}</span>
          <button class="btn-icon" style="color: var(--color-error); width: 24px; height: 24px;" @click="removeCustomModel(cm.id)" :title="$t('settings.delete')">
            <Trash2 :size="14" />
          </button>
        </div>
      </div>
      
      <div style="display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 8px;">
        <input v-model="newCustomModel.name" class="input" :placeholder="$t('settings.custom_model_name')" style="font-size: 0.85em; padding: 4px 8px;" />
        <input v-model="newCustomModel.id" class="input" :placeholder="$t('settings.custom_model_id')" style="font-size: 0.85em; padding: 4px 8px;" />
        <input v-model="newCustomModel.url" class="input" :placeholder="$t('settings.custom_model_url')" style="font-size: 0.85em; padding: 4px 8px;" />
        <input v-model="newCustomModel.key" class="input" type="password" :placeholder="$t('settings.custom_model_key')" style="grid-column: span 2; font-size: 0.85em; padding: 4px 8px;" />
        <button class="btn btn-primary" @click="addCustomModel" :disabled="!newCustomModel.name || !newCustomModel.url || !newCustomModel.key" style="padding: 4px; font-size: 0.85em;">{{ $t('settings.custom_model_add') }}</button>
      </div>
    </div>

    <!-- 工具凭证状态 -->
    <div v-if="toolStatuses.length > 0" style="margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--border-subtle);">
      <h4 style="margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">{{ $t('settings.tool_status_title') }}</h4>
      <div style="display: flex; flex-wrap: wrap; gap: 8px;">
        <span v-for="tool in toolStatuses" :key="tool.name"
          :title="tool.isActive ? tool.description : ($t('settings.tool_missing') + tool.missingCredentials.join(', '))"
          style="padding: 3px 10px; border-radius: 12px; font-size: 0.8em; display: inline-flex; align-items: center; gap: 6px;"
          :style="{ 
            background: tool.isActive ? 'color-mix(in srgb, var(--accent-primary) 10%, transparent)' : 'color-mix(in srgb, var(--text-tertiary) 10%, transparent)',
            color: tool.isActive ? 'var(--text-primary)' : 'var(--text-tertiary)'
          }"
        >
          <span class="status-dot" :style="{ background: tool.isActive ? 'var(--accent-primary)' : 'var(--text-tertiary)' }" style="width: 6px; height: 6px; border-radius: 50%; display: inline-block;"></span>
          {{ tool.name }}
        </span>
      </div>
    </div>
  </details>

  <!-- 模型供应商注册表编辑器 (Registry Editor) -->
  <details class="settings-section card custom-model-override">
    <summary class="section-title" style="cursor: pointer; display: flex; align-items: center; justify-content: space-between; margin-bottom: 0;">
      <div style="display: flex; align-items: center; gap: 8px;">
        <Database :size="16" class="section-icon" style="opacity: 0.6;" />
        {{ $t('settings.registry_title') }}
      </div>
      <ChevronDown :size="16" class="details-chevron" />
    </summary>


    <div v-if="registryData && registryData.providers" style="margin-top: 16px;">
      <div class="registry-providers-grid">
        <!-- 每个供应商 -->
        <div v-for="(provider, pIdx) in registryData.providers" :key="provider.id"
          class="registry-provider-card" :class="{ 'disabled-provider': !hasProviderKey(provider.id) }">
          <!-- 供应商标题栏 -->
          <div class="registry-provider-header" @click="toggleProviderExpand(provider.id)">
            <img v-if="getProviderLogo(provider.id)" :src="getProviderLogo(provider.id)" class="registry-provider-logo" />
            <span class="registry-provider-title">{{ $te('providers.' + provider.id) ? $t('providers.' + provider.id) : provider.name }}</span>
            <span class="registry-provider-meta-id">{{ provider.id }}</span>
            <span class="registry-provider-meta-count">{{ (provider.models || []).length }} models</span>
            <ChevronDown :size="14" class="registry-provider-chevron" :class="{ expanded: expandedProviders[provider.id] }" />
          </div>

          <!-- 供应商详情（展开时显示） -->
          <div v-if="expandedProviders[provider.id]" class="registry-provider-details">
            <!-- 供应商名称 + Base URL -->
            <div class="registry-form-row">
              <label class="registry-form-label">{{ $t('settings.registry_provider_name') }}</label>
              <input v-model="provider.name" class="input" @input="markRegistryDirty" style="font-size: 0.85em; padding: 4px 8px;" />
            </div>
            <div class="registry-form-row">
              <label class="registry-form-label">{{ $t('settings.registry_base_url') }}</label>
              <input v-model="provider.base_url" class="input" @input="markRegistryDirty" style="font-size: 0.85em; padding: 4px 8px;" />
            </div>
            <div class="registry-form-row">
              <label class="registry-form-label">{{ $t('settings.registry_auto_discover') }}</label>
              <label style="display: flex; align-items: center; gap: 6px; cursor: pointer;">
                <input type="checkbox" v-model="provider.supports_model_list" @change="markRegistryDirty" />
                <span style="font-size: 0.82em; color: var(--text-tertiary);">/v1/models</span>
              </label>
            </div>

            <!-- 模型列表 -->
            <div class="registry-models-section">
              <div class="registry-models-header">
                <span class="registry-models-title">{{ $t('settings.registry_models') }}
                  <span style="font-weight: 400; opacity: 0.7; margin-left: 4px;">({{ getVisibleCount(provider) }}/{{ (provider.models || []).length }})</span>
                </span>
                <div style="display: flex; gap: 4px; align-items: center;">
                  <button class="btn btn-ghost" @click="toggleAllModelsVisibility(provider)" style="padding: 2px 8px; font-size: 0.72em; white-space: nowrap;">
                    {{ getVisibleCount(provider) === (provider.models || []).length ? $t('settings.registry_hide_all') : $t('settings.registry_show_all') }}
                  </button>
                  <button class="btn btn-primary" @click="addModelToProvider(provider)" style="padding: 2px 8px; font-size: 0.78em;">
                    <Plus :size="12" /> {{ $t('settings.registry_add_model') }}
                  </button>
                </div>
              </div>
              <div v-for="(model, mIdx) in (provider.models || [])" :key="mIdx" class="registry-model-row">
                <label class="registry-visible-toggle" :title="model.visible === false ? $t('settings.registry_model_hidden') : $t('settings.registry_model_visible')">
                  <input type="checkbox" :checked="model.visible !== false" @change="e => { model.visible = e.target.checked; markRegistryDirty(); }" />
                </label>
                <input v-model="model.id" class="registry-model-id-input" :class="{ 'model-hidden': model.visible === false }" :placeholder="$t('settings.registry_model_id')" @input="markRegistryDirty" />
                <button class="btn-icon capability-toggle" :class="{ 'active': model.vision }" @click="model.vision = !model.vision; markRegistryDirty()" title="切换视觉 (图片理解) 能力">
                  <ImageIcon :size="14" />
                </button>
                <button class="btn-icon" style="color: var(--color-error); width: 22px; height: 22px; flex-shrink: 0;" @click="removeModelFromProvider(provider, mIdx)" :title="$t('settings.registry_remove_model')">
                  <X :size="12" />
                </button>
              </div>
              <div v-if="!provider.models || provider.models.length === 0" style="font-size: 0.8em; color: var(--text-tertiary); padding: 4px 0;">
                —
              </div>
            </div>

            <!-- 删除供应商 -->
            <div style="display: flex; justify-content: flex-end; padding-top: 6px; border-top: 1px solid var(--border-subtle);">
              <button class="btn btn-danger-ghost" style="padding: 3px 10px; font-size: 0.78em;" @click="removeProvider(pIdx)">
                <Trash2 :size="12" /> {{ $t('settings.registry_remove_provider') }}
              </button>
            </div>
          </div>
        </div>

        <!-- 添加新供应商 -->
        <div class="registry-add-provider-card">
          <span class="registry-add-provider-title">{{ $t('settings.registry_add_provider') }}</span>
          <div class="registry-add-provider-form">
            <input v-model="newProviderForm.id" class="input" :placeholder="$t('settings.registry_new_provider_id')" style="font-size: 0.82em; padding: 4px 8px;" />
            <input v-model="newProviderForm.name" class="input" :placeholder="$t('settings.registry_new_provider_name')" style="font-size: 0.82em; padding: 4px 8px;" />
            <input v-model="newProviderForm.base_url" class="input" :placeholder="$t('settings.registry_new_provider_url')" style="font-size: 0.82em; padding: 4px 8px;" />
            <button class="btn btn-primary" @click="addNewProvider" :disabled="!newProviderForm.id || !newProviderForm.name || !newProviderForm.base_url" style="padding: 6px; font-size: 0.82em; width: 100%;">
              <Plus :size="12" /> {{ $t('settings.registry_add_provider') }}
            </button>
          </div>
        </div>
      </div>

      <!-- 保存 / 重置 -->
      <div style="display: flex; gap: 8px; align-items: center; justify-content: flex-end; padding-top: 12px; border-top: 1px solid var(--border-subtle);">
        <span v-if="registrySaveMsg" style="font-size: 0.82em; color: var(--accent-primary); display: flex; align-items: center; gap: 4px;">
          <Check :size="14" /> {{ registrySaveMsg }}
        </span>
        <button class="btn btn-ghost" @click="resetRegistry" style="padding: 4px 12px; font-size: 0.85em;">{{ $t('settings.registry_reset') }}</button>
        <button class="btn btn-primary" @click="saveRegistry" :disabled="!registryDirty" style="padding: 4px 12px; font-size: 0.85em;">{{ $t('settings.registry_save') }}</button>
      </div>
    </div>
  </details>
</template>

<script setup>
import { ref, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { Server, FolderOpen, Info, Key, ChevronDown, X, Plus, Trash2, Database, Check, Image as ImageIcon } from 'lucide-vue-next';
import ModelHub from '../../components/ModelHub.vue';

const props = defineProps({
  config: { type: Object, required: true },
});
const emit = defineEmits(['config-changed']);
const { t } = useI18n();

// ── 离线推理引擎 ──
const offlineEngineStatus = ref('stopped');
const showLlamaGuide = ref(false);

async function openLlamaEngineDir() {
  await window.electronAPI.openLlamaEngineDir();
}

async function selectOfflineModel() {
  if (window.electronAPI.selectFile) {
    const path = await window.electronAPI.selectFile();
    if (path) {
      props.config.offlineModelPath = path;
      saveConfig('offlineModelPath', path);
    }
  }
}

async function toggleOfflineEngine() {
  if (offlineEngineStatus.value === 'running') {
    const res = await window.electronAPI.stopOfflineEngine();
    if (res && res.status === 'stopped') {
      offlineEngineStatus.value = 'stopped';
    }
  } else {
    if (!props.config.offlineModelPath) return;
    offlineEngineStatus.value = 'starting';
    try {
      const res = await window.electronAPI.startOfflineEngine(props.config.offlineModelPath);
      if (res && res.status === 'running') {
        offlineEngineStatus.value = 'running';
      } else {
        offlineEngineStatus.value = 'stopped';
      }
    } catch(err) {
      offlineEngineStatus.value = 'stopped';
      alert('启动离线引擎失败: ' + err);
    }
  }
}

// ── 凭证管理 (Credential Store) ──
const modelProviders = ref([]);
const toolProviders = ref([
  { id: 'TAVILY_API_KEY', name: 'Tavily (Web Search)', hasKey: false },
  { id: 'TINYFISH_API_KEY', name: 'TinyFish (Fetch)', hasKey: false },
]);
const apiKeys = ref({});
const toolStatuses = ref([]);

const customModels = ref([]);
const newCustomModel = ref({ name: '', url: '', key: '', id: '' });

const modelHubRef = ref(null);

// ── GCP Vertex AI 凭证管理 ──
const gcpCredStatus = ref({ configured: false });

async function loadGcpCredentialStatus() {
  if (window.electronAPI.getGcpCredentialStatus) {
    gcpCredStatus.value = await window.electronAPI.getGcpCredentialStatus();
  }
}

async function uploadGcpCredential() {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const selected = await open({
    multiple: false,
    title: '选择 GCP Service Account JSON 凭证文件',
    filters: [{ name: 'JSON', extensions: ['json'] }],
  });
  if (!selected) return;
  const result = await window.electronAPI.uploadGcpCredential(selected);
  if (result.error) {
    alert('凭证上传失败: ' + result.error);
  } else {
    await loadGcpCredentialStatus();
  }
}

async function testGcpCredential() {
  const result = await window.electronAPI.testGcpCredential();
  if (result.error) {
    alert('❌ 连通性测试失败: ' + result.error);
  } else {
    alert('✅ 连通性测试成功！\nProject: ' + result.project_id + '\nToken: ' + result.token_preview);
  }
}

async function removeGcpCredential() {
  const result = await window.electronAPI.removeGcpCredential();
  if (result.ok) {
    await loadGcpCredentialStatus();
  }
}

// ── 注册表编辑器 ──
const registryData = ref(null);
const registryDirty = ref(false);
const registrySaveMsg = ref('');
const newProviderForm = ref({ id: '', name: '', base_url: '' });
const expandedProviders = ref({});

async function loadRegistryProviders() {
  try {
    const reg = await window.electronAPI.getRegistry();
    registryData.value = reg;
    if (reg && reg.providers) {
      modelProviders.value = reg.providers
        .filter(p => p.id !== 'offline')
        .map(p => ({ id: p.id, name: p.name, hasKey: false }));
    }
  } catch (e) {
    console.warn('Failed to load registry:', e);
    modelProviders.value = [
      { id: 'deepseek', name: 'DeepSeek', hasKey: false },
      { id: 'openai', name: 'OpenAI', hasKey: false },
      { id: 'qwen', name: '通义千问 (Qwen)', hasKey: false },
      { id: 'doubao', name: '豆包 (Doubao)', hasKey: false },
      { id: 'zhipu', name: '智谱 AI (GLM)', hasKey: false },
      { id: 'kimi', name: 'Kimi (Moonshot)', hasKey: false },
      { id: 'minimax', name: 'MiniMax', hasKey: false },
    ];
  }
}

function hasProviderKey(providerId) {
  if (providerId === 'offline') return true;
  const p = modelProviders.value.find(p => p.id === providerId);
  if (p) return p.hasKey;
  return true;
}

function toggleProviderExpand(providerId) {
  if (!hasProviderKey(providerId)) return;
  expandedProviders.value[providerId] = !expandedProviders.value[providerId];
}

function markRegistryDirty() {
  registryDirty.value = true;
  registrySaveMsg.value = '';
}

function addModelToProvider(provider) {
  if (!provider.models) provider.models = [];
  provider.models.push({ id: '', name: '', vision: false, visible: true, pricing: { input: 0, output: 0 } });
  markRegistryDirty();
}

function removeModelFromProvider(provider, index) {
  provider.models.splice(index, 1);
  markRegistryDirty();
}

function getVisibleCount(provider) {
  if (!provider.models) return 0;
  return provider.models.filter(m => m.visible !== false).length;
}

function toggleAllModelsVisibility(provider) {
  if (!provider.models) return;
  const allVisible = getVisibleCount(provider) === provider.models.length;
  // 如果全部可见 → 全部隐藏；否则 → 全部显示
  const newState = !allVisible;
  provider.models.forEach(m => { m.visible = newState; });
  markRegistryDirty();
}

function addNewProvider() {
  const f = newProviderForm.value;
  if (!f.id || !f.name || !f.base_url) return;
  if (!registryData.value) return;
  if (registryData.value.providers.some(p => p.id === f.id)) return;
  registryData.value.providers.push({
    id: f.id,
    name: f.name,
    base_url: f.base_url,
    supports_model_list: false,
    models: [],
  });
  newProviderForm.value = { id: '', name: '', base_url: '' };
  markRegistryDirty();
}

function removeProvider(index) {
  if (!registryData.value) return;
  if (!confirm(t('settings.registry_confirm_remove'))) return;
  registryData.value.providers.splice(index, 1);
  markRegistryDirty();
  modelProviders.value = registryData.value.providers
    .filter(p => p.id !== 'offline')
    .map(p => ({ id: p.id, name: p.name, hasKey: false }));
  fetchApiKeys();
}

async function saveRegistry() {
  if (!registryData.value) return;
  try {
    registryData.value.last_updated = new Date().toISOString().slice(0, 10);
    await window.electronAPI.saveRegistry(registryData.value);
    registryDirty.value = false;
    registrySaveMsg.value = t('settings.registry_saved');
    await loadRegistryProviders();
    await fetchApiKeys();
    if (modelHubRef.value) modelHubRef.value.rescan();
    setTimeout(() => { registrySaveMsg.value = ''; }, 3000);
  } catch (e) {
    console.error('Failed to save registry:', e);
  }
}

async function resetRegistry() {
  await loadRegistryProviders();
  registryDirty.value = false;
  registrySaveMsg.value = '';
}

function getProviderLogo(providerId) {
  const name = (providerId || '').toLowerCase();
  if (name.includes('deepseek')) return '/logos/deepseek.png';
  if (name.includes('openai')) return '/logos/openai.png';
  if (name.includes('qwen') || name.includes('dashscope')) return '/logos/qwen.png';
  if (name.includes('doubao')) return '/logos/doubao.png';
  if (name.includes('zhipu')) return '/logos/glm.svg';
  if (name.includes('kimi')) return '/logos/kimi.png';
  if (name.includes('minimax')) return '/logos/minimax.png';
  if (name.includes('vertex')) return '/logos/google.png';
  if (name.includes('gemini') || name.includes('google')) return '/logos/google.png';
  if (name.includes('claude') || name.includes('anthropic')) return '/logos/claude.png';
  return null;
}

async function fetchApiKeys() {
  if (window.electronAPI.getApiKeys) {
    const status = await window.electronAPI.getApiKeys();
    [...modelProviders.value, ...toolProviders.value].forEach(p => {
      p.hasKey = !!status[p.id];
    });
  }
}

async function fetchToolStatuses() {
  if (window.electronAPI.getToolStatuses) {
    const statuses = await window.electronAPI.getToolStatuses();
    toolStatuses.value = statuses;
  }
}

async function saveApiKey(providerId) {
  if (window.electronAPI.setApiKey) {
    const key = apiKeys.value[providerId];
    if (key === undefined || key === null) return;
    await window.electronAPI.setApiKey(providerId, key);
    await fetchApiKeys();
    apiKeys.value[providerId] = '';
    await fetchToolStatuses();
    if (modelHubRef.value) {
      modelHubRef.value.refreshKeyStatus();
    }
    emit('config-changed');
  }
}

async function deleteApiKey(providerId) {
  if (window.electronAPI.setApiKey) {
    await window.electronAPI.setApiKey(providerId, '');
    apiKeys.value[providerId] = '';
    await fetchApiKeys();
    await fetchToolStatuses();
    if (modelHubRef.value) {
      modelHubRef.value.refreshKeyStatus();
    }
    emit('config-changed');
  }
}

async function loadCustomModels() {
  const allConfig = await window.electronAPI.getAllConfig();
  customModels.value = allConfig.customModels || [];
}

async function addCustomModel() {
  if (!newCustomModel.value.name || !newCustomModel.value.url || !newCustomModel.value.key) return;
  const id = newCustomModel.value.id || ('custom-' + Date.now());
  const provider = 'custom_' + id;
  if (window.electronAPI.addCustomModel) {
    await window.electronAPI.addCustomModel(id, newCustomModel.value.name, provider, newCustomModel.value.url, newCustomModel.value.key);
    newCustomModel.value = { name: '', url: '', key: '', id: '' };
    await loadCustomModels();
    if (modelHubRef.value) modelHubRef.value.rescan();
  }
}

async function removeCustomModel(id) {
  if (window.electronAPI.removeCustomModel) {
    await window.electronAPI.removeCustomModel(id);
    await loadCustomModels();
    if (modelHubRef.value) modelHubRef.value.rescan();
  }
}

async function saveConfig(key, value) {
  await window.electronAPI.setConfig(key, value);
  emit('config-changed');
}

// ── Init ──
onMounted(async () => {
  await loadRegistryProviders();
  await fetchApiKeys();
  await loadCustomModels();
  await fetchToolStatuses();
  await loadGcpCredentialStatus();
  if (window.electronAPI.getOfflineEngineStatus) {
    try {
      const res = await window.electronAPI.getOfflineEngineStatus();
      if (res && res.status) {
        offlineEngineStatus.value = res.status;
      }
    } catch(err) {}
  }
});

// Expose for parent to access modelHub
defineExpose({ modelHubRef });
</script>

<style scoped>
.settings-section {
  margin-bottom: var(--space-5);
}

.section-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--text-lg);
  font-weight: 500;
  margin-bottom: var(--space-4);
  color: var(--text-primary);
}

.section-desc {
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  margin-bottom: var(--space-4);
}

.form-group {
  margin-bottom: var(--space-4);
  position: relative;
}

.form-label {
  display: block;
  font-size: var(--text-sm);
  color: var(--text-secondary);
  margin-bottom: var(--space-2);
  font-weight: 500;
}

.workspace-group {
  display: flex;
  gap: var(--space-2);
  align-items: center;
}

.workspace-group .input {
  flex: 1;
  min-width: 0;
  text-overflow: ellipsis;
  cursor: default;
}

.browse-btn {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  white-space: nowrap;
  flex-shrink: 0;
}

details > summary {
  list-style: none;
}
details > summary::-webkit-details-marker {
  display: none;
}
.details-chevron {
  transition: transform 0.2s ease;
  color: var(--text-tertiary);
}
details[open] > summary .details-chevron {
  transform: rotate(180deg);
}

.btn-remove-key {
  flex-shrink: 0;
  color: var(--text-tertiary);
  opacity: 0.6;
  transition: all 0.15s ease;
}
.btn-remove-key:hover {
  color: var(--color-error);
  opacity: 1;
  background: var(--color-error-bg);
}

.btn-danger-ghost {
  background: transparent;
  color: var(--color-error);
  border: 1px solid color-mix(in srgb, var(--color-error) 30%, transparent);
}
.btn-danger-ghost:hover {
  background: var(--color-error-bg);
  border-color: var(--color-error);
}

/* Transition */
.briefing-fade-enter-active {
  transition: all 0.3s ease;
}
.briefing-fade-leave-active {
  transition: all 0.2s ease;
}
.briefing-fade-enter-from {
  opacity: 0;
  transform: scale(0.95);
}
.briefing-fade-leave-to {
  opacity: 0;
  transform: scale(0.95);
}

/* ── Model Provider Registry ── */
.registry-providers-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 16px;
  margin-bottom: 16px;
  align-items: start;
}
@media (max-width: 850px) {
  .registry-providers-grid {
    grid-template-columns: 1fr;
  }
}

.registry-provider-card {
  background: var(--bg-primary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  overflow: hidden;
  display: flex;
  flex-direction: column;
  transition: all 0.2s ease;
}
.registry-provider-card.disabled-provider {
  opacity: 0.55;
  filter: grayscale(100%);
}
.registry-provider-card.disabled-provider .registry-provider-header {
  cursor: not-allowed;
}

.registry-provider-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  background: var(--bg-tertiary);
  cursor: pointer;
  user-select: none;
}

.registry-provider-logo {
  width: 16px;
  height: 16px;
  object-fit: contain;
  border-radius: 2px;
  flex-shrink: 0;
}

.registry-provider-title {
  font-weight: 600;
  flex: 1;
  font-size: 0.9em;
  color: var(--text-primary);
}

.registry-provider-meta-id {
  font-size: 0.78em;
  color: var(--text-tertiary);
  margin-right: 4px;
}

.registry-provider-meta-count {
  font-size: 0.78em;
  color: var(--text-tertiary);
}

.registry-provider-chevron {
  transition: transform 0.2s ease;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.registry-provider-chevron.expanded {
  transform: rotate(180deg);
}

.registry-provider-details {
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  border-top: 1px solid var(--border-subtle);
  background: var(--bg-primary);
}

.registry-form-row {
  display: grid;
  grid-template-columns: 1fr 2.2fr;
  gap: 8px;
  align-items: center;
}

.registry-form-label {
  font-size: 0.82em;
  color: var(--text-secondary);
}

.registry-models-section {
  margin-top: 4px;
}

.registry-models-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
}

.registry-models-title {
  font-size: 0.82em;
  font-weight: 600;
  color: var(--text-secondary);
}

.registry-model-row {
  display: flex;
  gap: 6px;
  align-items: center;
  margin-bottom: 4px;
  padding: 2px 4px;
  border-radius: var(--radius-sm);
  transition: background 0.15s;
}
.registry-model-row:hover {
  background: var(--bg-tertiary);
}

.capability-toggle {
  color: var(--text-tertiary);
  opacity: 0.35;
  width: 22px;
  height: 22px;
  flex-shrink: 0;
  transition: all 0.2s ease;
}
.capability-toggle:hover {
  opacity: 0.8;
  background: var(--bg-hover);
}
.capability-toggle.active {
  color: var(--user-accent, var(--color-success));
  opacity: 1;
}

.registry-model-id-input {
  flex: 1;
  min-width: 0;
  border: 1px solid transparent;
  background: transparent;
  font-size: 0.85em;
  font-weight: 500;
  font-family: var(--font-mono, monospace);
  color: var(--text-primary);
  padding: 4px 6px;
  border-radius: 4px;
  outline: none;
  transition: all 0.2s;
}
.registry-model-id-input:hover, .registry-model-id-input:focus {
  border-color: var(--border-default);
  background: var(--bg-primary);
}

.registry-add-provider-card {
  border: 1px dashed var(--border-default);
  border-radius: 8px;
  padding: 12px 14px;
  background: var(--bg-secondary);
  display: flex;
  flex-direction: column;
}

.registry-add-provider-title {
  font-size: 0.82em;
  font-weight: 600;
  color: var(--text-secondary);
  margin-bottom: 8px;
  display: block;
}

.registry-add-provider-form {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

/* ── Model visibility toggle ── */
.registry-visible-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 22px;
  cursor: pointer;
}

.registry-visible-toggle input[type="checkbox"] {
  accent-color: var(--accent-primary);
  cursor: pointer;
  width: 14px;
  height: 14px;
  margin: 0;
}

.model-hidden {
  opacity: 0.4;
  text-decoration: line-through;
  text-decoration-color: var(--text-tertiary);
}
</style>
