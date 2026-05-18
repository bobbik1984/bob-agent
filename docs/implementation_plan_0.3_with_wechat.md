# Bob-Agent × 微信接入开发文档 (V3 — 完整版)

## 用户决策定稿

| # | 问题 | 决策 |
|---|---|---|
| 1 | 流式 vs 一次性回复 | **流式**：Bridge 先发 `sendTyping`，Bob 回复后立即 `sendMessage`。若回复过长则分段发送。 |
| 2 | Tool Calling | **支持**：Bob 完整执行 tool calling 循环（最多 5 轮），Bridge 负责在等待期间持续刷新 `sendTyping` 状态。 |
| 3 | 对话跨端共享 | **共享**：微信对话存入 Bob 的同一个 SQLite 数据库，桌面端可看到微信对话，反之亦然。 |
| 4 | 会话(Session)管理 | **支持指令切换**：默认继承/维持当前活跃会话；支持发送 `/chat` 列出最近会话，回复序号实现无缝跨端切换。 |
| 5 | 非文字消息 | **降级处理**：图片/语音/文件返回友好提示，不静默丢弃。 |
| 6 | Session 状态持久化 | **持久化**：Bridge 重启后恢复用户绑定关系，不强制新建会话。 |

---

## 总体架构

```
┌──────────┐   长轮询    ┌─────────────────────┐   SSE Stream    ┌─────────────────┐
│ 微信 App │◄─────────►│ wechat-bot-bridge   │◄──────────────►│ Bob-Agent       │
│ (手机)   │  腾讯云     │ (Node.js 微服务)    │ localhost:3721  │ (Tauri/Rust)    │
└──────────┘            │                     │                 │                 │
                        │ ① getUpdates 收消息  │                 │ ① /v1/chat SSE  │
                        │ ② sendTyping 状态   │                 │ ② LLM + Tools   │
                        │ ③ 分段 sendMessage  │                 │ ③ SQLite 存储   │
                        │ ④ 指令状态机        │                 │ ④ 广播 UI 更新  │
                        └─────────────────────┘                 └─────────────────┘
```

### 项目拓扑 (3 个独立项目)

| 项目 | 路径 | 语言 | 职责 |
|---|---|---|---|
| `wechat-bot-core` | `Gemini/openclaw-weixin/` (改造后) | TypeScript | 微信 API 底层封装（收发消息、登录、CDN 加密） |
| `wechat-bot-bridge` | `Gemini/wechat-bot-bridge/` (新建) | TypeScript | 桥接层：微信消息 ↔ Bob API 路由 + 指令状态机 |
| `bob-agent` | `Gemini/bob-agent/` (扩展) | Rust | 新增 HTTP API 模块，暴露 SSE 流式端点 |

---

## Phase 1: 剥离 `wechat-bot-core`

**目标**：移除 OpenClaw SDK 依赖，导出纯净的微信通信 API。

### 1.1 改造清单

#### [MODIFY] `package.json`
- 移除 `peerDependencies.openclaw` 和 `devDependencies.openclaw`
- 改名 `"name": "@gemini/wechat-bot-core"`
- 删除整个 `"openclaw"` 配置段

#### [DELETE] `openclaw.plugin.json`, 旧 `index.ts`

#### [MODIFY] `src/monitor/monitor.ts` — 核心改造

将 `processOneMessage` (OpenClaw 耦合) 替换为回调函数：

```typescript
export type OnMessageCallback = (msg: WeixinMessage) => Promise<string | void>;

export interface MonitorOpts {
  baseUrl: string; token: string; accountId: string;
  onMessage: OnMessageCallback;
  abortSignal?: AbortSignal;
}

// 在长轮询循环内:
for (const full of list) {
  const reply = await opts.onMessage(full);
  if (reply) {
    await sendMessageWeixin({
      to: full.from_user_id!, text: reply,
      opts: { baseUrl, token, contextToken }
    });
  }
}
```

#### [MODIFY] `src/auth/accounts.ts`
- 内联 `normalizeAccountId`，移除 `openclaw/plugin-sdk/account-id` 导入
- Token 存储改为独立的 JSON 文件 (`~/.wechat-bot/accounts.json`)

#### [NEW] `src/index.ts` — 新导出面板

```typescript
export { startMonitor, type MonitorOpts, type OnMessageCallback } from './monitor/monitor.js';
export { loginWithQr } from './auth/login-qr.js';
export { sendMessageWeixin } from './messaging/send.js';
export { sendTyping } from './api/api.js';
export type { WeixinMessage, MessageItem } from './api/types.js';
```

