# Evidence — 懒式响应（Lazy Dormant Sync）与非即时发送可靠性审计（2026-10-28）

- **审计对象**：`legacy/v0.9`（提交 `a4ff5fd1cb57549b10b2be2223833da982b18e78`，v0.9.30）与 `1.0.1` 统一架构基线。
- **审计问题**：
  1. 现有代码是否已经使用懒式响应（Lazy Dormant Sync）？
  2. 在非即时模式（懒式休眠、唤醒重连中、跨网隧道闪断或 Home 暂不可达）下，用户发送信息是否会失败？

---

## 1 现状核查：哪些已经做了懒式响应？

在 `legacy/v0.9` 最新提交 `a4ff5fd`（`[bob-agent] Implement peer-to-peer link smoothness and lazy dormant sync for long-haul remote sessions`）中，**Work 远程 Agent 面板已经实现了初版“懒式休眠同步（`linkDormant`）”**：

- **实现位置**：
  - `src/work/remote-agent-state.js`（第 26–49、194–201 行）
  - `src/work/remote-agent-service.js`（第 26–42、53–56、98–105、379–393 行）
  - `src/components/work/RemoteAgentPanel.vue`（第 555–586 行）
- **已实现机制**：
  1. 当远程会话处于静止状态（`idle` / `completed` / `stopped` / `blocked` / `failed`）、无待确认交互，且底层长途 WebSocket 因空闲断开（`REMOTE_DISCONNECTED` / `REMOTE_TIMEOUT` / `REMOTE_NETWORK`）时，状态机进入 `linkDormant = true`（`linkSmoothness = 'long-haul'`），停止后台轮询与指数退避重连风暴，并隐藏瞬态断连报错条。
  2. `RemoteAgentPanel.vue` 的 `canSend` 允许在 `state.linkDormant === true` 时保持发送按钮可用。
  3. 用户点击发送时，`service.send(text, id)` 检测到 `state.linkDormant` 会触发按需重连（`connectProfile(..., { preserveView: true, useCachedCatalog: true })`），重连成功后再调用 `controller.send(text, id)`。

---

## 2 风险与漏洞核查：为什么在非即时模式下发送仍可能失败？

经逐行审查 `legacy/v0.9` 的前后端链路，发现当前实现中仍存在 **4 处会导致“非即时模式下发送失败”的真实漏洞**：

### 漏洞 A：底层 Socket 刚断开但前端尚未进入 `linkDormant` 时，发送未出网即被误判为 `REMOTE_OUTCOME_UNKNOWN` 并锁死会话

- **位置**：`src-tauri/src/remote_agent/dsh.rs` 第 405–408 行与第 540–562 行。
- **原因**：
  - 静止状态下前端轮询间隔为 `1500ms`。若 WebSocket 在两次轮询之间断开，Rust 侧 `self.alive` 已变为 `false`，但前端 `state.linkDormant` 仍为 `false`、`state.live.ready` 仍为 `true`。
  - 此时用户点击发送，`service.send()` 不会走唤醒重连，而是直接调用 `Dsh::action("send")` → `self.rpc("session/prompt", args)`。
  - `self.rpc` 第一行检查 `if !self.alive.load(Ordering::Acquire)` 立即返回 `Err("REMOTE_DISCONNECTED")`——**此时 HTTP 请求根本没有发出（0 字节出网）**。
  - 但 `Dsh::action` 第 556 行用 `_ =>` 通配捕获了所有错误，无差别将该会话写入 `state.uncertain` 并返回 `REMOTE_OUTCOME_UNKNOWN`，导致前端弹出“结果未知需人工核对（`needsReview`）”并直接禁用发送框，用户发送失败。

### 漏洞 B：唤醒重连竞态（`connecting === true`）与旧 `watchId` 残留（`REMOTE_STALE`）

- **位置**：`src/work/remote-agent-service.js` 第 381–392 行、`src/work/remote-agent-state.js` 第 193–201 行。
- **原因**：
  1. `service.send()` 的按需唤醒条件写了 `&& !connecting`。如果用户点击发送时恰好后台正在执行 `resume()` 或 `refresh()`（`connecting === true`），`service.send()` 会直接跳过等待，立即调用 `controller.send()`，而此时 `live?.ready` 为 `false`，`controller.send()` 直接返回 `false`，发送失败。
  2. 唤醒时 `connectProfile` 成功后，第 389 行执行 `await controller.select(currentSession, { preserveView: true })`。若 `remoteAgentWatch` 在长途隧道冷启动时发生一次瞬态超时，`controller.select` 捕获错误后仅设置 `linkDormant = true` 而不抛错返回，此时 `live.watchId` 仍然是**旧连接的 `watchId`**。紧接着第 392 行调用 `controller.send()`，Rust 侧校验 `state.view.watch_id != watch_id` 直接报错 `REMOTE_STALE`，发送失败。
  3. 若唤醒的第一次重连因移动网络切换耗时失败，`service.send()` 直接返回 `false`，没有本地持久化发件箱（Durable Outbox）接管排队。

