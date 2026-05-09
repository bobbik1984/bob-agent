# bob-agent 技术架构

## 总体架构

```
┌──────────────────────────────────────────────────┐
│                 Electron Shell                    │
│                                                   │
│  ┌─────────── Renderer (Vue 3 + Vite) ─────────┐ │
│  │                                              │ │
│  │  ChatView │ InboxView │ SettingsView         │ │
│  │  (对话+视觉) (日程+待办)  (设置)                │ │
│  │                                              │ │
│  │  Composables: useLLM │ useCalendar │ useTheme│ │
│  │                                              │ │
│  └────────────── contextBridge (IPC) ───────────┘ │
│                        │                          │
│  ┌─────────── Main Process (Node.js) ──────────┐ │
│  │                                              │ │
│  │  ┌───────────┐  ┌──────────┐  ┌──────────┐  │ │
│  │  │ LLM       │  │ TodoList │  │ File     │  │ │
│  │  │ Engine    │  │ Engine   │  │ Engine   │  │ │
│  │  │           │  │          │  │          │  │ │
│  │  │ • Chat    │  │ • Parser │  │ • Reader │  │ │
│  │  │ • Vision  │  │ • CalSync│  │ • Office │  │ │
│  │  │ • Stream  │  │ • Remind │  │ • PDF    │  │ │
│  │  │ • Compact │  │ • SQLite │  │ • Export │  │ │
│  │  └───────────┘  └──────────┘  └──────────┘  │ │
│  │                                              │ │
│  │  ┌──────────────────────────────────────┐    │ │
│  │  │ Infra: Config │ DB │ Tray │ HotKey   │    │ │
│  │  └──────────────────────────────────────┘    │ │
│  └──────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────┘
```

---

## 模块详解

### 1. LLM Engine (`electron/services/llm-client.js`)

**职责**：管理所有 LLM API 调用，支持多供应商 + Chat + Vision + 流式。

```javascript
// 核心接口
class LLMClient {
  constructor(config) // { provider, apiKey, baseURL, model }

  // 普通对话（流式）
  async *chatStream(messages) // yields { type: 'text'|'thinking', content }

  // 带图片的对话
  async *visionStream(messages, imageBase64) // 自动插入 image_url 块

  // 模型列表
  getAvailableModels() // 返回当前 provider 支持的模型
}
```

**供应商配置**：

```javascript
const PROVIDERS = {
  deepseek: {
    baseURL: 'https://api.deepseek.com',
    models: [
      { id: 'deepseek-v4-pro', label: '🧠 深度', vision: true },
      { id: 'deepseek-v4-flash', label: '⚡ 快速', vision: true },
    ]
  },
  openai: {
    baseURL: 'https://api.openai.com/v1',
    models: [
      { id: 'gpt-4.1', label: '🧠 GPT-4.1', vision: true },
      { id: 'gpt-4.1-mini', label: '⚡ GPT-4.1 Mini', vision: true },
    ]
  },
  ollama: {
    baseURL: 'http://localhost:11434/v1',
    models: [] // 动态检测
  },
  custom: {
    baseURL: '', // 用户自定义
    models: []
  }
};
```

### 2. Micro-compact (`electron/services/micro-compact.js`)

**职责**：防止长对话撑爆上下文窗口。零 LLM 调用，纯字符截断。

**来源**：移植自 CodeRunner `src/core/micro_compact.py`。

```javascript
// 核心逻辑
function microCompact(messages, { maxToolOutputChars = 8000 } = {}) {
  return messages.map(msg => {
    // 只截断工具返回和长消息，保留用户/系统消息完整
    if (msg.role === 'assistant' && msg.content?.length > maxToolOutputChars) {
      const head = msg.content.slice(0, Math.floor(maxToolOutputChars * 0.4));
      const tail = msg.content.slice(-Math.floor(maxToolOutputChars * 0.4));
      return { ...msg, content: `${head}\n\n[... ${msg.content.length - maxToolOutputChars} chars truncated ...]\n\n${tail}` };
    }
    return msg;
  });
}
```

### 3. Parser (`electron/services/parser.js`)

**职责**：自然语言 → 结构化事件。

**来源**：移植自 TodoList `src/core/parser.py`（126 行 Python → ~100 行 JS）。

**核心**：一个固定的 System Prompt + JSON mode 调用 LLM。

```javascript
// 与 TodoList parser.py 保持同步的 Prompt
const PARSER_SYSTEM_PROMPT = `你是一个精准的日程解析助手...` // 见 REQUIREMENTS.md

async function parseNaturalLanguage(text, llmClient) {
  const response = await llmClient.chat([
    { role: 'system', content: PARSER_SYSTEM_PROMPT },
    { role: 'user', content: text }
  ], { responseFormat: { type: 'json_object' }, temperature: 0.1 });

  return JSON.parse(response.content);
}
```

### 4. Calendar Sync (`electron/services/calendar.js`)

**职责**：Microsoft 365 日历 CRUD。

**来源**：移植自 TodoList `src/core/calendar_sync.py`（122 行 Python → ~100 行 JS）。

