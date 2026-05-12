<template>
  <div class="settings-view">
    <div class="settings-scroll">
      <div class="settings-content">
      <h2 class="settings-title">
        <SettingsIcon :size="22" class="title-icon" />
        {{ $t('settings.title') }}
      </h2>

      <!-- AI 模型配置 - 常规动力 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Monitor :size="16" class="section-icon" />
          {{ $t('settings.ai_model') }} - 常规动力
          <Plug :size="16" :class="isMainConnected ? 'icon-success' : 'icon-disabled'" style="margin-left: auto;" title="连接状态" />
        </h3>
        <p class="section-desc" style="margin-bottom: 16px;">用于处理日常对话和高难度逻辑推理的主力模型。</p>

        <div class="form-group">
          <label class="form-label">{{ $t('settings.provider') }}</label>
          <CustomSelect
            v-model="config.provider"
            :options="providerOptions"
            @change="onProviderChange"
          />
        </div>

        <div class="form-group">
          <label class="form-label">{{ $t('settings.api_key') }}</label>
          <div style="position: relative;">
            <input
              v-model="config.apiKey"
              :type="showApiKey ? 'text' : 'password'"
              class="input"
              :placeholder="config._hasApiKey ? '已配置 (点击修改)' : '请输入 API Key (留空使用环境默认)'"
              style="padding-right: 36px;"
              @focus="onApiKeyFocus('apiKey')"
              @blur="onApiKeyBlur('apiKey')"
            />
            <button class="btn-icon toggle-key" @click="showApiKey = !showApiKey">
              <EyeOff v-if="showApiKey" :size="16" />
              <Eye v-else :size="16" />
            </button>
          </div>
        </div>

        <div class="form-group">
          <label class="form-label">{{ $t('settings.api_url') }} (可选，留空使用默认)</label>
          <input
            v-model="config.baseURL"
            class="input"
            :placeholder="config._defaultBaseURL || '留空则使用服务商默认接口...'"
            @blur="saveConfig('baseURL', config.baseURL)"
          />
        </div>

        <div class="form-group">
          <label class="form-label">{{ $t('settings.default_model') }}</label>
          <CustomSelect
            v-model="config.model"
            :options="computedModelOptions"
            @change="saveConfig('model', config.model)"
          />
        </div>

        <!-- 连接测试 -->
        <div class="test-section">
          <button class="btn btn-ghost" @click="testConnection('main')" :disabled="isTesting">
            <Loader2 v-if="isTesting" :size="14" class="animate-spin" />
            <Plug v-else :size="14" />
            <span>{{ isTesting ? $t('settings.testing') : $t('settings.test_connection') }}</span>
          </button>
          <span v-if="testResult" class="test-result" :class="testResult.ok ? 'success' : 'error'">
            {{ testResult.message }}
          </span>
        </div>
      </section>

      <!-- AI 模型配置 - 牛马之力 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Tractor :size="16" class="section-icon" />
          {{ $t('settings.ai_model') }} - 牛马之力
          <Plug :size="16" :class="isClerkConnected ? 'icon-success' : 'icon-disabled'" style="margin-left: auto;" title="连接状态" />
        </h3>
        <p class="section-desc" style="margin-bottom: 16px;">用于后台处理杂活（如文件夹速读、Session 压缩）的极简模型（建议配置低价模型如 doubao-1.6-lite，全面降低成本）。</p>

        <div class="form-group">
          <label class="form-label">{{ $t('settings.provider') }}</label>
          <CustomSelect
            v-model="config.clerkProvider"
            :options="providerOptions"
            @change="onClerkProviderChange"
          />
        </div>

        <div class="form-group" v-if="config.clerkProvider !== 'ollama'">
          <label class="form-label">{{ $t('settings.api_key') }}</label>
          <div style="position: relative;">
            <input
              v-model="config.clerkApiKey"
              :type="showClerkApiKey ? 'text' : 'password'"
              class="input"
              placeholder="请输入 API Key (留空使用环境默认)"
              style="padding-right: 36px;"
              @blur="saveConfig('clerkApiKey', config.clerkApiKey)"
            />
            <button class="btn-icon toggle-key" @click="showClerkApiKey = !showClerkApiKey">
              <EyeOff v-if="showClerkApiKey" :size="16" />
              <Eye v-else :size="16" />
            </button>
          </div>
        </div>

        <div class="form-group">
          <label class="form-label">{{ $t('settings.api_url') }} (可选，留空使用默认)</label>
          <input
            v-model="config.clerkBaseURL"
            class="input"
            placeholder="留空则使用服务商默认接口..."
            @blur="saveConfig('clerkBaseURL', config.clerkBaseURL)"
          />
        </div>

        <div class="form-group">
          <label class="form-label">{{ $t('settings.default_model') }}</label>
          <input
            v-model="config.clerkModel"
            class="input"
            list="clerk-model-list"
            placeholder="选择或手动输入模型名称..."
            @blur="saveConfig('clerkModel', config.clerkModel)"
          />
          <datalist id="clerk-model-list">
            <option v-for="opt in computedClerkModelOptions" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
          </datalist>
        </div>

        <!-- 连接测试 -->
        <div class="test-section">
          <button class="btn btn-ghost" @click="testConnection('clerk')" :disabled="isClerkTesting">
            <Loader2 v-if="isClerkTesting" :size="14" class="animate-spin" />
            <Plug v-else :size="14" />
            <span>{{ isClerkTesting ? $t('settings.testing') : $t('settings.test_connection') }}</span>
          </button>
          <span v-if="clerkTestResult" class="test-result" :class="clerkTestResult.ok ? 'success' : 'error'">
            {{ clerkTestResult.message }}
          </span>
        </div>
      </section>

      <!-- 外观 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Palette :size="16" class="section-icon" />
          {{ $t('settings.appearance') }}
        </h3>
        <div class="form-group">
          <label class="form-label">{{ $t('settings.theme') }}</label>
          <CustomSelect
            v-model="config.theme"
            :options="themeOptions"
            @change="applyTheme(config.theme)"
          />
        </div>
        <div class="form-group">
          <label class="form-label">{{ $t('settings.ui_scale') }}</label>
          <CustomSelect
            v-model="config.uiScale"
            :options="uiScaleOptions"
            @change="applyUiScale(config.uiScale)"
          />
        </div>
        <div class="form-group" style="margin-top: 12px;">
          <label class="form-label">专属强调色 (Accent Color)</label>
          <div style="display: flex; gap: 12px; flex-wrap: wrap; margin-top: 8px;">
            <button 
              v-for="color in accentColors" 
              :key="color.value"
              class="accent-circle"
              :class="{ active: config.accentColor === color.value }"
              :style="{ backgroundColor: color.value }"
              @click="applyAccentColor(color.value)"
              :title="color.name"
            ></button>
          </div>
        </div>
      </section>

      <!-- 语言 (直接嵌入外观区后面) -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Globe :size="16" class="section-icon" />
          {{ $t('settings.language') }}
        </h3>
        <div class="form-group">
          <CustomSelect
            v-model="currentLocale"
            :options="languageOptions"
            @change="switchLanguage"
          />
        </div>
      </section>

      <!-- 工作目录 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <FolderOpen :size="16" class="section-icon" />
          {{ $t('settings.workspace') }}
        </h3>
        <p class="section-desc">{{ $t('settings.workspace_desc') }}</p>
        <div class="form-group workspace-group">
          <input
            v-model="config.workspaceDir"
            class="input"
            :placeholder="$t('settings.workspace_placeholder')"
            readonly
          />
          <button class="btn btn-ghost browse-btn" @click="selectWorkspaceDir">
            <FolderOpen :size="14" />
            <span>{{ $t('settings.browse') }}</span>
          </button>
        </div>
        <button
          v-if="config.workspaceDir"
          class="btn-clear"
          @click="clearWorkspaceDir"
        >
          {{ $t('settings.clear_workspace') }}
        </button>
      </section>

      <!-- 工具与扩展 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Puzzle :size="16" class="section-icon" />
          {{ $t('settings.skills') }}
        </h3>
        <p class="section-desc">{{ $t('settings.skills_desc') }}</p>
        <div class="form-group workspace-group">
          <input
            v-model="config.externalSkillsDir"
            class="input"
            :placeholder="$t('settings.skills_placeholder')"
            readonly
          />
          <button class="btn btn-ghost browse-btn" @click="selectExternalSkillsDir">
            <FolderOpen :size="14" />
            <span>{{ $t('settings.browse') }}</span>
          </button>
        </div>
        <button
          v-if="config.externalSkillsDir"
          class="btn-clear"
          @click="clearExternalSkillsDir"
        >
          {{ $t('settings.clear_skills') }}
        </button>

        <div class="plugin-manager-entry" style="margin-top: 16px; padding-top: 16px; border-top: 1px solid var(--border-subtle);">
          <p class="section-desc" style="margin-bottom: 12px;">{{ $t('settings.plugin_center_desc') }}</p>
          <button class="btn btn-secondary" @click="showPluginManager = true" style="display: flex; align-items: center; gap: 8px;">
            <Layers :size="16" />
            <span>{{ $t('settings.open_plugin_center') }}</span>
          </button>
        </div>
      </section>

      <!-- 插件管理弹窗 -->
      <PluginManager :isOpen="showPluginManager" @close="showPluginManager = false" />

      <!-- MCP Servers -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Unplug :size="16" class="section-icon" />
          {{ $t('settings.mcp_servers') }}
        </h3>
        <p class="section-desc">{{ $t('settings.mcp_desc') }}</p>

        <div v-if="Object.keys(mcpServers).length > 0" class="tracked-folders-list">
          <div
            v-for="(cfg, name) in mcpServers"
            :key="name"
            class="tracked-folder-item"
          >
            <div class="folder-info">
              <span class="folder-name">{{ name }}</span>
              <span class="folder-path">{{ cfg.command }} {{ (cfg.args || []).join(' ') }}</span>
            </div>
            <button class="btn-icon btn-remove-folder" @click="removeMcpServer(name)" title="删除">
              <X :size="14" />
            </button>
          </div>
        </div>
        <div v-else class="empty-folders">
          <span>{{ $t('settings.mcp_empty') }}</span>
        </div>

        <!-- 添加 MCP Server -->
        <div v-if="showAddMcp" class="mcp-add-form">
          <div class="form-group">
            <label class="form-label">{{ $t('settings.mcp_name') }}</label>
            <input v-model="newMcp.name" class="input" placeholder="例如 filesystem" />
          </div>
          <div class="form-group">
            <label class="form-label">{{ $t('settings.mcp_command') }}</label>
            <input v-model="newMcp.command" class="input" placeholder="npx" />
          </div>
          <div class="form-group">
            <label class="form-label">{{ $t('settings.mcp_args') }}</label>
            <input v-model="newMcp.args" class="input" placeholder="-y @modelcontextprotocol/server-filesystem /path" />
          </div>
          <div style="display: flex; gap: 8px; margin-top: 8px;">
            <button class="btn btn-primary" @click="addMcpServer" :disabled="!newMcp.name || !newMcp.command">{{ $t('settings.mcp_save') }}</button>
            <button class="btn btn-ghost" @click="showAddMcp = false">{{ $t('settings.mcp_cancel') }}</button>
          </div>
        </div>
        <button v-else class="btn btn-ghost" @click="showAddMcp = true" style="margin-top: 12px;">
          <Plus :size="14" />
          <span>{{ $t('settings.mcp_add') }}</span>
        </button>
      </section>

      <!-- 关注的文件夹 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <FolderHeart :size="16" class="section-icon" />
          {{ $t('settings.tracked_folders') }}
        </h3>
        <p class="section-desc">{{ $t('settings.tracked_folders_desc') }}</p>

        <div v-if="trackedFolders.length > 0" class="tracked-folders-list">
          <div
            v-for="folder in trackedFolders"
            :key="folder.id"
            class="tracked-folder-item"
          >
            <div class="folder-info">
              <span class="folder-name">{{ folder.name }}</span>
              <span class="folder-path">{{ folder.path }}</span>
            </div>
            <button class="btn-icon btn-remove-folder" @click="removeFolder(folder.path)" title="取消关注">
              <X :size="14" />
            </button>
          </div>
        </div>
        <div v-else class="empty-folders">
          <span>{{ $t('settings.tracked_folders_empty') }}</span>
        </div>

        <button class="btn btn-ghost" @click="addFolder" style="margin-top: 12px;">
          <Plus :size="14" />
          <span>{{ $t('settings.add_folder') }}</span>
        </button>
      </section>

      <!-- 数据管理 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <HardDrive :size="16" class="section-icon" />
          {{ $t('settings.data_management') || '数据管理' }}
        </h3>
        <p class="section-desc" style="margin-bottom: 16px;">
          所有内部配置及对话记录保存在系统的隐藏目录（AppData）中。绿色版卸载时可手动清理。
        </p>

        <div class="form-group" style="display: flex; gap: 12px; margin-top: 16px;">
          <button class="btn btn-secondary" @click="openDataDir">
            <FolderOpen :size="14" />
            打开内部数据目录
          </button>
          
          <button class="btn btn-ghost" style="color: var(--color-error); border-color: var(--color-error);" @click="factoryReset">
            <Trash2 :size="14" />
            清空所有内部数据
          </button>
        </div>
      </section>

      <!-- 关于 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Info :size="16" class="section-icon" />
          {{ $t('settings.about') }}
        </h3>
        <div class="about-info">
          <p>bob-agent v0.1.0</p>
          <p class="about-desc">{{ $t('settings.about_desc') }}</p>
        </div>
      </section>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue';
