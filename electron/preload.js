/**
 * bob-agent — Preload 安全桥接
 *
 * 通过 contextBridge 将安全的 IPC 通道暴露给 Renderer 进程。
 * Renderer 进程绝不允许直接访问 Node.js API。
 */

const { contextBridge, ipcRenderer, webUtils } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  // ── LLM ────────────────────────────────────────────
  sendChat: async (messages, globalFileAccess, agentMode) => {
    await ipcRenderer.invoke('security:toggle-global-access', globalFileAccess);
    await ipcRenderer.invoke('security:set-agent-mode', agentMode);
    return ipcRenderer.invoke('llm:chat', messages);
  },
  sendVision: async (messages, imageBase64, globalFileAccess, agentMode) => {
    await ipcRenderer.invoke('security:toggle-global-access', globalFileAccess);
    await ipcRenderer.invoke('security:set-agent-mode', agentMode);
    return ipcRenderer.invoke('llm:vision', messages, imageBase64);
  },
  stopGeneration: () => ipcRenderer.invoke('llm:stop'),
  getModels: (provider) => ipcRenderer.invoke('llm:models', provider),

  // 流式 chunk 监听（主进程 → 渲染进程）
  onStreamChunk: (callback) => {
    const handler = (_event, chunk) => callback(chunk);
    ipcRenderer.on('llm:chunk', handler);
    // 返回清理函数
    return () => ipcRenderer.removeListener('llm:chunk', handler);
  },

  // ── 日历 / Parser ──────────────────────────────────
  parseEvent: (text) => ipcRenderer.invoke('calendar:parse', text),
  confirmEvent: (event) => ipcRenderer.invoke('calendar:confirm', event),
  listEvents: (start, end) => ipcRenderer.invoke('calendar:list', start, end),
  deleteEvent: (id) => ipcRenderer.invoke('calendar:delete', id),
  updateEventStatus: (id, status) => ipcRenderer.invoke('calendar:update-status', id, status),
  updateEventTime: (id, startTime, endTime) => ipcRenderer.invoke('calendar:update-time', id, startTime, endTime),

  // ── 文件 ───────────────────────────────────────────
  readFile: (filePath) => ipcRenderer.invoke('file:read', filePath),
  selectFile: () => ipcRenderer.invoke('file:select'),
  getFilePath: (file) => {
    try {
      return webUtils ? webUtils.getPathForFile(file) : file.path;
    } catch (e) {
      return file.path;
    }
  },

  // ── 工作目录 ─────────────────────────────────────────
  listWorkspaceDir: (relativePath) => ipcRenderer.invoke('workspace:list-dir', relativePath),
  readWorkspaceFile: (relativePath) => ipcRenderer.invoke('workspace:read-file', relativePath),
  selectWorkspaceDir: () => ipcRenderer.invoke('workspace:select-dir'),

  // ── 文件夹跟踪 ───────────────────────────────────────
  scanFolder: (folderPath) => ipcRenderer.invoke('folders:scan', folderPath),
  estimateKB: (folderPath) => ipcRenderer.invoke('kb:estimate', folderPath),

  // ── 对话历史 ───────────────────────────────────────
  getConversations: () => ipcRenderer.invoke('db:conversations'),
  createConversation: (title, model) => ipcRenderer.invoke('db:conversation:create', title, model),
  deleteConversation: (id) => ipcRenderer.invoke('db:conversation:delete', id),
  renameConversation: (id, title) => ipcRenderer.invoke('db:conversation:rename', id, title),
  getMessages: (conversationId) => ipcRenderer.invoke('db:messages', conversationId),
  addMessage: (conversationId, role, content, imageBase64) =>
    ipcRenderer.invoke('db:message:add', conversationId, role, content, imageBase64),

  // ── 记忆引擎 ───────────────────────────────────────
  summarizeSession: (conversationId) => ipcRenderer.invoke('memory:summarize-session', conversationId),

  // ── 文件夹跟踪 ─────────────────────────────────────
  getTrackedFolders: () => ipcRenderer.invoke('folders:list'),
  addTrackedFolder: (folderPath) => ipcRenderer.invoke('folders:add', folderPath),
  removeTrackedFolder: (folderPath) => ipcRenderer.invoke('folders:remove', folderPath),
  selectFolderToTrack: () => ipcRenderer.invoke('folders:select-dir'),

  // ── 配置 ───────────────────────────────────────────
  getConfig: (key) => ipcRenderer.invoke('config:get', key),
  setConfig: (key, value) => ipcRenderer.invoke('config:set', key, value),
  openDataDir: () => ipcRenderer.invoke('app:open-data-dir'),
  factoryReset: () => ipcRenderer.invoke('app:factory-reset'),
  getAllConfig: () => ipcRenderer.invoke('config:all'),

  // ── 系统 ───────────────────────────────────────────
  getClipboardImage: () => ipcRenderer.invoke('system:clipboard-image'),
  showNotification: (title, body) => ipcRenderer.invoke('system:notify', title, body),
  isSetupComplete: () => ipcRenderer.invoke('system:is-setup-complete'),
  selectDir: () => ipcRenderer.invoke('system:select-dir'),
  updateTheme: (theme) => ipcRenderer.invoke('system:update-theme', theme),
  exportMarkdown: (content, defaultName) => ipcRenderer.invoke('system:export-md', content, defaultName),
  openFile: (filePath) => ipcRenderer.invoke('system:open-file', filePath),
  showInFolder: (filePath) => ipcRenderer.invoke('system:show-in-folder', filePath),
  getFileMeta: (filePath) => ipcRenderer.invoke('system:file-meta', filePath),

  // ── Plugins ────────────────────────────────────────
  getPlugins: () => ipcRenderer.invoke('plugin:get-list'),
  installPlugin: (id) => ipcRenderer.invoke('plugin:install', id),
  onPluginProgress: (callback) => {
    const handler = (_event, data) => callback(data);
    ipcRenderer.on('plugin:progress', handler);
    return () => ipcRenderer.removeListener('plugin:progress', handler);
  },
  onPluginUpdated: (callback) => {
    const handler = (_event, data) => callback(data);
    ipcRenderer.on('plugin:updated', handler);
    return () => ipcRenderer.removeListener('plugin:updated', handler);
  },

  // ── MCP ─────────────────────────────────────────────
  getMcpConfig: () => ipcRenderer.invoke('mcp:config:get'),
  setMcpConfig: (config) => ipcRenderer.invoke('mcp:config:set', config),
});
