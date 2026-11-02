# 节点统一认证维护与多 Harness 凭据恢复契约（`harness-auth`）

> **定位**：本文档沉淀 `bob-node` / `bob-runtime` 对接各外部 Agent / CLI 工具（DSH、AGY、Codex、Claude Code、Pi、Hermes、OpenClaw）时的**官方认证机制、确定性恢复流程与五项统一内部动作**。
> **状态**：2026-11-02 基于各工具官方文档与源码研究确立的知识基线；各节点具体接入须按设备实际安装版本做兼容探测。遵循 [ARCHITECTURE](../ARCHITECTURE.md)“凭据留在边缘、三个信任域分离”不变量。

---

## 1. 核心原则：手机只管连节点，工具凭据由节点确定性维护

1. **手机/客户端零工具凭据负担**：手机端与浏览器只保留对 Bob 节点的设备连接（mTLS / 设备 Bearer）；各 Harness 的启动 Token、会话 Cookie、API Key、OAuth 登录态一律留在执行节点本地，工具升级或 Token 轮换尽量由节点适配器自行消化。
2. **认证恢复是适配器的确定性程序，不是 Agent 临时技能**：例如 DSH 从受控输出或日志提取当前启动 Token 并交换 Cookie，必须写成 Rust 适配器（`bob-runtime`）中的确定性代码逻辑，**严禁依赖大模型 Agent 临时调用 Shell 技能去猜日志**。
3. **重新认证绝不等于自动重发**：任何工具完成凭据恢复后，对此前发送结果未知（`unknown_send`）的任务**必须先核对持久回执**，严禁因重新认证成功就盲目重放模型请求。

---

## 2. Bob 节点统一提供的五项内部动作（Adapter Lifecycle Contract）

所有挂载到 `bob-node` 的工具适配器统一实现以下 5 个标准内部动作：

| 序号 | 内部动作 | 职责与输入/输出契约 |
| :---: | :--- | :--- |
| **1** | **发现工具 (`discover_tool`)** | 探测目标宿主上的可执行程序、本机服务端口、实际版本号、运行系统用户（User）及隔离配置档（`profile`）。 |
| **2** | **检查认证 (`check_auth`)** | 使用零模型 Token 消耗的轻量探针核验当前凭据，精确返回五态之一：<br>`available`（可用） / `login_required`（需要登录） / `pairing_required`（需要配对） / `insufficient_scope`（权限不足） / `temporarily_unreachable`（暂时不可达，如进程未起或网络抖动）。 |
| **3** | **恢复认证 (`recover_auth`)** | 当检测到认证失效时，由对应适配器按确定性步骤自动恢复（如读取受控配置、用启动 Token 交换新 Cookie、或调用原版工具的托管刷新接口），成功后原子更新本地安全存储。 |
| **4** | **开始登录 (`begin_login`)** | **仅当自动恢复（动作 3）确认失败或首次未登录时**，向前端返回准确的官方登录或配对引导操作（如官方登录命令、设备码或配对入口），不展示含糊报错。 |
| **5** | **验证能力 (`verify_capabilities`)** | 在认证可用后，逐项实测并标记六大核心能力位：`list_sessions`（列会话）、`read_history`（读历史）、`resume_chat`（原会话续聊）、`stream_events`（真实流式推送）、`cancel_execution`（取消并确认终止）、`read_artifacts`（成果预览读取）。 |

---

## 3. 七大工具认证机制与 Bob 节点推荐接法总表

