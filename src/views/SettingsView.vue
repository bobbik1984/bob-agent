<template>
  <div class="settings-view">
    <div class="settings-scroll">
      <div class="settings-content">
      <h2 class="settings-title">
        <SettingsIcon :size="22" class="title-icon" />
        设置
      </h2>

      <!-- AI 模型配置 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Cpu :size="16" class="section-icon" />
          AI 模型
        </h3>

        <div class="form-group">
          <label class="form-label">服务商</label>
          <CustomSelect
            v-model="config.provider"
            :options="providerOptions"
            @change="onProviderChange"
          />
        </div>

        <div class="form-group" v-if="config.provider !== 'ollama'">
          <label class="form-label">API Key</label>
          <input
            v-model="config.apiKey"
            :type="showApiKey ? 'text' : 'password'"
            class="input"
            placeholder="sk-..."
            @blur="saveConfig('apiKey', config.apiKey)"
          />
          <button class="btn-icon toggle-key" @click="showApiKey = !showApiKey">
            <EyeOff v-if="showApiKey" :size="16" />
            <Eye v-else :size="16" />
          </button>
        </div>

        <div class="form-group" v-if="config.provider === 'custom'">
          <label class="form-label">API 地址</label>
          <input
            v-model="config.baseURL"
            class="input"
            placeholder="https://your-api.com/v1"
            @blur="saveConfig('baseURL', config.baseURL)"
          />
        </div>

        <div class="form-group">
          <label class="form-label">默认模型</label>
          <CustomSelect
            v-model="config.model"
            :options="computedModelOptions"
            @change="saveConfig('model', config.model)"
          />
        </div>

        <!-- 连接测试 -->
        <div class="test-section">
          <button class="btn btn-ghost" @click="testConnection" :disabled="isTesting">
            <Loader2 v-if="isTesting" :size="14" class="animate-spin" />
            <Plug v-else :size="14" />
            <span>{{ isTesting ? '测试中...' : '测试连接' }}</span>
          </button>
          <span v-if="testResult" class="test-result" :class="testResult.ok ? 'success' : 'error'">
            {{ testResult.message }}
          </span>
        </div>
      </section>

      <!-- 外观 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Palette :size="16" class="section-icon" />
          外观
        </h3>
        <div class="form-group">
          <label class="form-label">主题</label>
          <CustomSelect
            v-model="config.theme"
            :options="themeOptions"
            @change="applyTheme(config.theme)"
          />
        </div>
        <div class="form-group">
          <label class="form-label">界面大小</label>
          <CustomSelect
            v-model="config.uiScale"
            :options="uiScaleOptions"
            @change="applyUiScale(config.uiScale)"
          />
        </div>
      </section>

      <!-- 工作目录 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <FolderOpen :size="16" class="section-icon" />
          工作目录
        </h3>
        <p class="section-desc">配置后，bob-agent 可以主动浏览和读取该目录下的文件</p>
        <div class="form-group workspace-group">
          <input
            v-model="config.workspaceDir"
            class="input"
            placeholder="点击右侧按钮选择目录..."
            readonly
          />
          <button class="btn btn-ghost browse-btn" @click="selectWorkspaceDir">
            <FolderOpen :size="14" />
            <span>浏览</span>
          </button>
        </div>
        <button
          v-if="config.workspaceDir"
          class="btn-clear"
          @click="clearWorkspaceDir"
        >
          清除工作目录
        </button>
      </section>

      <!-- 工具与扩展 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Puzzle :size="16" class="section-icon" />
          工具与扩展 (Skills)
        </h3>
        <p class="section-desc">配置外部技能所在的目录，Agent 会在启动时自动加载它们。</p>
        <div class="form-group workspace-group">
          <input
            v-model="config.externalSkillsDir"
            class="input"
            placeholder="点击右侧按钮选择外部技能目录..."
            readonly
          />
          <button class="btn btn-ghost browse-btn" @click="selectExternalSkillsDir">
            <FolderOpen :size="14" />
            <span>浏览</span>
          </button>
        </div>
        <button
          v-if="config.externalSkillsDir"
          class="btn-clear"
          @click="clearExternalSkillsDir"
        >
          清除技能目录
        </button>

        <div class="plugin-manager-entry" style="margin-top: 16px; padding-top: 16px; border-top: 1px solid var(--border-subtle);">
          <p class="section-desc" style="margin-bottom: 12px;">查看所有已注册技能并按需安装高级底层引擎（如本地文档解析）。</p>
          <button class="btn btn-secondary" @click="showPluginManager = true" style="display: flex; align-items: center; gap: 8px;">
            <Layers :size="16" />
            <span>打开技能与插件中心</span>
          </button>
        </div>
      </section>

      <!-- 插件管理弹窗 -->
      <PluginManager :isOpen="showPluginManager" @close="showPluginManager = false" />

      <!-- MCP Servers -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Unplug :size="16" class="section-icon" />
          MCP Servers
        </h3>
        <p class="section-desc">通过 Model Context Protocol 连接外部工具服务器，让 Bob 获得更多能力。</p>

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
          <span>尚未配置 MCP Server</span>
        </div>

        <!-- 添加 MCP Server -->
        <div v-if="showAddMcp" class="mcp-add-form">
          <div class="form-group">
            <label class="form-label">名称</label>
            <input v-model="newMcp.name" class="input" placeholder="例如 filesystem" />
          </div>
          <div class="form-group">
            <label class="form-label">命令</label>
            <input v-model="newMcp.command" class="input" placeholder="npx" />
          </div>
          <div class="form-group">
            <label class="form-label">参数 (空格分隔)</label>
            <input v-model="newMcp.args" class="input" placeholder="-y @modelcontextprotocol/server-filesystem /path" />
          </div>
          <div style="display: flex; gap: 8px; margin-top: 8px;">
            <button class="btn btn-primary" @click="addMcpServer" :disabled="!newMcp.name || !newMcp.command">保存</button>
            <button class="btn btn-ghost" @click="showAddMcp = false">取消</button>
          </div>
        </div>
        <button v-else class="btn btn-ghost" @click="showAddMcp = true" style="margin-top: 12px;">
          <Plus :size="14" />
          <span>添加 MCP Server</span>
        </button>
      </section>

      <!-- 关注的文件夹 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <FolderHeart :size="16" class="section-icon" />
          关注的文件夹
        </h3>
        <p class="section-desc">Bob 会记住这些文件夹的内容，以便你随时询问。你也可以直接把文件夹拖入聊天窗口来添加。</p>

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
          <span>还没有关注的文件夹</span>
        </div>

        <button class="btn btn-ghost" @click="addFolder" style="margin-top: 12px;">
          <Plus :size="14" />
          <span>添加本地目录</span>
        </button>
      </section>

      <!-- 关于 -->
      <section class="settings-section card">
        <h3 class="section-title">
          <Info :size="16" class="section-icon" />
          关于
        </h3>
        <div class="about-info">
          <p>bob-agent v0.1.0</p>
          <p class="about-desc">AI 桌面私人秘书 — 智能对话 + 图片识别 + 日程管理 + 文件分析</p>
        </div>
      </section>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, defineEmits } from 'vue';
