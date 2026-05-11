/**
 * bob-agent — Electron 主进程入口
 *
 * 职责：
 * - 创建 BrowserWindow + 加载前端
 * - 注册所有 IPC handler
 * - 初始化后端服务 (LLM / DB / Calendar 等)
 */

// 加载项目根目录的 .env 文件（必须在其他 require 之前）
require('dotenv').config({ path: require('path').join(__dirname, '..', '.env') });

const { app, BrowserWindow, ipcMain, dialog, clipboard, nativeImage, Notification } = require('electron');
const path = require('path');
const { LLMClient } = require('./services/llm-client');
const { ToolRegistry } = require('./tools/registry');
const { Database } = require('./services/db');

// ─── 全局单例 ───────────────────────────────────────────
let mainWindow = null;
let llmClient = null;
let toolRegistry = null;
let db = null;

const isDev = !app.isPackaged;

// ─── System Prompt ──────────────────────────────────────
function buildSystemPrompt(globalFileAccess = false) {
  const workspaceDir = db?.getConfig('workspaceDir') || '';
  const externalSkillsDir = db?.getConfig('externalSkillsDir') || '';

  const now = new Date();
  const timeString = now.toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' });

  // 通用的目录摘要工具函数
  function scanDir(dirPath, maxItems = 50) {
    const fs = require('fs');
    try {
      if (!fs.existsSync(dirPath)) return '(目录不存在)';
      const entries = fs.readdirSync(dirPath, { withFileTypes: true });
      const items = entries
        .filter(e => !e.name.startsWith('.'))
        .slice(0, maxItems)
        .map(e => {
          if (e.isDirectory()) return `  [DIR]  ${e.name}/`;
          const size = fs.statSync(path.join(dirPath, e.name)).size;
          const kb = (size / 1024).toFixed(1);
          return `  [FILE] ${e.name} (${kb}KB)`;
        });
      return items.join('\n') || '(目录为空)';
    } catch (err) {
      console.error(`[SystemPrompt] Failed to read dir ${dirPath}:`, err.message);
      return '(无法读取)';
    }
  }

  let prompt = `你是 bob-agent，一个运行在用户 Windows 桌面上的 AI 私人秘书。
当前系统时间：${timeString}

你拥有以下能力（通过 Electron 后端实现）：
1. **本地文件读取**：用户可以拖拽文件到对话窗口，或者通过粘贴操作分享文件，你能读取 txt/md/json/csv/py/js/docx/xlsx/pdf 等格式的文件内容。
2. **图片识别 (Vision)**：用户可以粘贴截图或拖入图片，你可以识别并分析图片内容。
3. **日程与待办管理**：你可以从用户的自然语言中提取日程和待办事项，保存到本地 SQLite 数据库，并在"智能收件箱"中展示周历和待办清单。
4. **剪贴板访问**：你可以读取用户剪贴板中的图片。
5. **工具调用 (Function Calling)**：你拥有以下可直接调用的工具——
   - **web_search**：搜索互联网获取实时信息。当用户询问最新新闻、价格、模型发布等需要联网的问题时，**必须主动调用此工具**，不要说"我无法联网"。
   - **list_directory**：列出指定目录下的文件和文件夹。
   - **read_file**：读取指定路径的文本文件内容。
   当你判断需要使用工具时，直接调用即可，不需要用户授权。搜索到结果后，用中文为用户总结关键信息。`;

  if (workspaceDir) {
    const dirListing = scanDir(workspaceDir);
    prompt += `
5. **工作目录浏览**：用户已配置工作目录为 \`${workspaceDir}\`。你可以直接看到该目录下的内容。

当前工作目录文件列表：
\`\`\`
${dirListing}
\`\`\`

当用户问"文件夹里有什么"时，直接根据上面的列表回答。当用户要求读取某个文件时，告诉他们可以把文件拖进对话窗口，你就能分析内容。`;
  }

  if (externalSkillsDir) {
    const skillsListing = scanDir(externalSkillsDir, 30);
    prompt += `
6. **外部技能目录**：用户已配置外部技能目录为 \`${externalSkillsDir}\`。这个目录包含可扩展的 Agent 技能模块，你也可以浏览和引用其中的内容。

外部技能目录内容：
\`\`\`
${skillsListing}
\`\`\``;
  }

  prompt += `

重要行为准则：
- 你是一个桌面原生应用内的助手，不是网页聊天机器人。`;

  if (globalFileAccess) {
    prompt += `
- **[⚠️ 最高授权覆盖]**：用户已在此会话中临时授予你"全局文件访问权限"。你可以突破工作目录和技能目录的限制，**自由读取或写入当前计算机上的任意绝对路径文件**。当用户要求操作系统文件或其他盘符的文件时，请直接执行操作，不要拒绝。`;
  } else {
    const accessibleDirs = [];
    if (workspaceDir) accessibleDirs.push(`工作目录(${workspaceDir})`);
    if (externalSkillsDir) accessibleDirs.push(`外部技能目录(${externalSkillsDir})`);

    if (accessibleDirs.length > 0) {
      prompt += `你可以直接访问以下目录：${accessibleDirs.join('、')}。当用户提到"文件"、"文件夹"、"技能"时，你可以直接帮他们浏览这些目录中的内容，也可以引导他们拖拽文件到对话窗口。`;
    } else {
      prompt += `当用户提到"文件"、"文件夹"时，引导他们通过拖拽文件到对话窗口来分享，或者建议他们在"设置"中配置工作目录以获得文件浏览能力。`;
    }
  }

  prompt += `
- 保持专业、简洁、有帮助。使用用户的语言（中文）回答。
- 【强制规则 — 日程/待办自动检测】任何时候，只要用户消息中同时包含"时间"和"活动/事件/地点"两个要素（例如："明天下午3点开会"、"周六去深圳湾"、"提醒我买牛奶"），你 **必须** 在回复文字之后、消息最末尾附上如下 XML 标签。没有任何例外，即使你已经用文字确认了也必须输出此标签：
  <calendar_event>
  {"type": "event", "title": "活动标题", "start_time": "YYYY-MM-DD HH:mm:ss", "end_time": "YYYY-MM-DD HH:mm:ss"}
  </calendar_event>
  如果没有明确结束时间，默认持续 1 小时。如果是待办事项（没有具体时间段），type 改为 "todo"。
- 你的名字是 bob-agent，是用户的私人 AI 桌面助理。`;

  return prompt;
}