| 工具 | 官方认证机制 | Bob 节点推荐接法 | 失效后如何确定性处理 | 官方参考来源 |
| :--- | :--- | :--- | :--- | :--- |
| **DSH**<br>*(DeepSeek Harness)* | 启动 `token` 换取浏览器会话 `Cookie`（签名密钥持久保存）；另有独立程序接入握手协议 | 优先评估独立程序接入握手协议；兼容现有 `token` $ightarrow$ `Cookie` 交换通道 | **先检查现有 Cookie 会话是否仍有效**；确认认证失效（401/403）后，才从受控启动输出或指定日志范围获取当前启动 `token` 交换新 `Cookie` | [DSH 浏览器 Token 认证架构](https://github.com/deepseek-ai/deepseek-harness/blob/master/.agents/notes/implemented/architecture/2026-08-24-browser-token-authentication.md) |
| **AGY**<br>*(Antigravity)* | Headless 模式直接复用本机已缓存的官方登录凭据 | 在同一系统用户（User）下运行官方 `agy` CLI（或核验官方协议） | 未登录时 CLI 会明确退出报错；节点捕获后报告 `login_required`，交回官方交互登录流程，**严禁抠取凭据或刮屏套壳** | [AGY 官方 Headless 文档](https://antigravity.google/docs/cli/headless/) |
| **Codex**<br>*(OpenAI Codex)* | 官方 CLI / `app-server` 托管账户登录状态、完成通知与 Token 自动刷新 | 由本机原版工具管理认证（调用官方 CLI 或 `app-server`） | 使用官方账户状态检查与托管刷新机制；需要重新认证时触发官方登录流程，**避免 Bob 自行搬运其内部凭据** | [Codex 官方 app-server 文档](https://developers.openai.com/codex/app-server) |
| **Claude Code**<br>*(`claude`)* | 官方账号 OAuth 登录、`ANTHROPIC_API_KEY` 或云厂商凭据 | 调用本机原版 `claude` CLI，沿用该系统用户的认证环境 | 通过结构化 `loggedIn` 状态核验；失效时报告 `login_required` 交回官方 `claude` 登录流程处理 | [Claude Code 官方认证文档](https://code.claude.com/docs/en/authentication) |
| **Pi**<br>*(Coding Agent)* | 本机 `--mode rpc` 标准输入输出管道；底层模型供应商另有 API Key 或 `/login` 凭据 | 节点按需启动子进程 `pi --mode rpc`，通过 **JSONL 双向管道（stdio）** 通信，无需开放公网端口 | 让 Pi 自身管理供应商认证与存储（Bob 不复制其 OAuth 刷新逻辑）；当 RPC 报告缺少供应商授权时提示用户执行登录 | [Pi 官方 RPC 文档](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md)、[模型与认证说明](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md) |
| **Hermes**<br>*(Hermes Agent)* | 本机官方 API 服务使用 `API_SERVER_KEY`（Bearer Key），支持多 `profile` 隔离 | 作为**本机服务型适配器**直连其官方 HTTP/SSE 接口（含会话列表、历史、续聊、流式与能力发现） | 按 `profile` 严格隔离凭据（严禁跨 profile 混用）；失效时从该 `profile` 指定的配置来源重新解析 `API_SERVER_KEY` 并重认证 | [Hermes 官方 API 文档](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/api-server.md) |
| **OpenClaw** | 网关认证 = **设备身份 + 配对批准 + 权限范围（Scope）** + 设备令牌 | 作为**有持久设备身份的网关客户端**接入 | 持久化保存已获批的设备凭据；严格区分 **`token_invalid`（令牌错误/过期）**、**`pairing_revoked`（配对被撤销）** 与 **`insufficient_scope`（权限范围不足）**，绝不能把所有拒绝一概当作 Token 过期处理 | [OpenClaw 官方认证协议](https://github.com/openclaw/openclaw/blob/main/docs/gateway/protocol/auth.md) |

---

## 4. 关键工具实现深水区细则

### 4.1 DSH 认证认知修正与六步确定性恢复算法

> **重要认知修正**：不能认为“DSH 进程重启、启动 token 变了，旧会话就一定作废”。
> 在 DSH 官方实现中，启动 `token` 是每个 Host 进程启动时生成的一次性兑换码，通过入口 URL 交换为浏览器会话 `Cookie`（**启动 token 本身不能直接当 API Bearer Token 用**）。由于 DSH 服务端对 `Cookie` 的签名密钥是持久保存的，**即使 DSH 进程重启、生成了新的启动 token，已有的会话 Cookie 往往依然有效！**

因此，`bob-runtime::dsh_transport` 必须严格按以下 **6 步顺序** 维护与恢复认证：

1. **先验现有会话**：优先用本地已保存的会话凭据发起零模型消耗的轻量认证检查（如轻量列表/状态探针）。
2. **区分网络不可达与认证失效**：若探针表现为连接超时、拒绝连接或网关 502/503，判定为 `temporarily_unreachable`，**保留现有缓存与凭据不动**，走退避重试，绝不清空凭据。
3. **受控提取当前启动 Token**：仅当明确收到认证失效（如 HTTP 401/403 或重定向登录）时，才去获取当前启动 `token`：
   - 若 DSH 由 Bob 节点拉起托管：直接从受控启动子进程的标准输出捕获启动 URL/token（最可靠）；
   - 若 DSH 已作为外部独立服务运行：仅从预先配置好的受控日志路径/systemd journal 读取**最近一次进程启动头部范围**的固定格式行，**严禁在整份巨大日志中盲目正则抓取最后一个长字符串当 token**。
4. **兑换并原子更新**：用提取到的启动 `token` 调用入口交换接口获取新会话 `Cookie`，发一次轻量请求验证通过后，原子写入节点本地 `SecretStore`。
5. **恢复业务读取**：自动恢复会话目录拉取与历史正文读取。
6. **未知发送先对账**：若断线/重认证前存在 `unknown_send` 的待决发送请求，重认证后必须先查询回执/历史序号核对是否已接纳，**严禁因重新认证成功而自动重发消息**。

*(注：下一步在核对 X1T 等节点的 DSH 实际版本时，同步检查其是否已开放官方设计中的“独立 CLI/MCP 程序接入握手协议”；若当前版本已支持，优先切换至程序握手协议，比长期维护浏览器 Cookie 交换更稳健。)*

### 4.2 Pi 与 Hermes 的进程/服务形态差异

- **Pi（子进程管道型）**：
  - 采用 `pi --mode rpc` 启动，通过 `stdin`/`stdout` 的 JSONL 帧进行双向异步通信；
  - 优势是**完全不需要在节点上额外分配或监听任何 TCP 端口**，天然由 `bob-node` 进程沙箱管理生命周期；
  - 供应商密钥由 Pi 自己的 `/login`、配置文件或环境变量管理，Bob 只消费其 RPC 状态，不介入其内部 OAuth 刷新。
- **Hermes（本机 HTTP/SSE 服务型）**：
  - 官方已内建成熟的 HTTP Server（含 `/sessions`、历史查询、会话内追问、SSE 增量流式输出及能力发现端点），使用 `API_SERVER_KEY` 保护；
  - 必须以 `(host, port, profile)` 为联合键管理凭据——同一台机器上如果跑了多个 Hermes `profile`，各自拥有独立的配置源与 `API_SERVER_KEY`，适配器必须按 `profile` 寻址，严禁混用。

### 4.3 OpenClaw 的三类拒绝状态机

接入 OpenClaw 网关时，适配器状态机必须解析握手与响应中的具体拒绝错误码：
- `TokenInvalid / Expired` $ightarrow$ 尝试使用已配对身份刷新或读取更新后的网关 Token；
- `PairingRevoked / Unpaired` $ightarrow$ 标记 `pairing_required`，停止后台自动重试，向前端展示重新配对流程；
- `InsufficientScope / Forbidden` $ightarrow$ 标记 `insufficient_scope`，明确告知用户当前设备角色缺少哪项网关权限（如只读设备尝试发起执行），不诱导用户反复无效重填 Token。

---

## 5. 推荐开发推进顺序

在 `Phase B（多节点 CLI 有序合流与常驻触达）` 中，节点认证维护与适配器的落地顺序固定为：

1. **第一棒：补齐 `DSH` 自动认证恢复闭环**（落实上述 6 步算法 + 核查 X1T 实际版本是否支持独立程序握手协议）；
2. **第二棒：完成 `AGY` 认证状态核验与历史接入**（同用户 Headless 缓存认证检测、未登录诚实报告 `login_required`、完成历史读取）；
3. **第三棒：接通 `Codex` 与 `Claude Code` 统一设备入口**（复用本机原版 CLI / `app-server` 官方认证状态与受控 `tmux` 保活续接）；
4. **第四棒：扩展独立适配器 `Pi`（`--mode rpc` JSONL 管道）、`Hermes`（按 profile 隔离的 `API_SERVER_KEY` + SSE 流式）、`OpenClaw`（设备身份 + 配对 + Scope 状态机）**。
