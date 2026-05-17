<template>
  <div class="settings-view">
    <div class="settings-scroll">
      <div class="settings-content">
      <h2 class="settings-title">
        <SettingsIcon :size="22" class="title-icon" />
        {{ $t('settings.title') }}
      </h2>

      <!-- 模型中心 (ModelHub) — 自动发现，替代旧的手填配置 -->
      <ModelHub ref="modelHubRef" @model-changed="emit('config-changed')" />

      <!-- 离线推理引擎 (Offline Engine) -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Server :size="16" class="section-icon" />
          {{ $te('settings.offline_engine') ? $t('settings.offline_engine') : '离线推理引擎 (本地模型)' }}
        </h3>
        <p class="section-desc" style="margin-bottom: 12px;">启动内置的 llama-server 边车以运行本地 GGUF 模型，实现断网计算和绝对隐私。</p>
        
        <div class="form-group workspace-group">
          <input
            v-model="config.offlineModelPath"
            class="input"
            placeholder="请选择本地 .gguf 模型文件路径"
            readonly
          />
          <button class="btn btn-primary browse-btn" @click="selectOfflineModel">
            <FolderOpen :size="14" />
            <span>{{ $te('settings.browse') ? $t('settings.browse') : '浏览' }}</span>
          </button>
        </div>
        
        <div style="display: flex; gap: 8px; align-items: center; margin-top: 12px;">
          <button 
            class="btn" 
            :class="offlineEngineStatus === 'running' ? 'btn-danger' : 'btn-primary'" 
            @click="toggleOfflineEngine"
            :disabled="!config.offlineModelPath"
          >
            <Server :size="14" />
            <span>{{ offlineEngineStatus === 'running' ? '停止本地引擎' : '启动本地引擎' }}</span>
          </button>
          
          <span style="font-size: 0.85em; display: flex; align-items: center; gap: 6px;" :style="{ color: offlineEngineStatus === 'running' ? 'var(--accent-primary)' : 'var(--text-tertiary)' }">
            <span class="status-dot" :style="{ background: offlineEngineStatus === 'running' ? 'var(--accent-primary)' : 'var(--text-tertiary)' }" style="width: 8px; height: 8px; border-radius: 50%; display: inline-block;"></span>
            {{ offlineEngineStatus === 'running' ? '正在运行 (端口: 8080)' : '已停止' }}
          </span>
        </div>
      </section>

      <!-- API 密钥管理 (Credential Store) -->
      <details class="settings-section card custom-model-override">
        <summary class="section-title" style="cursor: pointer; display: flex; align-items: center; justify-content: space-between; margin-bottom: 0;">
          <div style="display: flex; align-items: center; gap: 8px;">
            <Key :size="16" class="section-icon" style="opacity: 0.6;" />
            {{ $te('settings.api_keys_title') ? $t('settings.api_keys_title') : 'API 密钥管理 (安全存储)' }}
          </div>
          <ChevronDown :size="16" class="details-chevron" />
        </summary>
        <p class="section-desc" style="margin-top: 16px; margin-bottom: 16px;">{{ $te('settings.api_keys_desc') ? $t('settings.api_keys_desc') : '所有密钥都已通过操作系统级加密 (safeStorage) 存储在本地，防止未经授权的读取。' }}</p>
        
        <!-- T-821: Outbox 引导提示 -->
        <div style="margin-bottom: 16px; padding: 10px 14px; border-radius: 8px; background: rgba(var(--user-accent-rgb, 99,102,241), 0.08); border: 1px solid rgba(var(--user-accent-rgb, 99,102,241), 0.2); font-size: 0.82em; color: var(--text-secondary); line-height: 1.5;">
          💡 <strong>小提示</strong>：您也可以直接在对话中告诉 Bob "帮我配置这个 API Key"，或者把包含密钥的文件拖拽发送给他，Bob 会自动识别并安全地配置好。
        </div>

        <!-- 模型供应商密钥 -->
        <h4 style="margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">模型供应商密钥</h4>
        <div style="display: flex; flex-direction: column; gap: 6px; margin-bottom: 20px;">
          <div class="form-group" v-for="provider in modelProviders" :key="provider.id" style="display: flex; align-items: center; gap: 12px; border-bottom: 1px solid var(--border-subtle); padding-bottom: 6px;">
            <label class="form-label" style="width: 160px; margin-bottom: 0; display: flex; align-items: center; gap: 8px;">
              <img v-if="getProviderLogo(provider.id)" :src="getProviderLogo(provider.id)" style="width: 16px; height: 16px; object-fit: contain; border-radius: 2px;" @error="(e) => e.target.style.visibility = 'hidden'" />
              <span style="flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;" :title="provider.name">{{ provider.name }}</span>
            </label>
            <span class="status-dot" :style="{ background: provider.hasKey ? 'var(--user-accent)' : 'transparent', border: provider.hasKey ? '2px solid var(--user-accent)' : '2px solid var(--text-tertiary)' }" style="width: 10px; height: 10px; border-radius: 50%; display: inline-block; flex-shrink: 0;"></span>
            <input 
              v-model="apiKeys[provider.id]" 
              type="password" 
              class="input" 
              :placeholder="provider.hasKey ? ($te('settings.configured') ? $t('settings.configured') : '已配置') : ($te('settings.not_configured') ? $t('settings.not_configured') : '未配置')" 
              style="flex: 1;" 
            />
            <button class="btn btn-primary" @click="saveApiKey(provider.id)" style="padding: 4px 10px; font-size: 0.9em;">{{ $te('settings.save') ? $t('settings.save') : '保存' }}</button>
          </div>
        </div>

        <!-- 插件/外部服务密钥 -->
        <h4 style="margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">插件/外部服务密钥</h4>
        <div style="display: flex; flex-direction: column; gap: 12px;">
          <div class="form-group" v-for="provider in toolProviders" :key="provider.id" style="display: flex; align-items: center; gap: 12px; border-bottom: 1px solid var(--border-subtle); padding-bottom: 8px;">
            <label class="form-label" style="width: 140px; margin-bottom: 0;">{{ provider.name }}</label>
            <span class="status-dot" :style="{ background: provider.hasKey ? 'var(--user-accent)' : 'transparent', border: provider.hasKey ? '2px solid var(--user-accent)' : '2px solid var(--text-tertiary)' }" style="width: 10px; height: 10px; border-radius: 50%; display: inline-block;"></span>
            <input 
              v-model="apiKeys[provider.id]" 
              type="password" 
              class="input" 
              :placeholder="provider.hasKey ? ($te('settings.configured') ? $t('settings.configured') : '已配置') : ($te('settings.not_configured') ? $t('settings.not_configured') : '未配置')" 
              style="flex: 1;" 
            />
            <button class="btn btn-primary" @click="saveApiKey(provider.id)" style="padding: 6px 12px;">{{ $te('settings.save') ? $t('settings.save') : '保存' }}</button>
          </div>
        </div>

        <!-- 自定义模型配置 -->
        <div style="margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--border-subtle);">
          <h4 style="margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">自定义模型 (兼容 OpenAI 格式)</h4>
          
          <div style="display: flex; flex-direction: column; gap: 8px; margin-bottom: 12px;">
            <div v-for="cm in customModels" :key="cm.id" style="display: flex; align-items: center; gap: 8px; padding: 6px 8px; background: var(--bg-tertiary); border-radius: 4px; border: 1px solid var(--border-subtle);">
              <span style="font-weight: bold; width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{{ cm.displayName }}</span>
              <span style="flex: 1; font-size: 0.85em; color: var(--text-tertiary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{{ cm.baseUrl }}</span>
              <button class="btn-icon" style="color: var(--status-error); width: 24px; height: 24px;" @click="removeCustomModel(cm.id)" title="删除">
                <Trash2 :size="14" />
              </button>
            </div>
          </div>
          
          <div style="display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 8px;">
            <input v-model="newCustomModel.name" class="input" placeholder="模型名称 (例: Gemini Pro)" style="font-size: 0.85em; padding: 4px 8px;" />
            <input v-model="newCustomModel.id" class="input" placeholder="模型ID (例: gemini-2.5-pro)" style="font-size: 0.85em; padding: 4px 8px;" />
            <input v-model="newCustomModel.url" class="input" placeholder="Base URL" style="font-size: 0.85em; padding: 4px 8px;" />
            <input v-model="newCustomModel.key" class="input" type="password" placeholder="API Key" style="grid-column: span 2; font-size: 0.85em; padding: 4px 8px;" />
            <button class="btn btn-primary" @click="addCustomModel" :disabled="!newCustomModel.name || !newCustomModel.url || !newCustomModel.key" style="padding: 4px; font-size: 0.85em;">添加自定义模型</button>
          </div>
        </div>

        <!-- 工具凭证状态 -->
        <div v-if="toolStatuses.length > 0" style="margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--border-subtle);">
          <h4 style="margin-bottom: 8px; font-size: 0.85em; color: var(--text-secondary);">{{ $te('settings.tool_status_title') ? $t('settings.tool_status_title') : '🔧 工具激活状态' }}</h4>
          <div style="display: flex; flex-wrap: wrap; gap: 8px;">
            <span v-for="tool in toolStatuses" :key="tool.name"
              :title="tool.isActive ? tool.description : ('缺少: ' + tool.missingCredentials.join(', '))"
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
          <label class="form-label">{{ $t('settings.accent_color') }}</label>
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

      <!-- Bob 的工作间（目录管理） -->
      <details class="settings-section card custom-model-override">
        <summary class="section-title" style="cursor: pointer; display: flex; align-items: center; justify-content: space-between; margin-bottom: 0;">
          <div style="display: flex; align-items: center; gap: 8px;">
            <HardDrive :size="16" class="section-icon" style="opacity: 0.6;" />
            {{ $t('settings.bob_workspace') }}
          </div>
          <ChevronDown :size="16" class="details-chevron" />
        </summary>
        <p class="section-desc" style="margin-top: 16px; margin-bottom: 16px;">{{ $t('settings.bob_workspace_desc') }}</p>

        <!-- 工作目录 (workspaceDir) -->
        <div class="details-section">
          <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
            <FolderOpen :size="14" style="opacity: 0.6;" />
            {{ $t('settings.workspace') }}
          </label>
          <p class="section-desc" style="margin-bottom: 8px; font-size: 0.8em;">{{ $t('settings.workspace_desc') }}</p>
          <div class="form-group workspace-group">
            <input
              v-model="config.workspaceDir"
              class="input"
              :placeholder="$t('settings.workspace_placeholder')"
              readonly
            />
            <button class="btn btn-primary browse-btn" @click="selectWorkspaceDir">
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
        </div>

        <div class="details-section">
          <!-- 关注的文件夹 -->
          <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
            <FolderHeart :size="14" style="opacity: 0.6;" />
            {{ $t('settings.tracked_folders') }}
          </label>
          <p class="section-desc" style="margin-bottom: 8px; font-size: 0.8em;">{{ $t('settings.tracked_folders_desc') }}</p>

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

          <button class="btn btn-primary" @click="addFolder" style="margin-top: 12px;">
            <Plus :size="14" />
            <span>{{ $t('settings.add_folder') }}</span>
          </button>
        </div>

        <div class="details-section">
          <!-- 知识库目录 (wikiDir) -->
          <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
            <FileText :size="14" style="opacity: 0.6;" />
            {{ $t('settings.wiki_dir') }}
          </label>
          <p class="section-desc" style="margin-bottom: 8px; font-size: 0.8em;">{{ $t('settings.wiki_dir_desc') }}</p>
          <div class="form-group workspace-group">
            <input
              v-model="config.wikiDir"
              class="input"
              :placeholder="$t('settings.wiki_dir_placeholder')"
              readonly
            />
            <button class="btn btn-primary browse-btn" @click="selectWikiDir">
              <FolderOpen :size="14" />
              <span>{{ $t('settings.browse') }}</span>
            </button>
          </div>
          <button
            v-if="config.wikiDir"
            class="btn-clear"
            @click="clearWikiDir"
          >
            {{ $t('settings.clear_wiki') }}
          </button>
        </div>

        <div class="details-section">
          <!-- MCP Servers -->
          <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
            <Unplug :size="14" style="opacity: 0.6;" />
            {{ $t('settings.mcp_servers') }}
          </label>
          <p class="section-desc" style="margin-bottom: 8px; font-size: 0.8em;">{{ $t('settings.mcp_desc') }}</p>

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
              <button class="btn btn-primary" @click="showAddMcp = false">{{ $t('settings.mcp_cancel') }}</button>
            </div>
          </div>
          <button v-else class="btn btn-primary" @click="showAddMcp = true" style="margin-top: 12px;">
            <Plus :size="14" />
            <span>{{ $t('settings.mcp_add') }}</span>
          </button>
        </div>

        <div class="details-section">
          <!-- 技能目录 (externalSkillsDir) -->
          <label class="form-label" style="font-size: 0.85em; margin-bottom: 6px; display: flex; align-items: center; gap: 6px;">
            <Puzzle :size="14" style="opacity: 0.6;" />
            {{ $t('settings.skills') }}
          </label>
          <p class="section-desc" style="margin-bottom: 8px; font-size: 0.8em;">{{ $t('settings.skills_desc') }}</p>
          <div class="form-group workspace-group">
            <input
              v-model="config.externalSkillsDir"
              class="input"
              :placeholder="$t('settings.skills_placeholder')"
              readonly
            />
            <button class="btn btn-primary browse-btn" @click="selectExternalSkillsDir">
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

          <div class="plugin-manager-entry details-section">
            <p class="section-desc" style="margin-bottom: 12px;">{{ $t('settings.plugin_center_desc') }}</p>
            <button class="btn btn-primary" @click="showPluginManager = true" style="display: flex; align-items: center; gap: 8px;">
              <Layers :size="16" />
              <span>{{ $t('settings.open_plugin_center') }}</span>
            </button>
          </div>
        </div>
      </details>

      <!-- 插件管理弹窗 -->
      <PluginManager :isOpen="showPluginManager" @close="showPluginManager = false" />



      <!-- 关于 & 数据 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Info :size="16" class="section-icon" />
          {{ $t('settings.about') }}
        </h3>
        <div class="about-info">
          <p>bob-agent v{{ appVersion }}</p>
          <p class="about-desc">{{ $t('settings.about_desc') }}</p>
        </div>

        <div style="margin-top: 16px; padding-top: 16px; border-top: 1px solid var(--border-subtle); display: flex; gap: 12px; flex-wrap: wrap;">
          <button class="btn btn-primary" @click="openDataDir">
            <FolderOpen :size="14" />
            {{ $t('settings.open_data_dir') }}
          </button>
          
          <button class="btn btn-primary" @click="openLogDir">
            <FileText :size="14" />
            {{ $te('settings.open_log_dir') ? $t('settings.open_log_dir') : '打开日志目录' }}
          </button>
          
          <button class="btn btn-danger" @click="factoryReset">
            <Trash2 :size="14" />
            {{ $t('settings.clear_all_data') }}
          </button>
        </div>
      </section>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue';