// ─── 窗口创建 ───────────────────────────────────────────
function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1100,
    height: 750,
    minWidth: 800,
    minHeight: 600,
    title: 'bob-agent',
    backgroundColor: '#0c0c0c',
    titleBarStyle: 'hidden',
    titleBarOverlay: {
      color: '#141414',
      symbolColor: '#a0a0a0',
      height: 36,
    },
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true, // better-sqlite3 需要
    },
    show: false,
  });

  // 优雅显示：ready-to-show 后再展示，避免白屏闪烁
  mainWindow.once('ready-to-show', () => {
    mainWindow.show();
  });

  // 开发模式加载 Vite dev server，生产模式加载打包产物
  if (isDev) {
    mainWindow.loadURL('http://localhost:5173');
    // mainWindow.webContents.openDevTools(); // 需要时取消注释
  } else {
    mainWindow.loadFile(path.join(__dirname, '..', 'dist', 'index.html'));
  }
}

// ─── 安全状态 ───────────────────────────────────────────
global.securityState = {
  globalFileAccess: false,
  agentMode: 'insight'
};

// ─── 初始化后端服务 ─────────────────────────────────────
function initServices() {
  // 数据库
  db = new Database(app.getPath('userData'));
  global.db = db;

  // LLM Client — 从数据库配置加载
  const provider = db.getConfig('provider') || 'deepseek';
  const apiKey = db.getConfig('apiKey') || '';
  const model = db.getConfig('model') || '';

  toolRegistry = new ToolRegistry();
  toolRegistry.init(path.join(__dirname, 'tools', 'built-in'));

  llmClient = new LLMClient({ provider, apiKey, model, registry: toolRegistry });
}

