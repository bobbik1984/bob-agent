# TodoList 集成指南

## 概述

bob-agent 的"智能收件箱"功能源自 `todolist` 项目。本文档记录两个项目的代码对应关系和同步策略。

## TodoList 项目位置

```
D:\OneDrive\Learning\Code\Gemini\todolist\
```

GitHub: `bobbik1984/todolist` (私有)

## 代码映射

### 需要移植的模块

| TodoList 文件 | bob-agent 对应 | 移植说明 |
|--------------|---------------|---------|
| `src/core/parser.py` (126行) | `electron/services/parser.js` | Python → JS。核心是 System Prompt + OpenAI 兼容 API 调用 + JSON 解析。Prompt 文本两边必须一致 |
| `src/core/calendar_sync.py` (122行) | `electron/services/calendar.js` | `httpx` → `fetch`，`msal` → `@azure/msal-node`。API 路径和参数完全相同 |
| `src/models/database.py` (~200行) | `electron/services/db.js` | `aiosqlite` → `better-sqlite3`。events 表 schema 保持一致 |
| `src/models/event.py` (~100行) | TypeScript interface 或 JSDoc | Pydantic model → TS interface |
| `static/app.js` WeekTimeline 部分 | `src/components/WeekTimeline.vue` | 已经是 JS，提取 timeline 渲染逻辑到 Vue 组件 |

### 不需要移植的模块

| TodoList 文件 | 理由 |
|--------------|------|
| `src/core/notifier.py` | Discord 推送 → 改用 Electron `Notification` |
| Discord Bot 相关 | bob-agent 不需要 Discord |
| Telegram Bot 相关 | bob-agent 不需要 Telegram |
| `src/main.py` (FastAPI) | bob-agent 不是 Web 服务 |

## Parser System Prompt 同步

**这是两个项目之间唯一需要保持同步的文本资产。**

TodoList 版本 (`src/core/parser.py` L22-56):

```
你是一个精准的日程解析助手。用户会发给你一段自然语言文字，你需要从中提取日程事件信息。

输出规则：
1. 必须输出严格的 JSON，不要添加任何 markdown 标记
2. 所有时间使用 ISO 8601 格式，时区默认 Asia/Shanghai (+08:00)
3. 未指定结束时间默认 +1 小时
4. 没有明确时间的待办任务 type 设为 "todo"
5. 完全无法理解时 type 设为 "unknown"

输出 Schema:
{
  "type": "event" | "todo" | "reminder",
  "confidence": 0.0-1.0,
  "title": "简洁的事件标题",
  "start_time": "ISO 8601",
  "end_time": "ISO 8601",
  "location": "地点",
  "description": "补充描述",
  "participants": ["参与者"],
  "priority": "low" | "medium" | "high",
  "tags": ["标签"],
  "message": null
}
```

**同步频率**：每月对齐一次。如果 bob-agent 改了 Prompt，记得同步回 TodoList。

## 数据库 Schema 对齐

TodoList events 表 (`src/models/database.py`):

```sql
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    type TEXT DEFAULT 'event',        -- event / todo / reminder
    title TEXT NOT NULL,
    start_time TEXT,                   -- ISO 8601
    end_time TEXT,
    location TEXT,
    description TEXT,
    participants TEXT,                 -- JSON array
    priority TEXT DEFAULT 'medium',    -- low / medium / high
    tags TEXT,                         -- JSON array
    status TEXT DEFAULT 'pending',     -- pending / confirmed / done / cancelled
    raw_input TEXT,                    -- 原始用户输入
    calendar_event_id TEXT,           -- Microsoft 365 日历事件 ID
    confidence REAL,
    source TEXT DEFAULT 'user',        -- user / telegram / discord
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

bob-agent 的 events 表应保持相同的列定义，`source` 字段增加 `vision` 和 `file` 值。

## Microsoft 365 日历同步

**关键事实**：两个项目操作的是**同一个 Microsoft 365 日历**。

```
bob-agent (Windows) ──→ Microsoft Graph API ←── TodoList (VPS1)
                         同一个 calendar
```

凭据来源：`api-registry` → `microsoft_graph_azure` → `bobs_calendar`

```
AZURE_CLIENT_ID     = (查询 api-registry)
AZURE_TENANT_ID     = (查询 api-registry)
AZURE_CALENDAR_SECRET = (查询 api-registry)
GRAPH_API_BASE      = https://graph.microsoft.com/v1.0/users/{user_id}
GRAPH_SCOPES        = ["https://graph.microsoft.com/.default"]
```

## WeekTimeline 移植指南

TodoList 的 `static/app.js` 包含一个完整的周时间轴渲染器（Gantt 风格）。

**移植步骤**：
1. 从 `app.js` 中提取 `renderWeekTimeline()` 相关函数
2. 将 DOM 操作改写为 Vue 3 模板 + reactive 数据
3. 保留 CSS 样式（`static/style.css` 中的 `.timeline-*` 类）
4. 数据源改为从 SQLite 读取（替代 API 调用）

**注意**：TodoList 已实现 PC 水平段与 Mobile 垂直流两种排版。bob-agent 桌面端只需要 PC 水平段版本。