import { Settings as SettingsIcon, Monitor, Tractor, Eye, EyeOff, Plug, Loader2, Palette, Info, FolderOpen, FolderHeart, Puzzle, Layers, X, Plus, Unplug, Globe, HardDrive, Trash2 } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import CustomSelect from '../components/CustomSelect.vue';
import PluginManager from '../components/PluginManager.vue';

const emit = defineEmits(['config-changed']);
const { locale, t } = useI18n();
const currentLocale = ref('zh-CN');

const languageOptions = [
  { label: '简体中文', value: 'zh-CN' },
  { label: 'English', value: 'en-US' },
];

function switchLanguage(val) {
  locale.value = val || currentLocale.value;
  saveConfig('language', locale.value);
}

const providerOptions = [
  { label: 'DeepSeek', value: 'deepseek' },
  { label: 'OpenAI', value: 'openai' },
  { label: '通义千问 (Qwen)', value: 'qwen' },
  { label: '豆包 (Doubao)', value: 'doubao' },
  { label: '智谱 AI (GLM)', value: 'zhipu' },
  { label: 'Kimi (Moonshot)', value: 'kimi' },
  { label: 'MiniMax', value: 'minimax' },
  { label: '自定义', value: 'custom' },
];

const accentColors = [
  { name: 'MallOS 蓝', value: '#2776bb' },
  { name: '青灰', value: '#627C8C' },
  { name: '淡紫灰', value: '#989398' },
  { name: '淡灰蓝', value: '#B9C7D2' },
  { name: '朱红', value: '#E93C35' },
];