// ─── IPC Handlers ───────────────────────────────────────

function registerIPCHandlers() {
  // ── 安全与权限 ───────────────────────────────────────
  ipcMain.handle('security:toggle-global-access', async (_event, value) => {
    global.securityState.globalFileAccess = !!value;
    return true;
  });

  ipcMain.handle('security:set-agent-mode', async (_event, mode) => {
    if (['insight', 'yolo'].includes(mode)) {
      global.securityState.agentMode = mode;
    }
    return true;
  });

  // ── LLM ──────────────────────────────────────────────
  ipcMain.handle('llm:chat', async (_event, messages) => {
    const { globalFileAccess, agentMode } = global.securityState;
    if (!llmClient || !llmClient.isConfigured()) {
      return { error: 'LLM 未配置，请先在设置中填写 API Key' };
    }

    try {
      const hasSystemMsg = messages.some(m => m.role === 'system');
      const fullMessages = hasSystemMsg
        ? messages
        : [{ role: 'system', content: buildSystemPrompt(globalFileAccess) }, ...messages];

      const stream = llmClient.chatStream(fullMessages, agentMode);
      let fullContent = '';
      let thinkingContent = '';
      let usageData = null;
      let modelId = null;

      for await (const chunk of stream) {
        mainWindow?.webContents.send('llm:chunk', chunk);

        if (chunk.type === 'text') {
          fullContent += chunk.content;
        } else if (chunk.type === 'thinking') {
          thinkingContent += chunk.content;
        } else if (chunk.type === 'done') {
          usageData = chunk.usage || null;
          modelId = chunk.model || null;
        }
      }

      const pricing = llmClient.getPricing(modelId);
      return {
        content: fullContent,
        thinking: thinkingContent || null,
        usage: usageData,
        model: modelId,
        pricing,
      };
    } catch (err) {
      console.error('[LLM] Chat error:', err.message);
      return { error: err.message };
    }
  });

  ipcMain.handle('llm:vision', async (_event, messages, imageBase64) => {
    const { globalFileAccess, agentMode } = global.securityState;
    if (!llmClient || !llmClient.isConfigured()) {
      return { error: 'LLM 未配置' };
    }

    try {
      const hasSystemMsg = messages.some(m => m.role === 'system');
      const fullMessages = hasSystemMsg
        ? messages
        : [{ role: 'system', content: buildSystemPrompt(globalFileAccess) }, ...messages];

      const stream = llmClient.visionStream(fullMessages, imageBase64, agentMode);
      let fullContent = '';
      let thinkingContent = '';
      let usageData = null;
      let modelId = null;

      for await (const chunk of stream) {
        mainWindow?.webContents.send('llm:chunk', chunk);
        if (chunk.type === 'text') fullContent += chunk.content;
        else if (chunk.type === 'thinking') thinkingContent += chunk.content;
        else if (chunk.type === 'done') {
          usageData = chunk.usage || null;
          modelId = chunk.model || null;
        }
      }

      const pricing = llmClient.getPricing(modelId);
      return {
        content: fullContent,
        thinking: thinkingContent || null,
        usage: usageData,
        model: modelId,
        pricing,
      };
    } catch (err) {
      console.error('[LLM] Vision error:', err.message);
      return { error: err.message };
    }
  });

  ipcMain.handle('llm:stop', async () => {
    llmClient?.abort();
    return { ok: true };
  });

  ipcMain.handle('llm:models', async () => {
    return llmClient?.getAvailableModels() || [];
  });

  // ── 日历 (Parser + Calendar) ─────────────────────────
  ipcMain.handle('calendar:parse', async (_event, text) => {
    try {
      const { parseNaturalLanguage } = require('./services/parser');
      return await parseNaturalLanguage(text, llmClient);
    } catch (err) {
      console.error('[Parser] Error:', err.message);
      return { error: err.message };
    }
  });

  ipcMain.handle('calendar:confirm', async (_event, event) => {
    try {
      const id = db.createEvent(event);
      return { ok: true, id };
    } catch (err) {
      console.error('[Calendar] Confirm error:', err.message);
      return { error: err.message };
    }
  });

  ipcMain.handle('calendar:list', async (_event, start, end) => {
    try {
      return db.listEvents(start, end);
    } catch (err) {
      console.error('[Calendar] List error:', err.message);
      return { error: err.message };
    }
  });

  ipcMain.handle('calendar:delete', async (_event, id) => {
    try {
      db.deleteEvent(id);
      return { ok: true };
    } catch (err) {
      console.error('[Calendar] Delete error:', err.message);
      return { error: err.message };
    }
  });

  ipcMain.handle('calendar:update-status', async (_event, id, status) => {
    try {
      db.updateEventStatus(id, status);
      return { ok: true };
    } catch (err) {
      console.error('[Calendar] Update status error:', err.message);
      return { error: err.message };
    }
  });

  ipcMain.handle('calendar:update-time', async (_event, id, startTime, endTime) => {
    try {
      db.updateEventTime(id, startTime, endTime);
      return { ok: true };
    } catch (err) {
      console.error('[Calendar] Update time error:', err.message);
      return { error: err.message };
    }
  });

  // ── 文件 ─────────────────────────────────────────────
  ipcMain.handle('file:read', async (_event, filePath) => {
    try {
      const { readFile } = require('./services/file-reader');
      return await readFile(filePath);
    } catch (err) {
      console.error('[File] Read error:', err.message);
      return { error: err.message };
    }
  });

  ipcMain.handle('file:select', async () => {
    const result = await dialog.showOpenDialog(mainWindow, {
      properties: ['openFile'],
      filters: [
        { name: '图片', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'] },
        { name: '文档', extensions: ['txt', 'md', 'csv', 'json', 'yaml', 'log', 'py', 'js', 'ts', 'java', 'go', 'sql', 'docx', 'xlsx', 'pdf'] },
        { name: '所有文件', extensions: ['*'] },
      ],
    });
    if (result.canceled || result.filePaths.length === 0) return null;

    const filePath = result.filePaths[0];
    const fileName = path.basename(filePath);
    const ext = path.extname(filePath).toLowerCase();
    const imageExts = ['.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp'];

    if (imageExts.includes(ext)) {
      // Read image as base64
      const fs = require('fs');
      const buffer = fs.readFileSync(filePath);
      const base64 = buffer.toString('base64');
      return { name: fileName, type: 'image', content: base64 };
    } else {
      // Use file-reader for text files
      const { readFile } = require('./services/file-reader');
      const textResult = await readFile(filePath);
      return { name: fileName, type: 'text', content: textResult.content, error: textResult.error };
    }
  });

  // ── 工作目录（沙箱化文件浏览）─────────────────────
  ipcMain.handle('workspace:list-dir', async (_event, relativePath = '') => {
    try {
      const workspaceRoot = db.getConfig('workspaceDir');
      if (!workspaceRoot) return { error: '未设置工作目录，请在设置中配置' };

      const fs = require('fs');
      const targetPath = path.resolve(workspaceRoot, relativePath);

      // 安全检查：防止路径遍历越界
      const normalizedWorkspace = path.resolve(workspaceRoot);
      const normalizedTarget = path.resolve(targetPath);
      if (normalizedTarget !== normalizedWorkspace && !normalizedTarget.startsWith(normalizedWorkspace + path.sep)) {
        return { error: '禁止访问工作目录之外的路径' };
      }

      if (!fs.existsSync(targetPath)) {
        return { error: `路径不存在: ${relativePath || '/'}` };
      }

      const stat = fs.statSync(targetPath);
      if (!stat.isDirectory()) {
        return { error: '指定路径不是目录' };
      }

      const entries = fs.readdirSync(targetPath, { withFileTypes: true });
      const items = entries
        .filter(e => !e.name.startsWith('.')) // 隐藏文件过滤
        .map(e => ({
          name: e.name,
          isDir: e.isDirectory(),
          size: e.isDirectory() ? null : fs.statSync(path.join(targetPath, e.name)).size,
        }))
        .sort((a, b) => {
          // 目录优先，然后按名称排序
          if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
          return a.name.localeCompare(b.name);
        });

      return { path: relativePath || '/', items };
    } catch (err) {
      console.error('[Workspace] List dir error:', err.message);
      return { error: err.message };
    }
  });

  ipcMain.handle('workspace:read-file', async (_event, relativePath) => {
    try {
      const workspaceRoot = db.getConfig('workspaceDir');
      if (!workspaceRoot) return { error: '未设置工作目录' };

      const targetPath = path.resolve(workspaceRoot, relativePath);

      // 安全检查
      const normalizedWorkspace = path.resolve(workspaceRoot);
      const normalizedTarget = path.resolve(targetPath);
      if (normalizedTarget !== normalizedWorkspace && !normalizedTarget.startsWith(normalizedWorkspace + path.sep)) {
        return { error: '禁止访问工作目录之外的文件' };
      }

      const { readFile } = require('./services/file-reader');
      return await readFile(targetPath);
    } catch (err) {
      console.error('[Workspace] Read file error:', err.message);
      return { error: err.message };
    }
  });

  ipcMain.handle('workspace:select-dir', async () => {
    const result = await dialog.showOpenDialog(mainWindow, {
      properties: ['openDirectory'],
      title: '选择工作目录',
    });
    if (result.canceled || result.filePaths.length === 0) return null;
    return result.filePaths[0];
  });

  // ── 对话历史 ─────────────────────────────────────────
  ipcMain.handle('db:conversations', async () => {
    return db.getConversations();
  });

  ipcMain.handle('db:conversation:create', async (_event, title, model) => {
    return db.createConversation(title, model);
  });

  ipcMain.handle('db:conversation:delete', async (_event, id) => {
    db.deleteConversation(id);
    return { ok: true };
  });

  ipcMain.handle('db:conversation:rename', async (_event, id, title) => {
    db.updateConversationTitle(id, title);
    return { ok: true };
  });

  ipcMain.handle('db:messages', async (_event, conversationId) => {
    return db.getMessages(conversationId);
  });

  ipcMain.handle('db:message:add', async (_event, conversationId, role, content, imageBase64) => {
    return db.addMessage(conversationId, role, content, imageBase64);
  });

  // ── 配置 ─────────────────────────────────────────────
  ipcMain.handle('config:get', async (_event, key) => {
    return db.getConfig(key);
  });

  ipcMain.handle('config:set', async (_event, key, value) => {
    db.setConfig(key, value);
    // 配置变更时重建 LLM Client
    if (['provider', 'apiKey', 'model'].includes(key)) {
      const provider = db.getConfig('provider') || 'deepseek';
      const apiKey = db.getConfig('apiKey') || '';
      const model = db.getConfig('model') || '';
      llmClient = new LLMClient({ provider, apiKey, model, registry: toolRegistry });
    }
    return { ok: true };
  });

  ipcMain.handle('config:all', async () => {
    const config = db.getAllConfig();
    if (!config) return config;
    const safeConfig = { ...config };
    delete safeConfig.apiKey;
    return safeConfig;
  });

  // ── 系统 ─────────────────────────────────────────────
  ipcMain.handle('system:clipboard-image', async () => {
    const img = clipboard.readImage();
    if (img.isEmpty()) return null;
    return img.toDataURL().replace(/^data:image\/\w+;base64,/, '');
  });

  ipcMain.handle('system:notify', async (_event, title, body) => {
    if (Notification.isSupported()) {
      new Notification({ title, body }).show();
    }
    return { ok: true };
  });

  ipcMain.handle('system:is-setup-complete', async () => {
    const apiKey = db.getConfig('apiKey');
    return !!apiKey;
  });

  ipcMain.handle('system:select-dir', async () => {
    const { dialog } = require('electron');
    const result = await dialog.showOpenDialog(mainWindow, {
      properties: ['openDirectory'],
    });
    if (!result.canceled && result.filePaths.length > 0) {
      return result.filePaths[0];
    }
    return null;
  });
}

// ─── App 生命周期 ───────────────────────────────────────
app.whenReady().then(() => {
  initServices();
  createWindow();
  registerIPCHandlers();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
