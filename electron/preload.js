/**
 * bob-agent — Preload 安全桥接
 *
 * 通过 contextBridge 将安全的 IPC 通道暴露给 Renderer 进程。
 * Renderer 进程绝不允许直接访问 Node.js API。
 */

const { contextBridge, ipcRenderer } = require('electron');

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
  getModels: () => ipcRenderer.invoke('llm:models'),

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

  // ── 工作目录 ─────────────────────────────────────────
  listWorkspaceDir: (relativePath) => ipcRenderer.invoke('workspace:list-dir', relativePath),
  readWorkspaceFile: (relativePath) => ipcRenderer.invoke('workspace:read-file', relativePath),
  selectWorkspaceDir: () => ipcRenderer.invoke('workspace:select-dir'),

  // ── 对话历史 ───────────────────────────────────────
  getConversations: () => ipcRenderer.invoke('db:conversations'),
  createConversation: (title, model) => ipcRenderer.invoke('db:conversation:create', title, model),
  deleteConversation: (id) => ipcRenderer.invoke('db:conversation:delete', id),
  renameConversation: (id, title) => ipcRenderer.invoke('db:conversation:rename', id, title),
  getMessages: (conversationId) => ipcRenderer.invoke('db:messages', conversationId),
  addMessage: (conversationId, role, content, imageBase64) =>
    ipcRenderer.invoke('db:message:add', conversationId, role, content, imageBase64),

  // ── 配置 ───────────────────────────────────────────
  getConfig: (key) => ipcRenderer.invoke('config:get', key),
  setConfig: (key, value) => ipcRenderer.invoke('config:set', key, value),
  getAllConfig: () => ipcRenderer.invoke('config:all'),

  // ── 系统 ───────────────────────────────────────────
  getClipboardImage: () => ipcRenderer.invoke('system:clipboard-image'),
  showNotification: (title, body) => ipcRenderer.invoke('system:notify', title, body),
  isSetupComplete: () => ipcRenderer.invoke('system:is-setup-complete'),
  selectDir: () => ipcRenderer.invoke('system:select-dir'),
  updateTheme: (theme) => ipcRenderer.invoke('system:update-theme', theme),

  // ── Plugins ────────────────────────────────────────
  getPlugins: () => ipcRenderer.invoke('plugin:get-list'),
  installPlugin: (id) => ipcRenderer.invoke('plugin:install', id),
  onPluginProgress: (callback) => {
    const handler = (_event, data) => callback(data);
    ipcRenderer.on('plugin:progress', handler);
    return () => ipcRenderer.removeListener('plugin:progress', handler);
  },
});