const themeOptions = computed(() => [
  { label: t('settings.theme_dark'), value: 'dark' },
  { label: t('settings.theme_light'), value: 'light' },
]);

const uiScaleOptions = computed(() => [
  { label: t('settings.scale_compact'), value: 'compact' },
  { label: t('settings.scale_comfortable'), value: 'comfortable' },
]);

function applyUiScale(scale, persist = true) {
  document.documentElement.setAttribute('data-ui-scale', scale);
  if (persist) {
    saveConfig('uiScale', scale);
  }
}

function applyTheme(theme, persist = true) {
  document.documentElement.setAttribute('data-theme', theme);
  if (persist) {
    saveConfig('theme', theme);
  }
  if (window.electronAPI.updateTheme) {
    window.electronAPI.updateTheme(theme);
  }
}

function applyAccentColor(color) {
  config.value.accentColor = color;
  document.documentElement.style.setProperty('--user-accent', color);
  const hex = color.replace('#', '');
  const r = parseInt(hex.substring(0, 2), 16);
  const g = parseInt(hex.substring(2, 4), 16);
  const b = parseInt(hex.substring(4, 6), 16);
  document.documentElement.style.setProperty('--user-accent-rgb', `${r}, ${g}, ${b}`);
  saveConfig('accentColor', color);
  emit('config-changed');
}