import { Settings as SettingsIcon, Monitor, Tractor, Eye, EyeOff, Plug, Loader2, Palette, Info, FolderOpen, FolderHeart, Puzzle, Layers, X, Plus, Unplug, Globe, HardDrive, Trash2, Key, FileText, Server, ChevronDown } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import CustomSelect from '../components/CustomSelect.vue';
import PluginManager from '../components/PluginManager.vue';
import ModelHub from '../components/ModelHub.vue';

const emit = defineEmits(['config-changed']);
const { locale, t } = useI18n();
const currentLocale = ref('zh-CN');

const languageOptions = [
  { label: '简体中文', value: 'zh-CN' },
  { label: 'English', value: 'en-US' },
];

const appVersion = ref('0.1.0');

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
  { label: t('settings.custom_provider'), value: 'custom' },
];

const accentColors = computed(() => [
  { name: t('settings.color_mallos_blue'), value: '#2776bb' },
  { name: t('settings.color_slate_gray'), value: '#627C8C' },
  { name: t('settings.color_lavender'), value: '#989398' },
  { name: t('settings.color_powder_blue'), value: '#B9C7D2' },
  { name: t('settings.color_vermilion'), value: '#E93C35' },
]);

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
  document.documentElement.classList.add('theme-transitioning');
  
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      document.documentElement.setAttribute('data-theme', theme);
      setTimeout(() => {
        document.documentElement.classList.remove('theme-transitioning');
      }, 850);
    });
  });

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
  wikiDir: '',
  workspaceDir: '',
  externalSkillsDir: '',
  accentColor: '',
  offlineModelPath: '',
});

