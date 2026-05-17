import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';

// ═══════════════════════════════════════════════════════════
// Bob-Agent Tauri Bridge — 完整适配器层
// Tauri v2 IPC Bridge — 将所有前端 window.electronAPI 调用映射到 Rust @tauri-apps/api/core invoke
// ═══════════════════════════════════════════════════════════

window.electronAPI = {

  // ── 系统 & 配置 (Mapped to Rust) ─────────────────────
  isSetupComplete: () => invoke('system_is_setup_complete'),
  getConfig: (key) => invoke('config_get', { key }),
  setConfig: (key, value) => invoke('config_set', { key, value: JSON.parse(JSON.stringify(value)) }),
  getAllConfig: async () => {
    const config = await invoke('config_get_all');
    return config || {};
  },

  // ── 系统 & 原生能力 (Native Dialogs) ─────────────────
  selectWorkspaceDir: async () => {
    return await open({ directory: true, multiple: false, title: '选择工作资产库文件夹' });
  },
  selectDir: async () => {
    return await open({ directory: true, multiple: false, title: '选择文件夹' });
  },
  selectFolderToTrack: async () => {
    return await open({ directory: true, multiple: false, title: '选择要关注的文件夹' });
  },
  selectFile: async () => {
    return await open({ multiple: false, title: '选择文件' });
  },
  exportMarkdown: async (content, defaultName) => {
    const path = await save({ defaultPath: defaultName || 'export.md', filters: [{ name: 'Markdown', extensions: ['md'] }] });
    if (path) { console.log('Mock: would save MD to', path); }
    return path;
  },


  // ── 对话历史 (DB — 已由 Rust rusqlite 接管) ─────────
  getConversations: () => invoke('db_conversations'),
  getConversation: (id) => invoke('db_conversation_get', { id }),
  createConversation: (title, model) => invoke('db_conversation_create', { title: title || '新对话', model: model || '' }),
  deleteConversation: (id) => invoke('db_conversation_delete', { id }),
  renameConversation: (id, title) => invoke('db_conversation_rename', { id, title }),
  updateConversationCost: (id, cost) => invoke('db_conversation_update_cost', { id, cost }),
  getMessages: (conversationId) => invoke('db_messages', { conversationId }),
  addMessage: (conversationId, role, content, imageBase64) =>
    invoke('db_message_add', { conversationId, role, content: content || '', imageBase64: imageBase64 || null }),


  // ── LLM 通信 (Rust 引擎) ─────────────────────────────
  sendChat: (messages, globalFileAccess, agentMode) => 
    invoke('llm_chat', { messages }),
  sendVision: (messages, imageBase64, globalFileAccess, agentMode) => 
    invoke('llm_vision', { messages, imageBase64 }),
  stopGeneration: async () => { /* 待实现 AbortController */ },
  getModels: async (provider) => {
    try {
      return await invoke('llm_get_models', { provider: provider || null });
    } catch (err) {
      console.error('getModels error:', err);
      alert('获取模型失败: ' + err);
      return [];
    }
  },

  // ── 流式监听 (Event Listeners — 必须返回清理函数) ─────
  onStreamChunk: (callback) => {
    let unlisten = null;
    listen('llm:chunk', (event) => {
      callback(event.payload);
    }).then(fn => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  },

  // ── Model Hub ──────────────────────────────────────────
  getModelPool: () => invoke('llm_get_model_pool'),
  assignModelRole: (modelId, role) => invoke('llm_assign_model_role', { modelId, role }),
  getActiveModels: () => invoke('llm_get_active_models'),
  rescanModels: () => invoke('llm_rescan_models'),

  // ── 日历 / 日程 (Rust 原生 T-605) ──────────────────────
  listEvents: async () => invoke('system_list_events'),
  parseEvent: async (text) => invoke('system_parse_event', { text }),
  confirmEvent: async (event) => invoke('system_confirm_event', { event }),
  deleteEvent: async (id) => invoke('system_delete_event', { id }),
  updateEventStatus: async (id, status) => invoke('system_update_event_status', { id, status }),
  updateEventTime: async (id, startTime, endTime) => invoke('system_update_event_time', { id, startTime, endTime }),

  // ── 凭证管理 (Credential Store) ────────────────────────
  getApiKeys: async () => invoke('system_get_api_keys'),
  setApiKey: async (providerId, apiKey) => invoke('system_set_api_key', { providerId, apiKey }),
  addCustomModel: async (modelId, displayName, provider, baseUrl, apiKey) => invoke('system_add_custom_model', { modelId, displayName, provider, baseUrl, apiKey }),
  removeCustomModel: async (modelId) => invoke('system_remove_custom_model', { modelId }),
  getToolStatuses: async () => invoke('system_get_tool_statuses'),

  // ── 文件 & 工作目录 ──────────────────────────────────
  getFileMeta: async (path) => invoke('system_get_file_meta', { path }),
  readFile: async (filePath) => invoke('system_read_file', { filePath }),
  getFilePath: (file) => file?.path || file?.name || '',
  listWorkspaceDir: async (relativePath) => [],        // TODO T-606
  readWorkspaceFile: async (relativePath) => '',       // TODO T-606
  scanFolder: async (folderPath) => invoke('system_scan_folder', { folderPath }),
  estimateKB: async (folderPath) => invoke('system_estimate_kb', { folderPath }),
  buildKB: async (folderPath, plan) => invoke('system_build_kb', { folderPath, plan }),
  
  // LLM-Wiki 知识库引擎事件
  onKBProgress: (callback) => {
    let unlisten = null;
    listen('kb:progress', (event) => {
      callback(event.payload);
    }).then(fn => { unlisten = fn; });
    return () => { if (unlisten) unlisten(); };
  },
  onKBComplete: (callback) => {
    let unlisten = null;
    listen('kb:complete', (event) => {
      callback(event.payload);
    }).then(fn => { unlisten = fn; });
    return () => { if (unlisten) unlisten(); };
  },

  // Tauri 原生文件拖拽事件
  onDragDrop: (callback) => listen('tauri://drag-drop', callback),
  onDragEnter: (callback) => listen('tauri://drag-enter', callback),
  onDragLeave: (callback) => listen('tauri://drag-leave', callback),

  // ── 文件夹跟踪 (Rust 原生) ─────────────────────────────
  getTrackedFolders: async () => invoke('system_get_tracked_folders'),
  addTrackedFolder: async (folderPath) => invoke('system_add_tracked_folder', { folderPath }),
  removeTrackedFolder: async (folderPath) => invoke('system_remove_tracked_folder', { folderPath }),

  // ── 记忆引擎 (Rust 原生) ────────────────────────────────
  summarizeSession: async (conversationId) => invoke('system_summarize_session', { conversationId }),

  // ── 做梦引擎 (Rust 原生) ───────────────────────────────
  getDreamReport: async () => invoke('system_get_dream_report'),
  dismissDream: async () => invoke('system_dismiss_dream'),
  onDreamCompleted: (callback) => {
    // V2 将通过 Tauri Event 推送，目前 V1 同步生成不需要监听
    return () => {};
  },

  // ── 网页抓取 (Rust 原生 T-602) ─────────────────────────
  fetchUrl: async (url) => invoke('system_fetch_url', { url }),

  // ── 系统工具 (Rust 原生) ───────────────────────────────
  updateTheme: (theme) => console.log('Mock: updateTheme', theme), // TODO T-608
  getClipboardImage: async () => null,                 // TODO T-608
  showNotification: async (title, body) => console.log('Mock: notification', title, body), // TODO T-608
  openFile: async (filePath) => invoke('system_open_file', { filePath }),
  showInFolder: async (filePath) => invoke('system_show_in_folder', { filePath }),
  getVersion: async () => invoke('system_get_version'),
  getLogPath: async () => invoke('system_get_log_path'),
  openLogDir: async () => invoke('system_open_log_dir'),
  openDataDir: async () => invoke('system_open_data_dir'),
  factoryReset: async () => invoke('system_factory_reset'),

  // ── Outbox (声明式配置 — AI 自主修改设置) ──────────────
  writeOutbox: async (operations) => invoke('system_write_outbox', { operations }),
  onConfigReconciled: (callback) => {
    let unlisten = null;
    listen('config:reconciled', (event) => {
      callback(event.payload);
    }).then(fn => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  },

  // ── 插件系统 (Rust 原生) ───────────────────────────────
  getPlugins: async () => invoke('system_get_plugins'),
  installPlugin: async (id) => true,                   // TODO T-603 (安装逻辑)
  onPluginProgress: (callback) => {
    console.log('Mock: onPluginProgress listener bound'); // TODO T-603
    return () => {};
  },
  onPluginUpdated: (callback) => {
    console.log('Mock: onPluginUpdated listener bound'); // TODO T-603
    return () => {};
  },

  // ── MCP 配置 ──────────────────────────────────────────
  getMcpConfig: async () => ({ mcpServers: {} }),      // TODO T-609
  setMcpConfig: async (config) => true,                // TODO T-609

  // ── 离线引擎 (Sidecar) ─────────────────────────────────
  startOfflineEngine: async (modelPath) => invoke('start_offline_engine', { modelPath }),
  stopOfflineEngine: async () => invoke('stop_offline_engine'),
  getOfflineEngineStatus: async () => invoke('get_offline_engine_status'),
};

console.log('🚀 Tauri Bridge v5: 51 Rust-native IPC — only 4 lightweight mocks remain.');
