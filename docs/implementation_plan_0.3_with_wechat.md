# Bob-Agent × 微信接入开发文档 (V4 — Rust 原生版)

## 用户决策定稿（完整）

| # | 问题 | 决策 |
|---|---|---|
| 1 | 流式 vs 一次性回复 | **流式**：先发 `sendTyping` 心跳，Bob 回复后 `sendMessage`。长文自动截断（3800字）并提示「继续」。|
| 2 | Tool Calling | **支持**：Bob 完整执行 tool calling 循环（最多 5 轮），等待期间每 4 秒发 `sendTyping`。|
| 3 | 对话跨端共享 | **共享**：微信对话存入 Bob 同一 SQLite 库，桌面端可见。|
| 4 | 会话管理 | `/chat` 列出最近 5 条，60 秒内回复序号切换；`/new` 新建；`/status` 查当前；`/help` 显示帮助。|
| 5 | 非文字消息 | **降级**：语音→「请用语音转文字后发文字」；图片/文件→「暂不支持此类型」。|
| 6 | Session 持久化 | **持久化到 AppData**（bob-agent 数据目录），进程重启后自动恢复 wxid↔convId 绑定。|
| 7 | **实现方式** | **Rust 原生**（Phase 5）：微信协议层内嵌 bob-agent，零 Node.js 依赖，发布包 ≤ 12 MB。|
| 8 | **http_api.rs** | **保留**：作为外部 API 接口，供未来其他工具调用。|
| 9 | **QR 登录入口** | Onboarding 第四页 + 设置页开关，详见 Phase 5 UI 规格。|
| 10 | **选单 TTL** | 60 秒。|

---

## 总体架构

### Phase 1~4（已完成）：Node.js Bridge 方案

```
微信 App ◄──长轮询──► wechat-bot-bridge (Node.js) ◄──SSE──► Bob-Agent (Rust/Tauri)
```

### Phase 5（当前目标）：Rust 原生方案

```
┌──────────┐   ilink 长轮询   ┌──────────────────────────────────────┐
│ 微信 App │◄───────────────►│           bob-agent.exe              │
│  (手机)  │  腾讯 ilink CDN  │                                      │
└──────────┘                  │  wechat::monitor  (tokio::spawn)     │
                              │    └── wechat::commands  (/help…)    │
                              │        └── wechat::msg_queue         │
                              │            └── llm::stream_chat      │
                              │                └── db (SQLite)       │
                              │  http_api (127.0.0.1:3721, 保留)     │
                              └──────────────────────────────────────┘
```

**优势**：单一进程，零 Node.js，安装包从 85 MB → **≤ 12 MB**

### 项目拓扑

| 项目 | 状态 | 语言 | 职责 |
|---|---|---|---|
| `wechat-bot-core` | ✅ Phase 1 完成 | TypeScript | 微信协议底层（过渡期参考实现）|
| `wechat-bot-bridge` | ✅ Phase 3 完成 | TypeScript | Node.js 完整版桥接（过渡期可用）|
| `bob-agent/src/wechat/` | 🔄 Phase 5 目标 | **Rust** | 原生内嵌微信网关（最终形态）|

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
| QR 码过期用户无感知（Phase 5） | 🟡 中 | 设置页弹窗显示倒计时进度条 + 刷新按钮 |

---

## 工作量评估（完整）

| Phase | 预计工作量 | 状态 | 关键难点 |
|---|---|---|---|
| Phase 1 — core 剥离 | 3-4h | ✅ 完成 | 移除 OpenClaw 依赖链 |
| Phase 2 — Bob HTTP API | 5-7h | ✅ 完成 | SSE + Tauri 事件桥接 + DB 扩展 |
| Phase 3 — Bridge | 3-4h | ✅ 完成 | 持久化状态机 + 队列 + 指令路由 |
| Phase 4 — 部署 | 1h | ✅ 完成 | 本地启动脚本 |
| **Phase 5 — Rust 原生化** | **9 天** | 🔄 规划中 | QR 登录 UI + Onboarding 第四页 |

---

## Phase 5: Rust 原生化实施规格

### 5.1 新增模块文件结构

```
bob-agent/src-tauri/src/
├── lib.rs                    ← 新增 mod wechat + 注册 5 个 Tauri 命令
└── wechat/
    ├── mod.rs                ← 模块入口 + 共享状态（Arc<WechatState>）
    ├── types.rs              ← 协议结构体（WeixinMessage / SendMessageReq 等）
    ├── api.rs                ← ilink HTTP 调用层（getUpdates / sendMessage / sendTyping）
    ├── accounts.rs           ← 账号凭证持久化（AppData/bob-agent/wechat/）
    ├── monitor.rs            ← 长轮询主循环（tokio::spawn + 断线重试）
    ├── session_mgr.rs        ← wxid ↔ convId 状态机 + 60s 选单 TTL
    ├── msg_queue.rs          ← 每 wxid 串行任务队列
    ├── commands.rs           ← UX 指令路由（/help /new /chat /status）
    └── login_qr.rs           ← QR 登录流程 + emit wechat:qr-code 事件
```

### 5.2 Tauri IPC 接口