const offlineEngineStatus = ref('stopped');

async function selectOfflineModel() {
  if (window.electronAPI.selectFile) {
    const path = await window.electronAPI.selectFile();
    if (path) {
      config.value.offlineModelPath = path;
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
    if (!config.value.offlineModelPath) return;
    offlineEngineStatus.value = 'starting';
    try {
      const res = await window.electronAPI.startOfflineEngine(config.value.offlineModelPath);
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

// ── 凭证管理 (Credential Store) ──
const modelProviders = ref([
  { id: 'deepseek', name: 'DeepSeek', hasKey: false },
  { id: 'openai', name: 'OpenAI', hasKey: false },
  { id: 'qwen', name: '通义千问 (Qwen)', hasKey: false },
  { id: 'doubao', name: '豆包 (Doubao)', hasKey: false },
  { id: 'zhipu', name: '智谱 AI (GLM)', hasKey: false },
  { id: 'kimi', name: 'Kimi (Moonshot)', hasKey: false },
  { id: 'minimax', name: 'MiniMax', hasKey: false },
]);
const toolProviders = ref([
  { id: 'TAVILY_API_KEY', name: 'Tavily (Web Search)', hasKey: false },
  { id: 'TINYFISH_API_KEY', name: 'TinyFish (Fetch)', hasKey: false },
]);
const apiKeys = ref({});
const toolStatuses = ref([]);

const customModels = ref([]);
const newCustomModel = ref({ name: '', url: '', key: '', id: '' });

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

function getProviderLogo(providerId) {
  const name = (providerId || '').toLowerCase();
  if (name.includes('deepseek')) return '/logos/deepseek.png';
  if (name.includes('openai')) return '/logos/openai.png';
  if (name.includes('qwen') || name.includes('dashscope')) return '/logos/qwen.png';
  if (name.includes('doubao')) return '/logos/doubao.png';
  if (name.includes('zhipu')) return '/logos/glm.svg';
  if (name.includes('kimi')) return '/logos/kimi.png';
  if (name.includes('minimax')) return '/logos/minimax.png';
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
    // 只显示有 requiredCredentials 的工具（其他都是无条件可用的）
    toolStatuses.value = statuses.filter(t => t.missingCredentials.length > 0 || t.name === 'web_search' || t.name === 'tinyfish_fetch');
  }
}

const modelHubRef = ref(null);

async function saveApiKey(providerId) {
  if (window.electronAPI.setApiKey) {
    const key = apiKeys.value[providerId];
    // 空字符串代表删除该 key
    await window.electronAPI.setApiKey(providerId, key);
    apiKeys.value[providerId] = ''; // clear input after save
    await fetchApiKeys(); // refresh key status
    await fetchToolStatuses(); // refresh tool activation states
    if (modelHubRef.value) {
      modelHubRef.value.refreshKeyStatus();
    }
    emit('config-changed'); // notify App to refresh
  }
}

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
    wikiDir: allConfig.wikiDir || '',
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
  await fetchApiKeys();
  await loadCustomModels();
  await fetchToolStatuses();
  if (window.electronAPI.getVersion) {
    appVersion.value = await window.electronAPI.getVersion();
  }
  if (window.electronAPI.getOfflineEngineStatus) {
    try {
      const res = await window.electronAPI.getOfflineEngineStatus();
      if (res && res.status) {
        offlineEngineStatus.value = res.status;
      }
    } catch(err) {}
  }
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
      if (isMain) testResult.value = { ok: true, message: t('settings.connection_ok') };
      else clerkTestResult.value = { ok: true, message: t('settings.connection_ok') };
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

function openLogDir() {
  if (window.electronAPI.openLogDir) {
    window.electronAPI.openLogDir();
  }
}

async function factoryReset() {
  if (confirm(t('modal.factory_reset_warning'))) {
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

async function selectWikiDir() {
  const dirPath = await window.electronAPI.selectDir();
  if (dirPath) {
    config.value.wikiDir = dirPath;
    await saveConfig('wikiDir', dirPath);
  }
}

async function clearWikiDir() {
  config.value.wikiDir = '';
  await saveConfig('wikiDir', '');
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

/* 统一间距和折叠样式 */
.details-section {
  border-top: 1px solid var(--border-subtle);
  padding-top: var(--space-4);
  margin-top: var(--space-4);
}
.details-section:first-of-type {
  border-top: none;
  padding-top: 0;
  margin-top: 0;
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