const config = ref({
  provider: 'deepseek',
  apiKey: '',
  model: '',
  baseURL: '',
  _defaultBaseURL: '',
  clerkProvider: 'deepseek',
  clerkApiKey: '',
  clerkModel: '',
  clerkBaseURL: '',
  _defaultClerkBaseURL: '',
  theme: 'dark',
  uiScale: 'compact',
  workspaceDir: '',
  externalSkillsDir: '',
  accentColor: '',
});

const availableModels = ref([]);
const computedModelOptions = computed(() => {
  return availableModels.value.map(m => ({
    label: `${m.label} (${m.id})`,
    value: m.id
  }));
});

const availableClerkModels = ref([]);
const computedClerkModelOptions = computed(() => {
  return availableClerkModels.value.map(m => ({
    label: `${m.label} (${m.id})`,
    value: m.id
  }));
});

const showApiKey = ref(false);
const showClerkApiKey = ref(false);
const isTesting = ref(false);
const testResult = ref(null);
const isMainConnected = computed(() => {
  if (testResult.value) return testResult.value.ok;
  return config.value._hasApiKey || false;
});

const isClerkTesting = ref(false);
const clerkTestResult = ref(null);
const isClerkConnected = computed(() => {
  if (clerkTestResult.value) return clerkTestResult.value.ok;
  return config.value._hasClerkApiKey || false;
});

