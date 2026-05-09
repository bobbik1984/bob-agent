/**
 * bob-agent — Electron 主进程入口
 *
 * 职责：
 * - 创建 BrowserWindow + 加载前端
 * - 注册所有 IPC handler
 * - 初始化后端服务 (LLM / DB / Calendar 等)
 */

const { app, BrowserWindow, ipcMain, dialog, clipboard, nativeImage, Notification } = require('electron');
const path = require('path');
const { LLMClient } = require('./services/llm-client');
const { Database } = require('./services/db');

// ─── 全局单例 ───────────────────────────────────────────
let mainWindow = null;
let llmClient = null;
let db = null;

const isDev = !app.isPackaged;

// ─── 窗口创建 ───────────────────────────────────────────
function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1100,
    height: 750,
    minWidth: 800,
    minHeight: 600,
    title: 'bob-agent',
    backgroundColor: '#0a0a0f',
    titleBarStyle: 'hidden',
    titleBarOverlay: {
      color: '#0a0a0f',
      symbolColor: '#a0a0b0',
      height: 36,
    },
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false, // better-sqlite3 需要
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

// ─── 初始化后端服务 ─────────────────────────────────────
function initServices() {
  // 数据库
  db = new Database(app.getPath('userData'));

  // LLM Client — 从数据库配置加载
  const provider = db.getConfig('provider') || 'deepseek';
  const apiKey = db.getConfig('apiKey') || '';
  const model = db.getConfig('model') || '';

  llmClient = new LLMClient({ provider, apiKey, model });
}

// ─── IPC Handlers ───────────────────────────────────────

function registerIPCHandlers() {
  // ── LLM ──────────────────────────────────────────────
  ipcMain.handle('llm:chat', async (_event, messages) => {
    if (!llmClient || !llmClient.isConfigured()) {
      return { error: 'LLM 未配置，请先在设置中填写 API Key' };
    }

    try {
      const stream = llmClient.chatStream(messages);
      let fullContent = '';
      let thinkingContent = '';

      for await (const chunk of stream) {
        // 推送流式 chunk 到渲染进程
        mainWindow?.webContents.send('llm:chunk', chunk);

        if (chunk.type === 'text') {
          fullContent += chunk.content;
        } else if (chunk.type === 'thinking') {
          thinkingContent += chunk.content;
        }
      }

      return { content: fullContent, thinking: thinkingContent || null };
    } catch (err) {
      console.error('[LLM] Chat error:', err.message);
      return { error: err.message };
    }
  });

  ipcMain.handle('llm:vision', async (_event, messages, imageBase64) => {
    if (!llmClient || !llmClient.isConfigured()) {
      return { error: 'LLM 未配置' };
    }

    try {
      const stream = llmClient.visionStream(messages, imageBase64);
      let fullContent = '';
      let thinkingContent = '';

      for await (const chunk of stream) {
        mainWindow?.webContents.send('llm:chunk', chunk);
        if (chunk.type === 'text') fullContent += chunk.content;
        else if (chunk.type === 'thinking') thinkingContent += chunk.content;
      }

      return { content: fullContent, thinking: thinkingContent || null };
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
        { name: '支持的文件', extensions: ['txt', 'md', 'csv', 'json', 'yaml', 'log', 'py', 'js', 'ts', 'java', 'go', 'sql', 'docx', 'xlsx', 'pdf'] },
        { name: '所有文件', extensions: ['*'] },
      ],
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
      llmClient = new LLMClient({ provider, apiKey, model });
    }
    return { ok: true };
  });

  ipcMain.handle('config:all', async () => {
    return db.getAllConfig();
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