### 1.2 验证
- `npm run typecheck` 无 OpenClaw 类型错误
- `npm test` 现有测试通过

---

## Phase 2: Bob-Agent HTTP API (SSE 流式)

**目标**：在 Rust 后端新增 axum HTTP 服务，暴露 SSE 流式聊天端点，并在完成时广播桌面端 UI 更新。

### 2.1 新增文件

#### [NEW] `src-tauri/src/http_api.rs`

```rust
#[derive(Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub conversation_id: Option<String>,  // 空 = 新建对话
    pub from_channel: Option<String>,     // "wechat" | "desktop"
    pub from_user: Option<String>,        // 微信用户 ID
}

// SSE 事件类型
// { "type": "text", "content": "..." }
// { "type": "done", "conversation_id": "...", "full_text": "..." }

async fn handle_chat_sse(
    State(app): State<AppHandle>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // 1. 查找或创建 conversation
    // 2. 加载历史消息，构建 messages 数组
    // 3. 调用 llm::stream_internal(OutputSink::Channel(tx), messages)
    // 4. 完成后保存 assistant 消息到 SQLite
    // 5. 【新增】通过 app.emit("remote:new-message", conv_id) 通知桌面端刷新
    // 6. 发送 done 事件
    todo!()
}

async fn get_recent_conversations() -> Json<Value> {
    // 查询最近 N 条会话，返回 [{id, title, updated_at}]
    todo!()
}

// 【新增】微信连接状态端点，供桌面端轮询
async fn wechat_status() -> Json<Value> {
    // 返回 Bridge 的连接状态（Bridge 启动时 POST 注册，Bob 保存状态）
    todo!()
}

pub fn create_router(app: AppHandle) -> Router {
    Router::new()
        .route("/v1/chat", post(handle_chat_sse))
        .route("/v1/conversations", get(get_recent_conversations))
        .route("/v1/wechat-status", get(wechat_status))
        .route("/v1/health", get(health))
        .with_state(app)
}
```

### 2.2 改造 `llm.rs` — 双通道输出

```rust
pub(crate) enum OutputSink {
    TauriEvent(AppHandle),
    Channel(tokio::sync::mpsc::Sender<SseChunk>),
}

async fn stream_internal(sink: OutputSink, messages: Vec<Value>) -> Value {
    // 原来的 app.emit(...) 改为 sink.send(...)
}
```

### 2.3 数据库扩展

```sql
-- 消息来源标注
ALTER TABLE messages ADD COLUMN from_channel TEXT DEFAULT 'desktop';
-- 'desktop' | 'wechat'

-- 会话标题（微信发起的会话默认取首条消息前 20 字）
-- conversations 表已有 title 字段，需在 http_api.rs 创建会话时自动填充
```

### 2.4 修改清单

| 文件 | 改动 |
|---|---|
| `Cargo.toml` | 新增 `axum = "0.8"` 依赖 |
| `lib.rs` | `setup` 中启动 HTTP server 后台任务 |
| `llm.rs` | 抽象 `OutputSink`，支持双通道输出 |
| `db.rs` | messages 表新增 `from_channel`；新增 `get_recent_conversations` 查询 |
| `http_api.rs` | 新文件：SSE 端点 + 会话列表 + 状态端点 |

### 2.5 桌面端 UI 适配（Vue）

- `ChatView.vue`：监听 `remote:new-message` 事件，触发时重新拉取消息列表
- 来自微信的消息 avatar 旁加微信绿色小图标（通过 `from_channel` 字段判断）
- 系统托盘/状态栏：轮询 `/v1/wechat-status`，显示"微信已连接/未连接"状态

---

## Phase 3: Bridge 微服务

**目标**：连接 wechat-bot-core 和 Bob HTTP API，处理指令路由和并发控制。

### 3.1 项目结构

```
wechat-bot-bridge/
├── package.json
├── tsconfig.json
├── data/
│   └── sessions.json       # 持久化 wxid → conversation_id 映射
├── src/
│   ├── index.ts            # 入口：启动微信监听与指令路由
│   ├── bob-client.ts       # Bob SSE 客户端
│   ├── session-mgr.ts      # Session 状态机（持久化版）
│   ├── message-queue.ts    # 【新增】每 wxid 的串行请求队列
│   ├── typing-manager.ts   # sendTyping 心跳管理
│   └── config.ts           # 配置
└── AGENTS.md
```

### 3.2 核心模块

#### `session-mgr.ts` — 持久化状态机