import { Settings as SettingsIcon, Cpu, Eye, EyeOff, Plug, Loader2, Palette, Info, FolderOpen, FolderHeart, Puzzle, Layers, X, Plus, Unplug } from 'lucide-vue-next';
import CustomSelect from '../components/CustomSelect.vue';
import PluginManager from '../components/PluginManager.vue';

const emit = defineEmits(['config-changed']);

const providerOptions = [
  { label: 'DeepSeek', value: 'deepseek' },
  { label: 'OpenAI', value: 'openai' },
  { label: 'Ollama (本地)', value: 'ollama' },
  { label: '自定义', value: 'custom' },
];

const themeOptions = [
  { label: '暗色', value: 'dark' },
  { label: '亮色', value: 'light' },
];

const uiScaleOptions = [
  { label: '紧凑', value: 'compact' },
  { label: '舒适', value: 'comfortable' },
];

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
}

const config = ref({
  provider: 'deepseek',
  apiKey: '',
  model: '',
  baseURL: '',
  theme: 'dark',
  uiScale: 'compact',
  workspaceDir: '',
  externalSkillsDir: '',
});

const availableModels = ref([]);
const computedModelOptions = computed(() => {
  return availableModels.value.map(m => ({
    label: `${m.label} (${m.id})`,
    value: m.id
  }));
});

const showApiKey = ref(false);
const isTesting = ref(false);
const testResult = ref(null);
const showPluginManager = ref(false);
const trackedFolders = ref([]);

onMounted(async () => {
  const allConfig = await window.electronAPI.getAllConfig();
  config.value = {
    provider: allConfig.provider || 'deepseek',
    apiKey: allConfig.apiKey || '',
    model: allConfig.model || '',
    baseURL: allConfig.baseURL || '',
    theme: allConfig.theme || 'dark',
    uiScale: allConfig.uiScale || 'compact',
    workspaceDir: allConfig.workspaceDir || '',
    externalSkillsDir: allConfig.externalSkillsDir || '',
  };
  applyUiScale(config.value.uiScale, false);
  await loadModels();
  await loadTrackedFolders();
  await loadMcpConfig();
});

async function loadModels() {
  availableModels.value = await window.electronAPI.getModels();
}

async function saveConfig(key, value) {
  await window.electronAPI.setConfig(key, value);
  emit('config-changed');
}

async function onProviderChange() {
  await saveConfig('provider', config.value.provider);
  await loadModels();
  // 自动选择默认模型
  const defaultModel = availableModels.value.find(m => m.default);
  if (defaultModel) {
    config.value.model = defaultModel.id;
    await saveConfig('model', defaultModel.id);
  }
}

async function testConnection() {
  isTesting.value = true;
  testResult.value = null;

  try {
    const result = await window.electronAPI.sendChat([
      { role: 'user', content: '你好，请回复"连接成功"' }
    ]);

    if (result.error) {
      testResult.value = { ok: false, message: result.error };
    } else {
      testResult.value = { ok: true, message: '连接成功' };
    }
  } catch (err) {
    testResult.value = { ok: false, message: err.message };
  } finally {
    isTesting.value = false;
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
  top: 28px;
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
  color: var(--color-success);
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
</style>