const showPluginManager = ref(false);
const trackedFolders = ref([]);

onMounted(async () => {
  const allConfig = await window.electronAPI.getAllConfig();
  config.value = {
    provider: allConfig.provider || 'deepseek',
    apiKey: allConfig.apiKey || '',
    model: allConfig.model || '',
    baseURL: allConfig.baseURL || '',
    _defaultBaseURL: allConfig._defaultBaseURL || '',
    _hasApiKey: allConfig._hasApiKey || false,
    clerkProvider: allConfig.clerkProvider || 'deepseek',
    clerkApiKey: allConfig.clerkApiKey || '',
    clerkModel: allConfig.clerkModel || '',
    clerkBaseURL: allConfig.clerkBaseURL || '',
    _defaultClerkBaseURL: allConfig._defaultClerkBaseURL || '',
    _hasClerkApiKey: allConfig._hasClerkApiKey || false,
    theme: allConfig.theme || 'dark',
    uiScale: allConfig.uiScale || 'compact',
    workspaceDir: allConfig.workspaceDir || '',
    externalSkillsDir: allConfig.externalSkillsDir || '',
    language: allConfig.language || 'zh-CN',
    accentColor: allConfig.accentColor || '',
  };
  // 恢复用户选择的语言
  currentLocale.value = config.value.language;
  locale.value = config.value.language;
  applyUiScale(config.value.uiScale, false);
  await loadModels();
  await loadTrackedFolders();
  await loadMcpConfig();
});

async function loadModels() {
  availableModels.value = await window.electronAPI.getModels(config.value.provider);
  availableClerkModels.value = await window.electronAPI.getModels(config.value.clerkProvider);
}

async function saveConfig(key, value) {
  await window.electronAPI.setConfig(key, value);
  emit('config-changed');
}

// Track the masked key so we don't save it back by accident
const apiKeyOriginalMask = ref('');
const clerkApiKeyOriginalMask = ref('');

function onApiKeyFocus(field) {
  const mask = field === 'apiKey' ? config.value.apiKey : config.value.clerkApiKey;
  if (field === 'apiKey') {
    apiKeyOriginalMask.value = mask;
    // If the current value is a masked key, clear it for fresh input
    if (mask && mask.includes('...')) config.value.apiKey = '';
  } else {
    clerkApiKeyOriginalMask.value = mask;
    if (mask && mask.includes('...')) config.value.clerkApiKey = '';
  }
}

async function onApiKeyBlur(field) {
  const val = field === 'apiKey' ? config.value.apiKey : config.value.clerkApiKey;
  const original = field === 'apiKey' ? apiKeyOriginalMask.value : clerkApiKeyOriginalMask.value;

  if (!val) {
    // User cleared the field without entering new key — restore mask
    if (field === 'apiKey') config.value.apiKey = original;
    else config.value.clerkApiKey = original;
    return;
  }

  // Only save if it's a new, non-masked value
  if (val !== original && !val.includes('...')) {
    await saveConfig(field, val);
    // Update the _has flag and re-mask
    if (field === 'apiKey') {
      config.value._hasApiKey = true;
      const masked = val.length > 8 ? val.substring(0, 5) + '...' + val.substring(val.length - 4) : '\u2022\u2022\u2022\u2022\u2022\u2022';
      config.value.apiKey = masked;
      apiKeyOriginalMask.value = masked;
    } else {
      config.value._hasClerkApiKey = true;
      const masked = val.length > 8 ? val.substring(0, 5) + '...' + val.substring(val.length - 4) : '\u2022\u2022\u2022\u2022\u2022\u2022';
      config.value.clerkApiKey = masked;
      clerkApiKeyOriginalMask.value = masked;
    }
  }
}