```typescript
// 内存 + 文件双写，Bridge 重启后从文件恢复
interface SessionState {
  convId: string | null;          // 当前绑定的 conversation_id
  state: 'chat' | 'selecting';   // 正常聊天 / 选单模式
  selectingExpiry?: number;       // 选单模式超时时间戳（TTL: 60s）
  pendingList?: ConversationMeta[]; // 缓存上一次 /chat 返回的列表
}

class SessionManager {
  private sessions: Record<string, SessionState> = {};
  private filePath = './data/sessions.json';

  constructor() { this.load(); }

  private load() { /* 从 sessions.json 读取恢复 */ }
  private save() { /* 每次写操作后异步写入文件 */ }

  setSelecting(wxid: string, list: ConversationMeta[]) {
    this.sessions[wxid] = {
      ...this.sessions[wxid],
      state: 'selecting',
      pendingList: list,
      selectingExpiry: Date.now() + 60_000, // 60 秒 TTL
    };
    this.save();
  }

  isSelecting(wxid: string): boolean {
    const s = this.sessions[wxid];
    if (!s || s.state !== 'selecting') return false;
    // 检查 TTL
    if (s.selectingExpiry && Date.now() > s.selectingExpiry) {
      this.cancelSelecting(wxid);
      return false;
    }
    return true;
  }

  cancelSelecting(wxid: string) {
    if (this.sessions[wxid]) {
      this.sessions[wxid].state = 'chat';
      this.save();
    }
  }

  bindSession(wxid: string, convId: string | null) {
    this.sessions[wxid] = { convId, state: 'chat' };
    this.save();
  }

  getConvId(wxid: string): string | null {
    return this.sessions[wxid]?.convId ?? null;
  }
}
```

#### `message-queue.ts` — 串行化并发请求

```typescript
// 防止同一用户快速连发导致消息乱序或 DB 竞争
class MessageQueue {
  private queues: Record<string, Promise<void>> = {};

  enqueue(wxid: string, task: () => Promise<void>): Promise<void> {
    const prev = this.queues[wxid] ?? Promise.resolve();
    const next = prev.then(task).catch(console.error);
    this.queues[wxid] = next;
    return next;
  }
}
export const msgQueue = new MessageQueue();
```

#### `index.ts` — 完整指令路由

```typescript
import { startMonitor, sendTyping } from '@gemini/wechat-bot-core';
import { chatWithBob, getRecentConversations } from './bob-client.js';
import { sessionMgr } from './session-mgr.js';
import { msgQueue } from './message-queue.js';

const COMMANDS: Record<string, string> = {
  '/help':  '可用指令：\n/chat — 切换/列出会话\n/new — 开启新会话\n/status — 查看当前会话\n/help — 显示此帮助',
  '/new':   null, // 特殊处理
  '/chat':  null, // 特殊处理
  '/list':  null, // /chat 的别名
  '/status': null, // 特殊处理
};

await startMonitor({
  ...config.weixin,
  onMessage: async (msg) => {
    const wxid = msg.from_user_id!;

    // 非文字消息降级处理
    const textItem = msg.item_list?.find(i => i.type === 1);
    if (!textItem) {
      const typeMap: Record<number, string> = {
        3: '图片', 34: '语音', 43: '视频', 49: '文件',
      };
      const msgType = msg.item_list?.[0]?.type;
      const typeName = typeMap[msgType] ?? '该类型消息';
      return `暂不支持接收${typeName}，请发送文字消息。`;
    }

    const text = textItem.text_item?.text?.trim();
    if (!text) return;

    // 串行化：同一用户的请求按顺序处理
    return new Promise((resolve) => {
      msgQueue.enqueue(wxid, async () => {
        resolve(await handleMessage(wxid, text));
      });
    });
  },
});

async function handleMessage(wxid: string, text: string): Promise<string> {
  // 1. /help
  if (text === '/help') return COMMANDS['/help'];

  // 2. /new — 立即开启新会话
  if (text === '/new') {
    sessionMgr.bindSession(wxid, null);
    return '✅ 已开启全新会话，接下来的消息将进入新对话。';
  }

  // 3. /status — 查看当前会话
  if (text === '/status') {
    const convId = sessionMgr.getConvId(wxid);
    return convId ? `当前会话：${convId.slice(0, 8)}…（发送 /chat 可切换）` : '当前无活跃会话（下条消息将新建）';
  }

  // 4. /chat 或 /list — 列出会话
  if (text === '/chat' || text === '/list') {
    const list = await getRecentConversations(5);
    if (list.length === 0) return '暂无历史会话，直接发消息开始吧。';
    sessionMgr.setSelecting(wxid, list);
    let reply = '请回复序号切换会话（60秒内有效）：\n';
    list.forEach((c, i) => reply += `[${i + 1}] ${c.title || '未命名对话'}\n`);
    reply += `[0] 开启全新会话`;
    return reply;
  }

  // 5. 选单模式中处理序号
  if (sessionMgr.isSelecting(wxid)) {
    if (/^[0-9]$/.test(text)) {
      const idx = parseInt(text, 10);
      if (idx === 0) {
        sessionMgr.bindSession(wxid, null);
        return '✅ 已开启全新会话。';
      }
      const list = sessionMgr.getPendingList(wxid);
      const target = list?.[idx - 1];
      if (!target) return `❌ 序号无效，请回复 1-${list?.length ?? 5} 之间的数字，或发送 /chat 重新列出。`;
      sessionMgr.bindSession(wxid, target.id);
      return `✅ 已切换至「${target.title || '未命名对话'}」，继续上下文吧。`;
    } else {
      // 输入不是序号 → 退出选单，继续当普通消息处理
      sessionMgr.cancelSelecting(wxid);
    }
  }

  // 6. 正常对话
  const activeConvId = sessionMgr.getConvId(wxid);
  const reply = await chatWithBob({
    message: text,
    conversationId: activeConvId ?? undefined,
    fromUser: wxid,
    onTyping: () => sendTyping({ /* ... */ }),
  });

  // 长文本分段发送 (微信单条上限 ~4000 字符)
  if (reply.length > 3800) {
    // monitor 层目前只支持返回单条，需要改造支持多条
    // 临时方案：截断并附提示
    return reply.slice(0, 3800) + '\n…（回复「继续」查看剩余内容）';
  }
  return reply;
}
```