**认证**：OAuth 2.0 Client Credentials (MSAL)。凭据来自 `api-registry` → `microsoft_graph_azure` → `bobs_calendar`。

```javascript
// 关键 API
class CalendarClient {
  async createEvent(event)         // POST /me/events
  async listEvents(start, end)     // GET /me/calendarView
  async deleteEvent(eventId)       // DELETE /me/events/{id}
}
```

**重要**：TodoList (VPS1) 和 bob-agent (Windows) 操作的是**同一个 Microsoft 365 日历**，数据天然同步。

### 5. File Reader (`electron/services/file-reader.js`)

**职责**：读取本地文件并提取纯文本内容。

```javascript
// 支持的文件格式
const READERS = {
  // 纯文本（直接 fs.readFileSync）
  '.txt': readPlainText,
  '.md': readPlainText,
  '.csv': readPlainText,
  '.json': readPlainText,
  '.yaml': readPlainText,
  '.log': readPlainText,
  '.py': readPlainText,
  '.js': readPlainText,
  '.ts': readPlainText,

  // Office 格式（需要库）
  '.docx': readDocx,    // mammoth
  '.xlsx': readXlsx,    // xlsx
  '.pdf': readPdf,      // pdf-parse
};

// 大小限制：500KB
const MAX_FILE_SIZE = 500 * 1024;
```

### 6. Database (`electron/services/db.js`)

**职责**：本地 SQLite 持久化。

**表设计**（与 TodoList `database.py` 保持一致的 schema）：

```sql
-- 对话历史
CREATE TABLE conversations (
  id TEXT PRIMARY KEY,
  title TEXT,
  model TEXT,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id TEXT REFERENCES conversations(id),
  role TEXT NOT NULL, -- user / assistant / system
  content TEXT NOT NULL,
  image_base64 TEXT, -- 图片消息的 base64
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 日程事件（与 TodoList schema 一致）
CREATE TABLE events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  type TEXT DEFAULT 'event', -- event / todo / reminder
  title TEXT NOT NULL,
  start_time DATETIME,
  end_time DATETIME,
  location TEXT,
  description TEXT,
  priority TEXT DEFAULT 'medium',
  status TEXT DEFAULT 'pending', -- pending / confirmed / done / cancelled
  calendar_id TEXT, -- Microsoft 365 event ID
  source TEXT DEFAULT 'manual', -- manual / vision / file
  raw_input TEXT,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 配置
CREATE TABLE config (
  key TEXT PRIMARY KEY,
  value TEXT
);
```

---

## IPC 通道设计

Renderer → Main 通信通过 `contextBridge` 暴露安全 API：

```javascript
// preload.js
contextBridge.exposeInMainWorld('electronAPI', {
  // LLM
  sendChat: (messages) => ipcRenderer.invoke('llm:chat', messages),
  sendVision: (messages, image) => ipcRenderer.invoke('llm:vision', messages, image),
  onStreamChunk: (callback) => ipcRenderer.on('llm:chunk', callback),

  // 日历
  parseEvent: (text) => ipcRenderer.invoke('calendar:parse', text),
  confirmEvent: (event) => ipcRenderer.invoke('calendar:confirm', event),
  listEvents: (start, end) => ipcRenderer.invoke('calendar:list', start, end),
  deleteEvent: (id) => ipcRenderer.invoke('calendar:delete', id),

  // 文件
  readFile: (path) => ipcRenderer.invoke('file:read', path),
  selectFile: () => ipcRenderer.invoke('file:select'),

  // 对话历史
  getConversations: () => ipcRenderer.invoke('db:conversations'),
  getMessages: (id) => ipcRenderer.invoke('db:messages', id),

  // 配置
  getConfig: (key) => ipcRenderer.invoke('config:get', key),
  setConfig: (key, value) => ipcRenderer.invoke('config:set', key, value),

  // 系统
  captureScreen: () => ipcRenderer.invoke('system:capture'),
  showNotification: (title, body) => ipcRenderer.invoke('system:notify', title, body),
});
```

---

## 配置管理

用户配置存储在 `%APPDATA%/bob-agent/config.json`：

```json
{
  "provider": "deepseek",
  "apiKey": "sk-...(加密存储)",
  "model": "deepseek-v4-pro",
  "theme": "dark",
  "language": "zh-CN",
  "calendar": {
    "enabled": false,
    "azureClientId": "",
    "azureTenantId": "",
    "azureSecret": ""
  },
  "shortcuts": {
    "toggle": "Ctrl+Shift+Space",
    "screenshot": "Ctrl+Shift+S"
  }
}
```

API Key 使用 Electron `safeStorage` 加密存储。

---

## 构建与打包

```bash
# 开发
npm run dev          # Vite dev server + Electron

# 构建
npm run build        # Vite build → dist/
npm run pack         # electron-builder → installers/

# 打包配置 (package.json)
"build": {
  "appId": "com.bob.agent",
  "productName": "bob-agent",
  "win": {
    "target": ["nsis"],
    "icon": "assets/icon.ico"
  },
  "nsis": {
    "oneClick": true,
    "allowToChangeInstallationDirectory": false
  }
}
```
