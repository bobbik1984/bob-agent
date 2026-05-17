# bob-agent 技术架构

> **架构版本**: Tauri v2 (Rust) — 自 2026-05-10 完成从 Electron 的全面迁移
> **历史归档**: Electron 时代架构见 `docs/agents_electron.md`

## 总体架构

```
┌──────────────────────────────────────────────────────────┐
│                   Tauri v2 Shell (Rust)                   │
│                                                          │
│  ┌─────────── WebView (Vue 3 + Vite) ────────────────┐  │
│  │                                                    │  │
│  │  ChatView  │  InboxView(日程)  │  SettingsView     │  │
│  │  (对话+视觉+工具)  (时间轴+待办)  (配置+模型中心)     │  │
│  │                                                    │  │
│  └───────────── tauri-bridge.js (适配层) ─────────────┘  │
│                         │ invoke() / listen()            │
│  ┌─────────── Rust Backend (src-tauri/src/) ─────────┐  │
│  │                                                    │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────────────┐ │  │
│  │  │ llm.rs   │  │tools.rs  │  │ calendar.rs      │ │  │
│  │  │ LLM引擎  │  │工具引擎   │  │ 日程管理         │ │  │
│  │  │          │  │          │  │                  │ │  │
│  │  │ • Chat   │  │ • 12工具 │  │ • CRUD          │ │  │
│  │  │ • Vision │  │ • TC循环 │  │ • WeekTimeline  │ │  │
│  │  │ • SSE流  │  │ • 搜索   │  │ • 待办/日程     │ │  │
│  │  │ • ModelHub│  │ • 文件IO │  │ • SQLite        │ │  │
│  │  └──────────┘  └──────────┘  └──────────────────┘ │  │
│  │                                                    │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────────────┐ │  │
│  │  │outbox.rs │  │ dream.rs │  │ filesystem.rs    │ │  │
│  │  │声明式配置 │  │ 做梦引擎 │  │ 文件操作         │ │  │
│  │  │          │  │          │  │                  │ │  │
│  │  │ • 写Outbox│ │ • Session│  │ • 读/扫描/跟踪  │ │  │
│  │  │ • 校验墙 │  │ • 晨报   │  │ • walkdir       │ │  │
│  │  │ • 调谐器 │  │ • 记忆   │  │ • 500KB 限制    │ │  │
│  │  └──────────┘  └──────────┘  └──────────────────┘ │  │
│  │                                                    │  │
│  │  ┌─────────────────────────────────────────────┐  │  │
│  │  │ lib.rs: Config │ DB │ Tray │ SingleInstance │  │  │
│  │  └─────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

---

## Rust 后端模块详解 (`src-tauri/src/`)

### 1. LLM Engine (`llm.rs`)

**职责**：管理所有 LLM API 调用，支持多供应商 + Chat + Vision + SSE 流式 + Tool Calling。

```
核心函数:
  stream_internal()      — SSE 流式 + Tool Calling 循环 (≤5 轮) + 并行执行 + Restatement + 30s 超时
  get_model_pool()       — 返回 40+ 模型注册表 (含定价)
  get_active_models()    — 获取 Main/Clerk 角色绑定
  assign_model_role()    — 角色指派
  read_llm_config()      — 动态路由 Provider→API Key→Base URL
  build_memory_summary() — 三层记忆注入 (SOUL → Sessions → Wiki)

Restatement 机制 (防长线失焦):
  在 Tool Calling 循环的第 2 轮起，于 messages 数组尾部动态注入 system 消息，
  重申用户原始请求 + 人设约束。利用 LLM U 型注意力曲线 (首尾强，中间弱)，
  确保模型在消化大量工具结果后仍能准确归纳回答。

事件推送 (app.emit):
  "llm:chunk" → { type: "text"|"thinking"|"tool_start"|"tool_end"|"done"|"error", ... }
```

**供应商路由表**：

| Provider | Base URL | 模型示例 |
|----------|----------|----------|
| deepseek | api.deepseek.com | deepseek-chat, deepseek-reasoner |
| doubao | ark.cn-beijing.volces.com/api/v3 | doubao-1.5-pro-256k |
| qwen | dashscope.aliyuncs.com/compatible-mode/v1 | qwen3-235b-a22b |
| minimax | api.minimax.chat/v1 | MiniMax-M1 |
| glm | open.bigmodel.cn/api/paas/v4 | glm-4-plus |
| modelscope | api-inference.modelscope.cn/v1 | deepseek-v3-0324 (免费) |
| ollama | localhost:11434/v1 | 本地模型 |
| custom | 用户自定义 | 任意 OpenAI 兼容端点 |

### 2. Tool Calling Engine (`tools.rs`)

**职责**：Agent 的"手" — 12 个 Rust 原生工具 + 执行调度器。

```rust
// 工具注册表
pub fn get_tool_schemas() -> Vec<Value>    // OpenAI Function Calling 格式
pub async fn execute_tool(app, name, args) -> Value  // 异步执行调度 + 审计日志

