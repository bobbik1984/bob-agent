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
  searchMessages: (query) => invoke('db_search_messages', { query }),


  // ── LLM 通信 (Rust 引擎) ─────────────────────────────
  sendChat: (messages, globalFileAccess, agentMode, conversationId) => 
    invoke('llm_chat', { messages, conversationId }),
  sendVision: (messages, imageBase64, globalFileAccess, agentMode, conversationId) => 
    invoke('llm_vision', { messages, imageBase64, conversationId }),
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

  // Phase 2: 微信 Bridge 完成回复后广播的通知事件
  // 载荷: { conversation_id: string, from_user: string }
  onRemoteNewMessage: (callback) => {
    return listen('remote:new-message', (event) => {
      callback(event);
    });
  },

  // ── Model Hub ──────────────────────────────────────────
  getModelPool: () => invoke('llm_get_model_pool'),
  assignModelRole: (modelId, role) => invoke('llm_assign_model_role', { modelId, role }),
  getActiveModels: () => invoke('llm_get_active_models'),
  rescanModels: () => invoke('llm_rescan_models'),
  refreshModels: (providerId) => invoke('llm_refresh_models', { providerId }),
  getRegistry: () => invoke('llm_get_registry'),
  saveRegistry: (registry) => invoke('llm_save_registry', { registry }),

  // ── 日历 / 日程 (Rust 原生 T-605) ──────────────────────
  listEvents: async () => invoke('system_list_events'),
  parseEvent: async (text) => invoke('system_parse_event', { text }),
  confirmEvent: async (event) => invoke('system_confirm_event', { event }),
  deleteEvent: async (id) => invoke('system_delete_event', { id }),
  updateEventStatus: async (id, status) => invoke('system_update_event_status', { id, status }),
  updateEventTime: async (id, startTime, endTime) => invoke('system_update_event_time', { id, startTime, endTime }),

  // ── Cron 定时任务 (Rust 原生 T-1211) ───────────────
  listCronJobs: async () => invoke('system_list_cron_jobs'),
  addCronJob: async (title, cronExpr, prompt) => invoke('system_add_cron_job', { title, cronExpr, prompt }),
  removeCronJob: async (id) => invoke('system_remove_cron_job', { id }),
  toggleCronJob: async (id, enabled) => invoke('system_toggle_cron_job', { id, enabled }),
  onSchedulerCompleted: (callback) => {
    let unlisten = null;
    listen('scheduler:completed', (event) => {
      callback(event.payload);
    }).then(fn => { unlisten = fn; });
    return () => { if (unlisten) { unlisten(); unlisten = null; } };
  },
  // T-1307: 待办提醒事件
  onTodoReminder: (callback) => {
    let unlisten = null;
    listen('todo:reminder', (event) => {
      callback(event.payload);
    }).then(fn => { unlisten = fn; });
    return () => { if (unlisten) { unlisten(); unlisten = null; } };
  },

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
  migrateWikiDir: async (oldDir, newDir, mode) => invoke('system_migrate_wiki_dir', { oldDir, newDir, mode }),
  
  // LLM-Wiki 知识库引擎事件
  onKBProgress: (callback) => {
    let unlisten = null;
    listen('kb:progress', (event) => {
      callback(event.payload);
    }).then(fn => { unlisten = fn; });
    return () => { if (unlisten) { unlisten(); unlisten = null; } };
  },
  onKBComplete: (callback) => {
    let unlisten = null;
    listen('kb:complete', (event) => {
      callback(event.payload);
    }).then(fn => { unlisten = fn; });
    return () => { if (unlisten) { unlisten(); unlisten = null; } };
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
  getMemoryEntries: async () => invoke('system_get_memory_entries'),
  deleteMemoryEntry: async (entryType, entryId) => invoke('system_delete_memory_entry', { entryType, entryId }),

  // ── 做梦引擎 (Rust 原生) ───────────────────────────────
  getDreamReport: async () => invoke('system_get_dream_report'),
  dismissDream: async () => invoke('system_dismiss_dream'),
  onDreamCompleted: (callback) => {
    let unlisten = null;
    listen('dream:completed', (event) => {
      callback(event.payload);
    }).then(fn => { unlisten = fn; });
    return () => { if (unlisten) { unlisten(); unlisten = null; } };
  },

  // ── 进化引擎 (Rust 原生) ───────────────────────────────
  getEvolutionStats: async () => invoke('system_get_evolution_stats'),

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

  // ── 闪念速记 (Quick Note) ──────────────────────────────
  appendQuickNote: async (content) => invoke('system_append_quick_note', { content }),

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
  getMcpConfig: async () => invoke('mcp_get_config'),
  setMcpConfig: async (config) => invoke('mcp_set_config', { config }),

  // ── 连接器 (Connectors) ────────────────────────────────
  connectorList: async () => invoke('connector_list'),
  connectorStartOAuth: async (name) => invoke('connector_start_oauth', { name }),
  connectorSaveCredentials: async (name, credentials) => invoke('connector_save_credentials', { name, credentials }),
  connectorDisconnect: async (name) => invoke('connector_disconnect', { name }),

  // ── 离线引擎 (Sidecar) ─────────────────────────────────
  startOfflineEngine: async (modelPath) => invoke('start_offline_engine', { modelPath }),
  stopOfflineEngine: async () => invoke('stop_offline_engine'),
  getOfflineEngineStatus: async () => invoke('get_offline_engine_status'),
  openLlamaEngineDir: () => invoke('system_open_llm_engine_dir'),

  // ── 微信助理 (Rust 原生 WeChat Gateway) ─────────────────
  wechatGetLoginQr: async () => invoke('wechat_get_login_qr'),
  wechatCheckLoginStatus: async (qrcode) => invoke('wechat_check_login_status', { qrcode }),
  wechatGetCurrentStatus: async () => invoke('wechat_get_current_status'),

  // ── 浏览器增强 (CDP Browser Enhancement) ─────────────────
  browserDetect: async () => invoke('system_browser_detect'),
  browserEnable: async () => invoke('system_browser_enable'),

  // ── GCP Vertex AI 凭证管理 ─────────────────────────────
  uploadGcpCredential: async (sourcePath) => invoke('system_upload_gcp_credential', { sourcePath }),
  testGcpCredential: async () => invoke('system_test_gcp_credential'),
  removeGcpCredential: async () => invoke('system_remove_gcp_credential'),
  getGcpCredentialStatus: async () => invoke('system_get_gcp_credential_status'),

  // ── Doctor 自检引擎 (T-1304) ────────────────────────────
  healthCheck: async () => invoke('system_health_check'),
  autoFix: async (code) => invoke('system_auto_fix', { code }),

  // ── 聊天就绪校验 (T-1305) ──────────────────────────────
  validateChatReady: async () => invoke('system_validate_chat_ready'),

  // Generic invoke passthrough for components that call invoke directly
  invoke: (cmd, args) => invoke(cmd, args || {}),
};

console.log('Tauri Bridge v5.7: 69 Rust-native IPC — T-1306/1307/1308 P2.');