async function onProviderChange() {
  await saveConfig('provider', config.value.provider);
  availableModels.value = await window.electronAPI.getModels(config.value.provider);
  // 自动选择默认模型
  const defaultModel = availableModels.value.find(m => m.default);
  if (defaultModel) {
    config.value.model = defaultModel.id;
    await saveConfig('model', defaultModel.id);
  }
}

async function onClerkProviderChange() {
  await saveConfig('clerkProvider', config.value.clerkProvider);
  availableClerkModels.value = await window.electronAPI.getModels(config.value.clerkProvider);
  // 自动选择默认模型
  const defaultModel = availableClerkModels.value.find(m => m.default);
  if (defaultModel) {
    config.value.clerkModel = defaultModel.id;
    await saveConfig('clerkModel', defaultModel.id);
  }
}

async function testConnection(target = 'main') {
  const isMain = target === 'main';
  if (isMain) {
    isTesting.value = true;
    testResult.value = null;
  } else {
    isClerkTesting.value = true;
    clerkTestResult.value = null;
  }

  try {
    const result = await window.electronAPI.sendChat([
      { role: 'user', content: '你好，请回复"连接成功"' }
    ], { useClerk: !isMain }); // 传一个标识给后端

    if (result.error) {
      if (isMain) testResult.value = { ok: false, message: result.error };
      else clerkTestResult.value = { ok: false, message: result.error };
    } else {
      if (isMain) testResult.value = { ok: true, message: '连接成功' };
      else clerkTestResult.value = { ok: true, message: '连接成功' };
    }
  } catch (err) {
    if (isMain) testResult.value = { ok: false, message: err.message };
    else clerkTestResult.value = { ok: false, message: err.message };
  } finally {
    if (isMain) isTesting.value = false;
    else isClerkTesting.value = false;
  }
}

function openDataDir() {
  if (window.electronAPI.openDataDir) {
    window.electronAPI.openDataDir();
  }
}

async function factoryReset() {
  if (confirm('警告：您确定要清空所有聊天记录、配置、记忆和临时技能吗？此操作无法撤销。程序清理后将自动重启。')) {
    if (window.electronAPI.factoryReset) {
      await window.electronAPI.factoryReset();
    }
  }
}

async function selectWorkspaceDir() {
  const dirPath = await window.electronAPI.selectWorkspaceDir();
  if (dirPath) {
    config.value.workspaceDir = dirPath;
    await saveConfig('workspaceDir', dirPath);
  }
}

async function clearWorkspaceDir() {
  config.value.workspaceDir = '';
  await saveConfig('workspaceDir', '');
}

async function selectExternalSkillsDir() {
  const dirPath = await window.electronAPI.selectDir();
  if (dirPath) {
    config.value.externalSkillsDir = dirPath;
    await saveConfig('externalSkillsDir', dirPath);
  }
}

async function clearExternalSkillsDir() {
  config.value.externalSkillsDir = '';
  await saveConfig('externalSkillsDir', '');
}

// ── 文件夹跟踪 ────────────────────────────────────────
async function loadTrackedFolders() {
  trackedFolders.value = await window.electronAPI.getTrackedFolders();
}

async function addFolder() {
  const dirPath = await window.electronAPI.selectFolderToTrack();
  if (dirPath) {
    await window.electronAPI.addTrackedFolder(dirPath);
    await loadTrackedFolders();
  }
}

async function removeFolder(folderPath) {
  await window.electronAPI.removeTrackedFolder(folderPath);
  await loadTrackedFolders();
}

// ── MCP 配置管理 ─────────────────────────────────────
const mcpServers = ref({});
const showAddMcp = ref(false);
const newMcp = ref({ name: '', command: '', args: '' });

async function loadMcpConfig() {
  if (!window.electronAPI.getMcpConfig) return;
  const config = await window.electronAPI.getMcpConfig();
  mcpServers.value = config.mcpServers || {};
}