### 3.3 验证
1. 启动 Bob-Agent → 启动 Bridge → 手机微信扫码
2. 微信发送「你好」→ 收到 Bob 回复，桌面端列表自动刷新出新消息
3. 微信发送 `/help` → 收到指令说明
4. 微信发送 `/chat` → 收到会话列表 → 回复 `1` → 成功切换
5. Bridge 重启后再发消息 → 自动继承之前的会话（不丢失绑定关系）
6. 桌面端能看到微信消息旁带有微信图标标记

---

## Phase 4: 部署

### 本地 All-in-One (开发用)
```bash
# 终端 1: Bob 桌面（HTTP API 随之启动在 3721）
cd bob-agent && npm run tauri dev

# 终端 2: Bridge
cd wechat-bot-bridge && npx tsx src/index.ts
```

### VPS 远程 (Phase 5 未来规划)
通过 Tailscale 内网穿透，Bridge 在 VPS 上连接本地 Bob 的 `:3721` 端口。

---

## 安全红线

> [!CAUTION]
> 1. HTTP API 仅绑定 `127.0.0.1`，**绝不暴露** `0.0.0.0`
> 2. 微信消息经腾讯服务器中转，**禁止传输** API Key、密码等敏感信息
> 3. Bridge 建议运行在隔离环境中，限制文件系统访问范围
> 4. `from_channel: "wechat"` 的请求**禁用** Tool Calling 中的 `write_file` 权限

---

## 已识别风险与解决方案

| 风险 | 严重程度 | 解决方案 |
|---|---|---|
| Bridge 重启丢失 Session 绑定 | 🔴 高 | `session-mgr.ts` 持久化到 `data/sessions.json` |
| 同一用户快速连发导致消息乱序 | 🔴 高 | `message-queue.ts` 串行化每个 wxid 的请求 |
| 选单模式永不超时 | 🟡 中 | `isSelecting()` 检查 60 秒 TTL |
| 非文字消息静默丢弃 | 🟡 中 | `onMessage` 入口检测 type，返回降级提示 |
| 微信会话在桌面端无标题 | 🟡 中 | 创建会话时取首条消息前 20 字作为标题 |
| 桌面端无法实时感知微信新消息 | 🟡 中 | HTTP API 完成后发 `remote:new-message` Tauri 事件 |
| 用户不知道有哪些指令 | 🟢 低 | `/help` 指令 |

---

## 工作量评估

| Phase | 预计工作量 | 关键难点 |
|---|---|---|
| Phase 1 | 3-4h | 移除 `channelRuntime` 依赖链 |
| Phase 2 | 5-7h | `OutputSink` 双通道 + SSE + Tauri 广播 + DB 扩展 |
| Phase 3 | 3-4h | 持久化状态机 + 并发队列 + 完整指令路由 |
| Phase 4 | 1h | 本地启动脚本 |
| **总计** | **~13h** | |
