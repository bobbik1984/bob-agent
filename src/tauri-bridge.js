// ═══════════════════════════════════════════════════════════
// Bob-Agent Tauri Bridge — 完整适配器层
// Tauri v2 IPC Bridge — 将所有前端 window.appAPI 调用映射到 Rust @tauri-apps/api/core invoke
//
// 浏览器降级：当 window.__TAURI_INTERNALS__ 不存在时，
// 提供完整 Mock API 让 UI 能在纯浏览器 (npm run dev) 下渲染。
// ═══════════════════════════════════════════════════════════

const IS_TAURI = typeof window !== 'undefined' && !!window.__TAURI_INTERNALS__;

// ── Tauri 或 Mock 基础函数 ──────────────────────────────
let invoke, open, save, listen, getCurrentWindow;

if (IS_TAURI) {
  // 真实 Tauri 环境 — 动态 import 确保浏览器不会加载这些模块
  const core = await import('@tauri-apps/api/core');
  const dialog = await import('@tauri-apps/plugin-dialog');
  const event = await import('@tauri-apps/api/event');
  const win = await import('@tauri-apps/api/window');
  invoke = core.invoke;
  open = dialog.open;
  save = dialog.save;
  listen = event.listen;
  getCurrentWindow = win.getCurrentWindow;
} else {
  // ── 浏览器 Mock 环境 ──────────────────────────────────
  console.log('%c[Bridge] Running in BROWSER mock mode', 'color: #f59e0b; font-weight: bold;');

  // Mock 数据
  const MOCK_CONVERSATIONS = [
    {
      id: 'mock-conv-1',
      title: '测试对话：欢迎使用 Bob',
      model: 'mock-model',
      updated_at: new Date().toISOString(),
      last_message: '你好，Bob！',
      last_role: 'user',
      total_cost: 0.0012,
    },
    {
      id: 'mock-conv-2',
      title: '项目讨论',
      model: 'mock-model',
      updated_at: new Date(Date.now() - 86400000).toISOString(),
      last_message: '好的，我来帮你分析一下这个方案。',
      last_role: 'assistant',
      total_cost: 0.0058,
    },
  ];

  const MOCK_MESSAGES = {
    'mock-conv-1': [
      { id: 'm1', role: 'user', content: '你好，Bob！', created_at: new Date(Date.now() - 60000).toISOString() },
      { id: 'm2', role: 'assistant', content: '嗨！我是 Bob，你的 AI 桌面助手。有什么可以帮你的吗？\n\n我可以帮你：\n- 💬 智能对话\n- 📅 管理日程\n- 📝 记录笔记\n- 🧠 构建知识图谱', created_at: new Date(Date.now() - 30000).toISOString() },
    ],
    'mock-conv-2': [
      { id: 'm3', role: 'user', content: '帮我分析一下当前的项目架构', created_at: new Date(Date.now() - 172800000).toISOString() },
      { id: 'm4', role: 'assistant', content: '好的，我来帮你分析一下这个方案。', created_at: new Date(Date.now() - 172700000).toISOString() },
    ],
  };

  const MOCK_CONFIG = {
    theme: 'dark',
    accentColor: '#2776BB',
    uiScale: '100',
    locale: 'zh-CN',
    offlineModelPath: '',
    weatherCity: '',
  };

  const MOCK_MODEL_POOL = {
    providers: [
      {
        id: 'deepseek', name: 'DeepSeek', configured: true,
        models: [
          { id: 'deepseek-chat', displayName: 'DeepSeek Chat', role: 'primary' },
          { id: 'deepseek-reasoner', displayName: 'DeepSeek Reasoner', role: null },
        ],
      },
      {
        id: 'openai', name: 'OpenAI', configured: false,
        models: [
          { id: 'gpt-4o', displayName: 'GPT-4o', role: null },
        ],
      },
    ],
  };

  // Mock invoke — 根据命令返回合理的假数据
  invoke = async (cmd, args) => {
    // console.log(`[Mock invoke] ${cmd}`, args);
    switch (cmd) {
      // 系统
      case 'system_is_setup_complete': return true;
      case 'system_get_version': return '0.5.1-dev (browser)';
      case 'system_get_log_path': return '/mock/logs/bob.log';
      case 'system_health_check': return { ok: true, checks: [] };
      case 'system_validate_chat_ready': return { ready: true };
      case 'system_get_evolution_stats': return { total_sessions: 5, total_tools: 12 };

      // 配置
      case 'config_get': return MOCK_CONFIG[args?.key] ?? null;
      case 'config_get_all': return { ...MOCK_CONFIG };
      case 'config_set': { MOCK_CONFIG[args?.key] = args?.value; return true; }

      // 对话
      case 'db_conversations': return [...MOCK_CONVERSATIONS];
      case 'db_conversation_get': return MOCK_CONVERSATIONS.find(c => c.id === args?.id) || null;
      case 'db_conversation_create': {
        const newConv = {
          id: 'mock-conv-' + Date.now(),
          title: args?.title || '新对话',
          model: args?.model || '',
          updated_at: new Date().toISOString(),
          last_message: '', last_role: '',
          total_cost: 0,
        };
        MOCK_CONVERSATIONS.unshift(newConv);
        MOCK_MESSAGES[newConv.id] = [];
        return newConv.id;
      }
      case 'db_conversation_delete': {
        const idx = MOCK_CONVERSATIONS.findIndex(c => c.id === args?.id);
        if (idx >= 0) MOCK_CONVERSATIONS.splice(idx, 1);
        return true;
      }
      case 'db_conversation_rename': {
        const conv = MOCK_CONVERSATIONS.find(c => c.id === args?.id);
        if (conv) conv.title = args?.title;
        return true;
      }
      case 'system_auto_rename_conversation': return true;
      case 'db_conversation_update_cost': return true;
      case 'db_messages': return MOCK_MESSAGES[args?.conversationId] || [];
      case 'db_message_add': {
        const msgs = MOCK_MESSAGES[args?.conversationId];
        if (msgs) {
          msgs.push({
            id: 'msg-' + Date.now(),
            role: args?.role || 'user',
            content: args?.content || '',
            created_at: new Date().toISOString(),
          });
        }
        return true;
      }
      case 'db_search_messages': return [];

      // LLM
      case 'llm_chat': case 'llm_vision': return 'mock-request-id';
      case 'llm_get_models': return [];
      case 'llm_get_model_pool': return MOCK_MODEL_POOL;
      case 'llm_assign_model_role': return true;
      case 'llm_get_active_models': return { primary: 'deepseek-chat', clerk: null, vision: null };
      case 'llm_rescan_models': return true;
      case 'llm_refresh_models': return true;
      case 'llm_get_registry': return { providers: {} };
      case 'llm_save_registry': return true;

      // 日历
      case 'system_list_events': return [];
      case 'system_parse_event': return null;
      case 'system_confirm_event': return true;
      case 'system_delete_event': return true;
      case 'system_update_event_status': return true;
      case 'system_update_event_time': return true;

      // Cron
      case 'system_list_cron_jobs': return [];
      case 'system_add_cron_job': return 'mock-cron-id';
      case 'system_remove_cron_job': return true;
      case 'system_toggle_cron_job': return true;

      // 凭证
      case 'system_get_api_keys': return {};
      case 'system_set_api_key': return true;
      case 'system_add_custom_model': return true;
      case 'system_remove_custom_model': return true;
      case 'system_get_tool_statuses': return [];

      // 文件
      case 'system_get_file_meta': return { name: 'mock.txt', size: 1024, isDir: false, isDirectory: false };
      case 'system_read_file': return '(mock file content)';
      case 'system_scan_folder': return { error: false, message: 'mock scan', files: [] };
      case 'system_estimate_kb': return { totalFiles: 0, totalSize: 0 };
      case 'system_build_kb': return true;
      case 'system_check_project_index': return null;

      // 文件夹跟踪
      case 'system_get_tracked_folders': return [];
      case 'system_add_tracked_folder': return true;
      case 'system_remove_tracked_folder': return true;

      // 记忆
      case 'system_summarize_session': return true;
      case 'system_get_memory_entries': return [];
      case 'system_delete_memory_entry': return true;

      // 做梦
      case 'system_get_dream_report': return null;
      case 'system_dismiss_dream': return true;
      case 'system_get_tag_proposals': return [];
      case 'system_clear_tag_proposals': return true;

      // 网页
      case 'system_fetch_url': return '(mock fetched content)';

      // 系统工具
      case 'system_open_file': case 'system_show_in_folder':
      case 'system_open_log_dir': case 'system_open_data_dir':
      case 'system_open_llm_engine_dir': case 'system_factory_reset':
      case 'system_take_screenshot':
        console.log(`[Mock] ${cmd} — desktop only, ignored`); return null;

      case 'start_web_drop': return 'https://mock.webdrop.link';

      // Outbox
      case 'system_write_outbox': return true;

      // 插件
      case 'system_get_plugins': return [];

      // MCP
      case 'mcp_get_config': return { servers: [] };
      case 'mcp_set_config': return true;

      // 连接器
      case 'connector_list': return [];
      case 'connector_start_oauth': return null;
      case 'connector_save_credentials': return true;
      case 'connector_disconnect': return true;

      // 离线引擎
      case 'start_offline_engine': case 'stop_offline_engine':
        return true;
      case 'get_offline_engine_status': return { running: false };

      // 微信/TG/Discord
      case 'wechat_get_login_qr': return null;
      case 'wechat_check_login_status': return { status: 'disconnected' };
      case 'wechat_get_current_status': return { connected: false };
      case 'system_save_telegram_token': case 'system_save_discord_token': return true;
      case 'system_get_telegram_token': case 'system_get_discord_token': return '';

      // 浏览器增强
      case 'system_browser_detect': return { found: false };
      case 'system_browser_enable': return false;

      // GCP
      case 'system_upload_gcp_credential': case 'system_test_gcp_credential':
      case 'system_remove_gcp_credential': return true;
      case 'system_get_gcp_credential_status': return { configured: false };

      // Doctor
      case 'system_auto_fix': return { success: true };

      // 知识图谱
      case 'kg_get_full_graph': return { nodes: [], edges: [] };
      case 'kg_query': return { nodes: [], edges: [] };
      case 'kg_stats': return { node_count: 0, edge_count: 0, type_distribution: {} };
      case 'kg_delete_node_cmd': return true;
      case 'kg_backfill': return true;
      case 'system_remove_source': return true;

      // 笔记
      case 'notebook_list_notes': return [];
      case 'notebook_read_note': return '';
      case 'notebook_save_note': case 'notebook_create_note':
      case 'notebook_delete_note': case 'notebook_move_note':
      case 'notebook_rename_note': case 'notebook_append_daily':
      case 'notebook_save_asset': case 'notebook_create_folder':
      case 'notebook_update_tags': case 'notebook_merge_tags':
      case 'notebook_reject_tag_merge': return true;
      case 'notebook_search': return [];
      case 'notebook_list_all_tags': return [];
      case 'notebook_get_backlinks': return [];

      default:
        console.warn(`[Mock invoke] unhandled command: ${cmd}`, args);
        return null;
    }
  };

  // Mock 其他 Tauri API
  open = async () => null;
  save = async () => null;
  listen = async (event, handler) => {
    // console.log(`[Mock listen] ${event}`);
    return () => {}; // 返回空清理函数
  };
  getCurrentWindow = () => ({
    minimize: async () => {},
    toggleMaximize: async () => {},
    hide: async () => {},
    show: async () => {},
    setFocus: async () => {},
    unminimize: async () => {},
  });
}