async function addMcpServer() {
  const name = newMcp.value.name.trim();
  if (!name) return;
  const updated = { ...mcpServers.value };
  updated[name] = {
    command: newMcp.value.command.trim(),
    args: newMcp.value.args.trim().split(/\s+/).filter(Boolean),
  };
  await window.electronAPI.setMcpConfig({ mcpServers: updated });
  mcpServers.value = updated;
  newMcp.value = { name: '', command: '', args: '' };
  showAddMcp.value = false;
}

async function removeMcpServer(name) {
  const updated = { ...mcpServers.value };
  delete updated[name];
  await window.electronAPI.setMcpConfig({ mcpServers: updated });
  mcpServers.value = updated;
}
</script>

<style scoped>
.settings-view {
  flex: 1;
  min-width: 0;
  height: 100%;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.settings-scroll {
  height: 100%;
  overflow-y: auto;
  padding: var(--space-6) var(--space-8);
}

.settings-content {
  padding: 0;
  max-width: 1000px;
  width: 100%;
  margin: 0 auto;
  box-sizing: border-box;
}

.settings-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-2xl);
  font-weight: 600;
  margin-bottom: var(--space-6);
}

.title-icon {
  color: var(--text-secondary);
}

.section-icon {
  color: var(--text-tertiary);
  vertical-align: middle;
}

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

.toggle-key {
  position: absolute;
  right: var(--space-2);
  top: 50%;
  transform: translateY(-50%);
}

select.input {
  cursor: pointer;
  appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath fill='%23a0a0b8' d='M6 8L1 3h10z'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 12px center;
  padding-right: 32px;
}

.test-section {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-top: var(--space-3);
}

.test-result {
  font-size: var(--text-sm);
}

.test-result.success {
  color: var(--user-accent);
}

.test-result.error {
  color: var(--color-error);
}

.about-info {
  color: var(--text-secondary);
  font-size: var(--text-sm);
}

.about-desc {
  color: var(--text-tertiary);
  margin-top: var(--space-1);
}

.section-desc {
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  margin-bottom: var(--space-4);
}

.workspace-group {
  display: flex;
  gap: var(--space-2);
  align-items: center;
}

.workspace-group .input {
  flex: 1;
  cursor: default;
}

.browse-btn {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  white-space: nowrap;
  flex-shrink: 0;
}

.btn-clear {
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  font-size: var(--text-xs);
  cursor: pointer;
  padding: var(--space-1) 0;
  margin-top: var(--space-2);
  font-family: var(--font-sans);
  transition: color var(--duration-fast);
}

.btn-clear:hover {
  color: var(--color-error);
}

/* ── 关注的文件夹 ── */
.tracked-folders-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 12px;
}

.tracked-folder-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  background: var(--bg-secondary);
  border-radius: 8px;
  border: 1px solid var(--border-subtle);
  transition: border-color var(--duration-fast);
}

.tracked-folder-item:hover {
  border-color: var(--border-default);
}

.folder-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.folder-name {
  font-weight: 600;
  font-size: 13px;
  color: var(--text-primary);
}

.folder-path {
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.btn-remove-folder {
  flex-shrink: 0;
  opacity: 0.4;
  transition: opacity var(--duration-fast), color var(--duration-fast);
}

.btn-remove-folder:hover {
  opacity: 1;
  color: var(--color-error);
}

.empty-folders {
  padding: 16px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 13px;
  border: 1px dashed var(--border-subtle);
  border-radius: 8px;
  margin-top: 12px;
}

.mcp-add-form {
  margin-top: 16px;
  padding: 12px;
  background: var(--surface-glass);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
}

.icon-success {
  color: var(--user-accent);
  transition: color var(--duration-fast);
}

.icon-disabled {
  color: var(--text-tertiary);
  opacity: 0.4;
  transition: color var(--duration-fast), opacity var(--duration-fast);
}

.accent-circle {
  width: 24px;
  height: 24px;
  border-radius: 12px;
  border: 2px solid transparent;
  cursor: pointer;
  transition: transform 0.2s, box-shadow 0.2s;
}

.accent-circle:hover {
  transform: scale(1.1);
}

.accent-circle.active {
  border-color: var(--text-primary);
  transform: scale(1.1);
  box-shadow: 0 0 0 2px var(--bg-primary), 0 0 0 4px var(--text-primary);
}
</style>
