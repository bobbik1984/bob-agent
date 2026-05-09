/**
 * bob-agent — Preload 安全桥接
 *
 * 通过 contextBridge 将安全的 IPC 通道暴露给 Renderer 进程。
 * Renderer 进程绝不允许直接访问 Node.js API。
 */

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  // ── LLM ────────────────────────────────────────────
  sendChat: (messages) => ipcRenderer.invoke('llm:chat', messages),
  sendVision: (messages, imageBase64) => ipcRenderer.invoke('llm:vision', messages, imageBase64),
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

  // ── 文件 ───────────────────────────────────────────
  readFile: (filePath) => ipcRenderer.invoke('file:read', filePath),
  selectFile: () => ipcRenderer.invoke('file:select'),

  // ── 对话历史 ───────────────────────────────────────
  getConversations: () => ipcRenderer.invoke('db:conversations'),
  createConversation: (title, model) => ipcRenderer.invoke('db:conversation:create', title, model),
  deleteConversation: (id) => ipcRenderer.invoke('db:conversation:delete', id),
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
});