// 安全层
fn resolve_write_path(path, global_file_access) -> Result<PathBuf, String>  // 路径白名单引擎
fn audit_tool_call(name, args, result_summary)  // 写入 logs/tools.log
```

| 工具 | 来源 | 描述 |
|------|------|------|
| `read_file` | filesystem.rs 复用 | 读取文本文件 (≤500KB) |
| `list_dir` | tools.rs 原生 | 浏览目录 |
| `write_file` | tools.rs 原生 | 安全写入 (resolve_write_path 白名单: wiki/workspaceDir/tracked_folders) |
| `append_file` | tools.rs 原生 | 追加内容 |
| `fetch_url` | web.rs 复用 | 网页抓取 (≤2MB, 10s 超时) |
| `web_search` | tools.rs 原生 | Tavily (主) + TinyFish (降级) |
| `list_skills` | tools.rs 原生 | 列出 49 个可用技能 |
| `read_skill` | tools.rs 原生 | 读取 SKILL.md 全文 |
| `system_time` | tools.rs 原生 | 当前时间+时区+星期 |
| `get_weather` | tools.rs 原生 | wttr.in 天气查询 |
| `brain_search` | tools.rs 原生 | 知识库全文检索 (wiki/) |
| `add_calendar_event` | tools.rs + calendar.rs | 写入 SQLite events 表 |

### 3. Calendar Engine (`calendar.rs`)

**职责**：日程/待办管理，SQLite 持久化。

```
Tauri Commands:
  system_list_events()        — 列出所有事件
  system_parse_event(text)    — 自然语言→结构化事件 (V1 关键词匹配)
  system_confirm_event(event) — 保存到数据库
  system_delete_event(id)     — 删除
  system_update_event_status(id, status) — 状态变更 (pending/done/cancelled)
  system_update_event_time(id, start, end) — 拖拽调整时间
```

### 4. Outbox/Reconciler (`outbox.rs`)

**职责**：AI 声明式配置引擎。AI 只写 Outbox 文件，Rust 守护者单向校验后生效。

```
数据流: AI 输出 bob-config 代码块 → writeOutbox() → bob_outbox.json
       → Reconciler (2s 轮询) → validate_operation() → config.json 生效
       → app.emit("config:reconciled") → 前端自动刷新

安全防线: 6 层校验 (op 白名单 + provider 合法性 + key 长度 + config key 白名单 + 备份 + 审计日志)
```

### 5. 其他模块

| 模块 | 职责 |
|------|------|
| `lib.rs` | 入口 + config CRUD + DB 初始化 + 所有 Tauri Command 注册 + 系统托盘 + 全局快捷键 (Ctrl+Shift+B) + 单实例锁 |
| `filesystem.rs` | 文件读取/扫描/文件夹跟踪 |
| `web.rs` | reqwest + scraper 网页抓取 |
| `plugins.rs` | 技能/插件扫描 (SKILL.md YAML frontmatter) |
| `dream.rs` | 做梦引擎 V2 (Clerk 异步压缩 + 7天冷热迁移 + 晨间简报) |
| `sidecar.rs` | Sidecar 子进程管理 (llama-server 离线推理) |
| `kb_extractor.rs` | 知识库文件提取 (PDF/DOCX/XLSX → 纯文本) |
| `kb_indexer.rs` | 知识库索引构建 |

---

## IPC 通信模型

Tauri v2 采用 `invoke()` (前端→Rust) + `app.emit()` (Rust→前端) 双通道：

```javascript
// src/tauri-bridge.js — 适配器层
window.electronAPI = {
  // 同步调用 Rust Command
  getConversations: () => invoke('db_conversations'),
  listEvents:      () => invoke('system_list_events'),
  
  // 监听 Rust 推送事件
  onStreamChunk: (cb) => listen('llm:chunk', (e) => cb(e.payload)),
  onConfigReconciled: (cb) => listen('config:reconciled', (e) => cb(e.payload)),
};
```

> **铁律**: Vue 组件统一调用 `window.electronAPI.xxx()`，不直接 import `@tauri-apps/api`。
> 所有 Tauri 特有 API 仅在 `tauri-bridge.js` 中使用。

---

## 数据库 Schema (SQLite / rusqlite)

数据库位于 `%LOCALAPPDATA%/bob-agent/bob.db`：

```sql
-- 对话历史
CREATE TABLE conversations (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL DEFAULT '新对话',
  model TEXT DEFAULT '',
  cost REAL DEFAULT 0.0,
  last_message TEXT,
  last_role TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  content TEXT NOT NULL DEFAULT '',
  image_base64 TEXT,
  created_at INTEGER NOT NULL
);

-- 日程事件
CREATE TABLE events (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL DEFAULT '',
  type TEXT NOT NULL DEFAULT 'event',    -- event | todo
  status TEXT NOT NULL DEFAULT 'pending', -- pending | done | cancelled
  date TEXT,
  start_time TEXT,                        -- YYYY-MM-DD HH:MM:SS 格式
  end_time TEXT,
  description TEXT DEFAULT '',
  created_at INTEGER NOT NULL
);

-- 配置键值
CREATE TABLE config (
  key TEXT PRIMARY KEY,
  value TEXT
);

-- 关注的文件夹
CREATE TABLE tracked_folders (
  id TEXT PRIMARY KEY,
  folder_path TEXT NOT NULL UNIQUE,
  created_at INTEGER NOT NULL
);
```

---

## 配置管理

用户配置存储在 `%LOCALAPPDATA%/bob-agent/config.json`：

```json
{
  "setupComplete": true,
  "language": "zh-CN",
  "theme": "dark",
  "accentColor": "blue",
  "wikiDir": "D:\\path\\to\\wiki",
  "externalSkillsDir": "D:\\path\\to\\skills",
  "apiKeys": {
    "deepseek": "sk-...",
    "doubao": "...",
    "tavily": "tvly-..."
  },
  "mainModel": "deepseek-chat",
  "clerkModel": "deepseek-v3-0324"
}
```

> ⚠️ API Key 目前明文存储。T-303 计划迁移至 `tauri-plugin-stronghold` 加密存储。

---

## 构建与打包

```bash
# 开发 (Vite 热更新 + Rust 编译)
npm run dev:tauri

# 生产打包 (~15MB 安装包)
npm run build:tauri

# 仅检查 Rust 编译
cd src-tauri && cargo check

# 前端测试
npm test
```