| 命令 / 事件 | 方向 | 说明 |
|------------|------|------|
| `wechat_get_status` | 前→Rust | 返回 `{connected, accountId, sessionCount}` |
| `wechat_start_login` | 前→Rust | 启动 QR 登录流程，触发 `wechat:qr-code` 事件 |
| `wechat_start_monitor` | 前→Rust | 开始长轮询监听 |
| `wechat_stop_monitor` | 前→Rust | 停止监听 + 发送 notifyStop |
| `wechat_get_sessions` | 前→Rust | 返回 wxid→convId 映射列表 |
| `wechat:qr-code` 事件 | Rust→前 | `{data_uri: String, expires_at: i64}` |
| `wechat:status` 事件 | Rust→前 | `{connected: bool, accountId: String}` |
| `remote:new-message` 事件 | Rust→前 | 已有（Phase 2 实现）|

### 5.3 UX 指令体系

| 微信输入 | 行为 |
|----------|------|
| `/help` | 回复帮助文本（列出所有指令）|
| `/new` | 新建会话，清空 wxid 绑定，回复确认 |
| `/chat` | 回复最近 5 条会话列表，60s 内数字选择切换 |
| `/chat 2` | 直接切换到列表中第 2 条会话 |
| `/status` | 回复当前会话 ID 前 8 位 + 消息计数 |
| 语音消息 | 「请使用微信语音转文字功能后发送文字」|
| 图片 / 文件 | 「暂不支持此消息类型，请发送文字」|
| 普通文字 | 进入串行队列 → LLM 推理 → sendTyping 心跳 → 回复 |

**sendTyping 心跳**：LLM 推理期间每 4 秒自动发送，完成后停止。

### 5.4 微信网关 UI 规格

#### A. Onboarding 第四页「连接微信」

```
┌─────────────────────────────────────────────────┐
│                                                 │
│   🔗  连接微信                                  │
│                                                 │
│   让 Bob 通过微信与你随时沟通                    │
│                                                 │
│  ┌─────────────────────────────────────────┐   │
│  │  微信机器人连接                    ○──●  │   │  ← Toggle（默认关闭）
│  └─────────────────────────────────────────┘   │
│                                                 │
│  ┌──────────── 二维码区域（fade-in）─────────┐  │
│  │                                           │  │  ← Toggle 开启后渐显
│  │            [180×180 QR Code]              │  │
│  │                                           │  │
│  │    用手机微信扫描，确认登录               │  │
│  └───────────────────────────────────────────┘  │
│                                                 │
│                              🚀  →（右下角）   │  ← 扫码成功后高亮可点
└─────────────────────────────────────────────────┘
```

**状态机**：

| 状态 | UI 表现 |
|------|---------|
| 默认 | Toggle 关闭，二维码区域不渲染 |
| Toggle 打开 | 调用 `wechat_start_login`，二维码区 fade-in（0.5s），显示 QR |
| 扫码成功 | 收到 `wechat:status {connected:true}`，二维码替换为 ✅ 已连接 + 账号 ID |
| 🚀 高亮 | 可点击跳转到主界面，同时 `wechat_start_monitor` 开始监听 |

#### B. 设置页「微信网关」区块（非首次）

如果 Onboarding 时未开启，可通过**设置页**的开关激活，开关打开时弹出晨报式对话框：

```
╔═══════════════════════════════════════════════╗
║  🔗  微信网关                         ●──○   ║  ← 已连接态
║  账号：xxxxx@im.bot   活跃会话：3 个          ║
╚═══════════════════════════════════════════════╝

// 已断开或首次，点击开关 → 弹出对话框：

┌──────────────────────────────────────────────┐
│  🌅  连接微信                                │
│  ────────────────────────────────────────    │
│                                              │
│           [180×180 QR Code]                  │
│                                              │
│   用手机微信扫描，确认登录                    │
│                                              │
│   二维码有效期：120 秒                       │
│   ████████████████░░░░  倒计时进度条          │
│                                              │
│                    [取消]   [刷新二维码]      │
└──────────────────────────────────────────────┘
```

### 5.5 新增 Cargo 依赖

```toml
# Cargo.toml 新增（其余依赖 bob-agent 已有）
qrcode = "0.14"   # QR 码生成 → PNG data-URI  (+50 KB)
base64  = "0.22"  # base64 编解码              (+20 KB)
# reqwest / tokio / serde / serde_json / log / uuid ← 已有
```

**release binary 净增量**：约 **+400~600 KB**（发布包总大小 ≤ 12 MB）

### 5.6 实施顺序

| Day | 任务 | 产出文件 |
|-----|------|---------|
| 1 | `wechat/types.rs` 协议 structs | types.rs |
| 2 | `wechat/api.rs` HTTP 调用层 | api.rs |
| 3 | `wechat/accounts.rs` + `wechat/monitor.rs` | accounts.rs + monitor.rs |
| 4 | `wechat/session_mgr.rs` + `wechat/msg_queue.rs` | 状态机 + 队列 |
| 5 | `wechat/commands.rs` 指令路由 | commands.rs |
| 6 | `wechat/login_qr.rs` QR 登录 | login_qr.rs |
| 7 | `wechat/mod.rs` 装配 + `lib.rs` 注册命令 | mod.rs + lib.rs |
| 8 | 前端：Onboarding 第四页 + 设置区块 + tauri-bridge.js | Vue + JS |
| 9 | 集成测试 + e2e 验证 | — |