// ═══════════════════════════════════════════════════════════
// window.appAPI — 统一接口层（Tauri 和浏览器共用）
// ═══════════════════════════════════════════════════════════

window.appAPI = {
  // ── 窗口与事件控制 ──────────────────────────────────
  minimizeWindow: () => getCurrentWindow().minimize(),
  toggleMaximize: () => getCurrentWindow().toggleMaximize(),
  hideWindow: () => getCurrentWindow().hide(),
  showWindow: () => getCurrentWindow().show(),
  focusWindow: () => getCurrentWindow().setFocus(),
  unminimizeWindow: () => getCurrentWindow().unminimize(),
  listenEvent: (event, handler) => listen(event, handler),

  // ── 系统 & 配置 (Mapped to Rust) ─────────────────────
  openExternal: (url) => IS_TAURI ? invoke('plugin:shell|open', { path: url }) : window.open(url, '_blank'),
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
  autoRenameConversation: (id) => invoke('system_auto_rename_conversation', { conversationId: id }),
  updateConversationCost: (id, cost) => invoke('db_conversation_update_cost', { id, cost }),
  getMessages: (conversationId) => invoke('db_messages', { conversationId }),
  addMessage: (conversationId, role, content, imageBase64) =>
    invoke('db_message_add', { conversationId, role, content: content || '', imageBase64: imageBase64 || null }),
  searchMessages: (query) => invoke('db_search_messages', { query }),


  // ── LLM 通信 (Rust 引擎) ─────────────────────────────
  sendChat: (messages, globalFileAccess, agentMode, conversationId) => 
    invoke('llm_chat', { messages, conversationId, globalFileAccess, agentMode }),
  sendVision: (messages, imageBase64s, globalFileAccess, agentMode, conversationId) => 
    invoke('llm_vision', { messages, imageBase64s, conversationId, globalFileAccess, agentMode }),
  stopGeneration: async () => { /* 待实现 AbortController */ },
  getModels: async (provider) => {
    try {
      return await invoke('llm_get_models', { provider: provider || null });
    } catch (err) {
      console.error('getModels error:', err);
      return [];
    }
  },

  // ── 流式监听 (Event Listeners — 必须返回清理函数) ─────
  onStreamChunk: (callback) => {
    let unlisten = null;
    listen('llm:chunk', (event) => {
      callback(IS_TAURI ? event.payload : event);
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
      callback(IS_TAURI ? event.payload : event);
    }).then(fn => { unlisten = fn; });
    return () => { if (unlisten) { unlisten(); unlisten = null; } };
  },
  // T-1307: 待办提醒事件
  onTodoReminder: (callback) => {
    let unlisten = null;
    listen('todo:reminder', (event) => {
      callback(IS_TAURI ? event.payload : event);
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
  openLlmEngineDir: () => invoke('system_open_llm_engine_dir'),
  migrateWikiDir: (oldDir, newDir, mode) => invoke('system_migrate_wiki_dir', { oldDir, newDir, mode }),
  checkProjectIndex: (projectName) => invoke('system_check_project_index', { projectName }),
  
  // LLM-Wiki 知识库引擎事件
  onKBProgress: (callback) => {
    let unlisten = null;
    listen('kb:progress', (event) => {
      callback(IS_TAURI ? event.payload : event);
    }).then(fn => { unlisten = fn; });
    return () => { if (unlisten) { unlisten(); unlisten = null; } };
  },
  onKBComplete: (callback) => {
    let unlisten = null;
    listen('kb:complete', (event) => {
      callback(IS_TAURI ? event.payload : event);
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
  getTagProposals: async () => invoke('system_get_tag_proposals'),
  clearTagProposals: async () => invoke('system_clear_tag_proposals'),
  onDreamCompleted: (callback) => {
    let unlisten = null;
    listen('dream:completed', (event) => {
      callback(IS_TAURI ? event.payload : event);
    }).then(fn => { unlisten = fn; });
    return () => { if (unlisten) { unlisten(); unlisten = null; } };
  },

  // ── 进化引擎 (Rust 原生) ───────────────────────────────
  getEvolutionStats: async () => invoke('system_get_evolution_stats'),

  // ── 网页抓取 (Rust 原生 T-602) ─────────────────────────
  fetchUrl: async (url) => invoke('system_fetch_url', { url }),

  // ── 系统工具 (Rust 原生) ───────────────────────────────
  updateTheme: (theme) => console.log('Mock: updateTheme', theme), // TODO T-608
  getClipboardImage: async () => {
    try {
      const items = await navigator.clipboard.read();
      for (const item of items) {
        for (const type of item.types) {
          if (type.startsWith('image/')) {
            const blob = await item.getType(type);
            return new Promise((resolve) => {
              const reader = new FileReader();
              reader.onload = (e) => resolve(e.target.result.replace(/^data:image\/\w+;base64,/, ''));
              reader.readAsDataURL(blob);
            });
          }
        }
      }
    } catch (e) {
      console.error('getClipboardImage error:', e);
    }
    return null;
  },
  showNotification: async (title, body) => console.log('Mock: notification', title, body), // TODO T-608
  openFile: async (filePath) => invoke('system_open_file', { filePath }),
  showInFolder: async (filePath) => invoke('system_show_in_folder', { filePath }),
  startWebDrop: async (filePath) => invoke('start_web_drop', { filePath }),
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
      callback(IS_TAURI ? event.payload : event);
    }).then(fn => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  },

  // ── 插件系统 (Rust 原生) ───────────────────────────────
  getPlugins: async () => invoke('system_get_plugins'),
  importSkillsZip: async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Zip files', extensions: ['zip'] }]
    });
    if (selected) {
      await invoke('import_skills_zip', { path: selected });
      return true;
    }
    return false;
  },
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

  // Telegram API
  telegramSaveToken: async (token) => invoke('system_save_telegram_token', { token }),
  telegramGetToken: async () => invoke('system_get_telegram_token'),

  // Discord API
  discordSaveToken: async (token) => invoke('system_save_discord_token', { token }),
  discordGetToken: async () => invoke('system_get_discord_token'),

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
  takeScreenshot: async () => invoke('system_take_screenshot'),

  // ── M17: 知识图谱 (Knowledge Graph) ────────────────
  kgGetFullGraph: async () => invoke('kg_get_full_graph'),
  kgQuery: async (term, maxHops) => invoke('kg_query', { term, maxHops }),
  kgStats: async () => invoke('kg_stats'),
  kgDeleteNode: async (nodeId) => invoke('kg_delete_node_cmd', { nodeId }),
  kgBackfill: async () => invoke('kg_backfill'),
  systemRemoveSource: async (batchId) => invoke('system_remove_source', { batchId }),

  // ── 智能笔记 (Notebook) ──────────────────────────────
  notebookListNotes:   async () => invoke('notebook_list_notes'),
  notebookReadNote:    async (path) => invoke('notebook_read_note', { path }),
  notebookSaveNote:    async (path, content) => invoke('notebook_save_note', { path, content }),
  notebookCreateNote:  async (title, tags, category) => invoke('notebook_create_note', { title, tags, category: category || null }),
  notebookDeleteNote:  async (path) => invoke('notebook_delete_note', { path }),
  notebookMoveNote:    async (path, targetCategory) => invoke('notebook_move_note', { path, targetCategory }),
  notebookRenameNote:  async (oldPath, newTitle) => invoke('notebook_rename_note', { oldPath, newTitle }),
  notebookSearch:      async (query) => invoke('notebook_search', { query }),
  notebookAppendDaily: async (content) => invoke('notebook_append_daily', { content }),
  notebookSaveAsset:   async (fileName, data) => invoke('notebook_save_asset', { fileName, data }),
  notebookCreateFolder: async (name) => invoke('notebook_create_folder', { name }),
  notebookListAllTags: async () => invoke('notebook_list_all_tags'),
  notebookUpdateTags:  async (path, tags) => invoke('notebook_update_tags', { path, tags }),
  notebookGetBacklinks: async (path) => invoke('notebook_get_backlinks', { path }),
  notebookMergeTags: async (canonical, aliases) => invoke('notebook_merge_tags', { canonical, aliases }),
  notebookRejectTagMerge: async (tagA, tagB) => invoke('notebook_reject_tag_merge', { tagA, tagB }),

  // ── 同步引擎 (Phase 3 Mobile Sync) ──────────
  triggerMobileSync: async (payload) => invoke('trigger_mobile_sync', { payload }),
  writeMobileOutbox: async (operations) => invoke('write_mobile_outbox', { operations }),
  relayHandshake: async (targetDeviceId) => invoke('relay_handshake', { targetDeviceId }),

  // ── 扫码 (Mobile Only) ──────────────────────
  scanQrCode: async () => {
    if (!IS_TAURI) return null;
    try {
      const { scan, checkPermissions, requestPermissions } = await import('@tauri-apps/plugin-barcode-scanner');
      let perm = await checkPermissions();
      if (perm === 'prompt' || perm === 'prompt-with-rationale') {
        perm = await requestPermissions();
      }
      if (perm !== 'granted') {
        alert("需要相机权限才能扫码");
        return null;
      }
      const result = await scan({ windowed: true, formats: ['QR_CODE'] });
      return result?.content || null;
    } catch (e) {
      console.error("scanQrCode error:", e);
      return null;
    }
  },
  cancelQrCode: async () => {
    if (!IS_TAURI) return;
    try {
      const { cancel } = await import('@tauri-apps/plugin-barcode-scanner');
      await cancel();
    } catch (e) {
      console.error("cancelQrCode error:", e);
    }
  },

  // Generic invoke passthrough for components that call invoke directly
  invoke: (cmd, args) => invoke(cmd, args || {}),
};

console.log(`Tauri Bridge v6.0: 73 IPC — ${IS_TAURI ? 'Tauri native' : 'Browser mock'} mode.`);