### 漏洞 C：新创建的空会话（`messages.length === 0`）无法进入懒式休眠

- **位置**：`src/work/remote-agent-state.js` 第 27 行、`src/work/remote-agent-service.js` 第 29 行。
- **原因**：`canEnterLazyDormant` 要求 `previous.messages.length > 0`。用户刚新建或打开一个尚无消息的会话时，若长途连接空闲断开，无法进入 `linkDormant`，会直接报错并禁用发送按钮。

### 漏洞 D：Chat 视图（`@pc` 远程电脑执行）在非即时/离线状态下直接弹窗拦截，拒绝发送

- **位置**：`src/views/ChatView.vue` 第 1792–1829 行。
- **原因**：当执行目标选为配对 PC（X1C）时，`sendMessage()` 先同步调用 `checkDeviceOnline(targetPcId)`；一旦 PC 处于睡眠、中继懒式未建连或探针超时，直接弹窗 `⚠️ 目标设备离线` 并 `return` 中止发送，未写入本地 Outbox 队列。

---

## 3 `1.0.1` 统一架构的根治契约（已归并入 `BOB-D-008` 与 `ARCHITECTURE.md`）

为确保在 `1.0.1` 中**“任何非即时模式下发送信息绝对不会因为连接状态而失败”**，在架构与实现层强制执行以下四条不变式：

1. **本地落盘即发送成功（Local-First Durable Outbox）**：
   - 用户点击发送的唯一同步前置条件是**写入本机 SQLite `intent_outbox` 成功**（生成 `request_id` 与幂等键，状态 `local_saved` → `queued_for_wakeup`）。
   - 写入本机成功后，输入框立即清空、消息立即上屏（显示状态徽标：`已入队 · 正在唤醒连接` 或 `等待 X1C 上线投递`），**永远不因网络处于非即时模式而让发送动作失败**。
2. **输入框聚焦/打字静默预热（Speculative Pre-Warm）+ 单飞唤醒等待（Single-Flight Wakeup Await）**：
   - **打字时间差预热**：用户点击输入框（`@focus`）或输入首字（`@input`，带冷却节流）时，若链路处于 `linkDormant` 或距上次存活确认超过阈值，后台静默发起预热建连与 `session/watch` 刷新（不设 `busy`、不弹错误、保留当前视图）。用户通常需要 3～15 秒打字，而隧道唤醒仅需 0.5～2 秒，打完字点击发送时连接已就绪，体验与常驻长连接完全一致。
   - **单飞 Promise 复用**：预热与发送共享同一个 `inFlightWakeupPromise`。若用户秒点发送而预热仍在握手，发送管线直接 `await` 该在途 Promise，拿到新连接的 `watchId` 后立即投递；若预热后用户放弃输入未发送，空闲超时后自动回落 `linkDormant`。
   - **Outbox 退避兜底**：若因极端网络切换导致首次唤醒未赶上，消息已在 `intent_outbox` 中排队，后台自动退避重试出队，无需用户重发。
3. **严格区分 `not_dispatched`（未出网）与 `unknown_send`（已出网未确认）**：
   - 凡是在本地连接检查（`!alive`）、唤醒建连、HTTP 建连阶段失败（请求体尚未写入 socket），一律标记为 `not_dispatched`，保留在 `intent_outbox` 中自动重连重发，**严禁打入 `REMOTE_OUTCOME_UNKNOWN` 锁死会话**。
   - 只有请求字节已发出、等待响应中途断连，才标记为 `unknown_send`；重连后先按 `request_id` 自动对账，对端未收到则自动补发，对端已收到则直接绑定执行流。
4. **空会话同样支持懒式休眠**：
   - 只要会话已成功建立过初始快照（哪怕 `messages.length === 0`）且处于静止阶段，空闲断连均平滑进入 `linkDormant`。
