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
| **DSH**<br>*(DeepSeek Harness)* | 启动 `token` 换取浏览器会话 `Cookie`（签名密钥持久保存）；另有独立程序接入握手协议 | 优先评估独立程序接入握手协议；兼容现有 `token` $
ightarrow$ `Cookie` 交换通道 | **先检查现有 Cookie 会话是否仍有效**；确认认证失效（401/403）后，才从受控启动输出或指定日志范围获取当前启动 `token` 交换新 `Cookie` | [DSH 浏览器 Token 认证架构](https://github.com/deepseek-ai/deepseek-harness/blob/master/.agents/notes/implemented/architecture/2026-08-24-browser-token-authentication.md) |
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
- `TokenInvalid / Expired` $
ightarrow$ 尝试使用已配对身份刷新或读取更新后的网关 Token；
- `PairingRevoked / Unpaired` $
ightarrow$ 标记 `pairing_required`，停止后台自动重试，向前端展示重新配对流程；
- `InsufficientScope / Forbidden` $
ightarrow$ 标记 `insufficient_scope`，明确告知用户当前设备角色缺少哪项网关权限（如只读设备尝试发起执行），不诱导用户反复无效重填 Token。

---

## 5. 推荐开发推进顺序

在 `Phase B（多节点 CLI 有序合流与常驻触达）` 中，节点认证维护与适配器的落地顺序固定为：

1. **第一棒：补齐 `DSH` 自动认证恢复闭环**（落实上述 6 步算法 + 核查 X1T 实际版本是否支持独立程序握手协议）；
2. **第二棒：完成 `AGY` 认证状态核验与历史接入**（同用户 Headless 缓存认证检测、未登录诚实报告 `login_required`、完成历史读取）；
3. **第三棒：接通 `Codex` 与 `Claude Code` 统一设备入口**（复用本机原版工具认证与持久程序接口；`tmux` 仅作必要的兼容保活）；
4. **第四棒：扩展独立适配器 `Pi`（`--mode rpc` JSONL 管道）、`Hermes`（按 profile 隔离的 `API_SERVER_KEY` + SSE 流式）、`OpenClaw`（设备身份 + 配对 + Scope 状态机）**。

## 6. 标准适配组件：原生常驻优先与真实流式

2026-11-04 用户确认的设计，按 BOB-D-013 生效；本节是组件协议的唯一正文，不表示所有适配器已实现或部署。

### 6.1 接入选择与官方能力依据

顺序：复用已运行、支持机器调用的官方服务 → 官方持久子进程协议 → 受监管的原生命令/续接 → 必要时使用 tmux 兼容保活。官方 Remote Control 仅供厂商客户端使用时，保留为“在官方界面打开”；它不是 Bob 自动可用的程序接口。服务常驻、会话持久化和机器不休眠是三个不同条件。

| 工具 | 首选接入 | 流式依据与待核对边界 |
|---|---|---|
| DSH | 复用现有 Host 的认证会话接口；SDK/ACP 作为按版本探测的候选，不为接入另启争用同一会话的服务 | 官方架构定义实时 assistant-stream 与 Web Session-follow；临时文本增量和持久 SessionEvent 分开，断流须用已提交历史补齐。当前节点协议与 Bob 到 UI 链路仍须实测 |
| AGY | `agy acp` 或同一进程的双向 stream-json；原生远控 daemon 用于官方远控入口 | ACP/stream-json 为公开程序接口；正文 text_delta、回合 result 可标准化。历史枚举、旧会话恢复、审批与取消逐项探测，不能由 ACP 名称推断全部支持 |
| Codex | `codex app-server`，优先本机 stdio；Bob 提供设备网络边界 | 原生会话、审批及增量事件；WebSocket 为实验性，不能直接当公网服务。具体 schema 以安装版本生成/核对，认证适用范围单独检查 |
| Claude Code | 官方程序接口/Agent SDK 或受监管结构化 CLI 续接；Remote Control 为官方备用入口 | 核验文本增量、审批和原 ID 恢复；官方远控在线不等于 Bob 已控制该进程，不同时恢复同一活动会话 |
| Pi | `pi --mode rpc` 持久子进程、JSONL stdin/stdout | 官方 RPC 有 message_update/text_delta。prompt 接受回执不是完成；按该版本终态协议判定，当前文档以 agent_settled 区别低层 agent_end。正常业务启用持久 session，不照抄示例的 --no-session |
| Hermes / OpenClaw | 沿 §3 的本机 HTTP/SSE 或网关适配 | 后续逐版本核验；不能沿用静态 supports_stream=true 宣称已接通 |

官方来源（核查日期 2026-11-04；上游主分支不等于节点安装版本）：[DSH 架构](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md)、[AGY ACP](https://antigravity.google/docs/cli/acp/)、[AGY 持续流](https://antigravity.google/docs/cli/headless/)、[AGY 远控](https://antigravity.google/docs/remote-control/)、[Codex app-server](https://developers.openai.com/codex/app-server)、[Claude 程序接口](https://code.claude.com/docs/en/headless)、[Pi RPC](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md)。

### 6.2 组件职责与契约

这些是待接入现有 runtime/node 的逻辑组件，不是新增服务或另一套任务账本：

| 组件 | 标准职责 |
|---|---|
| AdapterRegistry | 根据工具、版本、OS 用户/profile 选择兼容驱动；配置命令/端点，不硬编码用户设备、域名或目录；未知版本先探测 |
| HarnessAdapter | 保留 §2 五项认证动作；提供 list_sessions、read_history(cursor, limit)、open/resume_session、send(request_id)、subscribe(cursor)、approve、cancel、read_artifact；不支持的动作显式返回 unsupported |
| ProcessSupervisor | 管理节点自己启动的进程、并发/空闲回收、退出和重启对账；外部服务仅连接，不擅自重启；有在途/待审批/未知回执时不得按空闲释放 |
| EventNormalizer | 将原协议映射为统一事件及错误，同时保留原生身份/序号；stdout 只作协议，stderr 有界脱敏；不解析终端画面 |
| SessionDirectory / ReadCache | 沿用已有目录、分页和有界缓存；先读缓存后核对；保留 project/device/workspace/harness/profile/native_session_id，不复制权威历史 |

能力报告必须分三层：`documented_support`（来源/日期/兼容版本）、`observed_support`（supported/unsupported/unknown、探针/时间/失败原因）、`bob_integration`（unwired/candidate/verified）。认证、可达性、授权范围独立报告；有配置、存在二进制、官方宣称支持都不能直接变成当前可用。当前实现兼容值 `probe_only` 表示只完成上游探测，`unverified` 表示未验，均不等于 `verified`；旧布尔 `supports_*` 未验时为 null。只读状态检查不调用模型；有副作用的能力验证须在既有明确授权内执行。

### 6.3 统一流式事件与恢复

AGY ACP源事件恢复必须匹配持久请求的原生session与严格整数acp_request_id；session/prompt_result额外持久化规范session_id，不信任缺身份的旧结果或其他请求结果。正文源片段独立累积，覆盖源事件已落盘而展示投影未落盘的崩溃窗口。只有官方end_turn/cancelled可补终态；没有官方结果仍unknown，不重发。

旧轮次仅找到结束摘要而完整条目读取失败时，保持已确认终态并持久化recovery_pending；后续有界补读成功才清除，不再生成第二次终态事件。该状态属于恢复检查点，不允许迟到普通事件重开结束的请求。

- 事件信封包含 schema_version、device_id、adapter/profile、native_session_id、Bob request_id/执行引用（可空）、原生 turn/message/item ID（不可得时可空）、Bob 持久 event_id/sequence、原生 cursor（如支持）、observed_at、kind 与有界 payload。关联不明时保持未知，不把“最新回复”直接归到本次请求。
- 统一 kind：accepted、running、text_delta、message_snapshot、tool_status、approval_required、artifact_available、completed、failed、cancelled。完成、取消确认和消息快照不同；停止请求不等于目标已停止。
- UI 以真实事件呈现发送中、已接收、处理中、回复中、完成；审批独立可操作。只有上游正在输出可展示文本才追加 text_delta；整包接口显示处理中后一次呈现，不伪造打字动画，不索取内部思维链。
- 先订阅再发送，先落持久回执再向用户确认；事件重复去重、迟到隔离；snapshot 替换同一消息已显示增量，不能追加一份完整回复。SDK/管道分帧处理 UTF-8 跨块、JSONL 长帧和背压。
- 浏览器掉线不停止节点任务。客户端用 Bob 游标补事件；上游不可重放时，用原生 ID 的有界持久历史核对并标记增量缺口。不能将暂存片段冒充源已提交历史，不能因重连重发 prompt。
- 沿用阅读缓存容量/条数/TTL 上限；事件日志另设有界保留窗口，过期游标返回 resync_required；未决回执、审批及成果身份不得随阅读缓存淘汰。凭据不进事件或缓存。
- 同会话发送串行，同工作区写权沿用既有租约；厂商原生窗口不受 Bob 租约控制时，能力报告标明接管限制并核对活动状态，不宣称全局互斥。

### 6.4 面向用户的统一方式

设置只暴露添加设备（地址/扫码）、节点认证、工具列表及明确的“可使用/需要登录/未安装/暂不可达/功能未接通”状态；内部端口、RPC、tmux 和流协议置于诊断。已授权工具可自动发现，登录/安装仅按需引导，不自动授予新工具执行权。新工具只增加适配器和兼容测试，不新增一套聊天页面。

### 6.5 当前候选实现及未完成边界

`agy_index_probe.py` 的原生执行分支保留固定 session ID：Claude `--resume` + stream-json，AGY `--conversation` + stream-json，Codex app-server thread/resume/turn/start，Pi RPC 指定原会话文件并 get_state 校验身份。恢复失败禁止偷偷新建会话。节点私有 SQLite 存请求幂等和增量事件，网页 app_native_events 按游标补读；这是传输回执，不冒充内核 Task/Execution。

终态必须结合原生结束原因：Pi agent_settled 不能覆盖此前 length/error/aborted，message_end 仅处理 assistant，空回复不能报告完成；上游中断、超时或进程消失保持 unknown，阻止自动重发。现有候选限制为只读验证，不代表任意工具修改文件获得授权。

当前正文/终态事件已增加全局有界日志及恢复投影：最多20,000条/16MiB载荷/7天，快照最多256个/64MiB/7天；这是逻辑保留上限，SQLite物理文件与WAL不承诺立即缩小。每页100事件/1.5MiB，epoch变化、游标落后保留窗口或领先服务器时返回reset+替换快照+history_required；一次快照最多32项/1.5MiB，超出明确snapshot_incomplete，不冒称完整。未决/unknown发送回执不随阅读缓存删除，过期快照仍可由请求回执与原生历史核对。终态事件与最终回执互相修复崩溃窗口；迟到事件不能重开已确认终态。外部Codex重连只按官方原轮次确认状态，无法确认时标unknown，不能当新回复追加或继续转圈。此机制尚不等于审批、工具过程、成果身份等所有事件类型的恢复；跨轮常驻监管和各工具外部订阅仍待接。DSH 现场协议需要 assistantStream=true 才推 assistant-stream(start/chunk/end)，text-delta.text 为正文；旧 snapshot reader 不等于流式订阅。实际通过与失败分工具见 CURRENT_STAGE §3.6 和 2026-11-05 evidence，不把官方支持写成产品已接通。

### 6.6 原生会话新建与首次落盘

2026-11-09 核对官方文档及本地参考源码。侧栏和主助理可复用同一个创建请求，适配器分别核对工作区、原生会话身份和持久状态。下表是接口研究，目标节点的实际版本和重启恢复仍需逐项验收。

| 工具 | 创建与续接 | 持久化验收 |
| --- | --- | --- |
| Claude Code | 以目标工作区启动 `claude --session-id <UUID>`；非交互首轮用 `-p` 和既有 stream-json 流，后续用 `--resume <UUID>`。 | UUID 可在启动前记入请求。空白启动是否写入可重启恢复的原生记录，须在已安装版本核对。首次发送仍使用用户原文。[官方 CLI](https://code.claude.com/docs/en/cli-reference) |
| Codex | 复用 app-server，先 `thread/start`，再用返回的 `thread.id` 发 `turn/start` 或重连 `thread/resume`。 | 本地参考版本的测试显示空白线程未落盘，当前上游测试则覆盖归档空白线程时落盘。两者都不能替代目标安装版本对空白会话可发现性和重启恢复的验证。[官方 app-server](https://developers.openai.com/codex/app-server/)、[上游空白线程测试](https://github.com/openai/codex/blob/main/codex-rs/app-server/tests/suite/v2/thread_archive.rs) |
| AGY | 复用 `agy acp`，协商完成后通过 `session/new` 提交 `cwd` 和 `mcpServers: []`，记录返回的 `sessionId`；发送走 `session/prompt`，恢复按协商结果走 `session/load` 或 `session/resume`。 | ACP 创建回执证明本进程拥有会话。需验证目标版本在未发送首条消息时能否重新列出并加载同一个 ID。[Google ACP](https://antigravity.google/docs/cli/acp)、[ACP v1 会话规范](https://agentclientprotocol.com/protocol/v1/session-setup) |
| Pi | 在目标工作区以可持久模式启动 `pi --mode rpc`，使用 `new_session` 后查 `get_state`，验证 `cancelled`、`sessionId` 和 `sessionFile`。续接走现有 `--session <原文件>`。 | 本地参考的普通新会话到首条助手消息才写入 JSONL。路径已返回仍须验证文件存在、身份匹配和重启可打开。[Pi RPC](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc-commands.md#new_session) |

统一界面可保留已确认目标的空白草稿，随后用用户的第一条消息启动原生会话或使其持久化。Bob 仅在原生身份和实际持久性核对后记录为可重启续接。固定请求 ID 绑定设备、工具、工作区与首条消息。工具分配 ID 的创建若丢失回执，只按权威来源核对，无法确定时保留未知状态并避免自动重发。首轮沿用适配器既有权限边界。

### 统一设备入口与 DSH 直连分流

连接类型由无凭据的节点协议发现确认，不根据主机名称推断。Bob 节点请求使用设备 Bearer，不能作为 DSH 启动令牌放入 URL；原生 DSH 直连继续使用 Cookie 与受控日志恢复。已确认节点发生请求错误时不得改投其他 peer，未知发送不自动重发。网页与 Tauri 必须保持一致。

### 活动原会话与新续聊进程的边界

Claude适配器在启动结构化resume前调用同用户官方`claude agents --json`核对活动会话（已核验安装版本2.1.319）。清单包含交互式及后台会话；`idle`仍表示会话被进程持有，不能据此从另一个进程resume。相同`sessionId`返回`HARNESS_SESSION_BUSY`；清单失败或结构无法核实返回`HARNESS_ACTIVITY_UNCONFIRMED`，不会启动续聊进程。旧版本未提供此接口时亦不能宣称已经证明无活动持有者。

该探测并非跨进程原子锁，也不解决探测后另一个外部进程启动的竞争；官方原进程附着/事件接入仍需实现。`claude logs`帮助定义为终端输出，不用它解析状态或充当结构化实时事件源。AGY官方remote-control daemon存在也不自动意味着第三方可消费其网页远控通道；继续使用已核实的官方程序接口，不逆向凭据或刮屏。

### AGY ACP 的能力协商与历史重放边界

官方接入说明：https://antigravity.google/docs/ide/extensions/acp 。AGY 根帮助未列出某子命令不能作为“不支持”的证据；以该子命令帮助及实际初始化能力为准。X1T安装版本1.4.12实测支持`agy acp --stdio`，协议1，复用同用户缓存认证，不从Bob复制OAuth凭据。已验证list/load/resume；不将这些只读能力推导为取消、审批或外部活跃进程订阅已可用。

官方协议`session/load`会重放历史（https://agentclientprotocol.com/protocol/v1/session-setup）。Google文档称resume不重放，但本次同版本实测resume仍产生148条初始化更新。适配器必须显式区分加载阶段与prompt阶段，不能把历史片段算作新回复或把其终态匹配到新请求；常驻连接优先，不能每次网页轮询重新启动ACP及拉全历史。断连后未知prompt不能自动重发。权限请求不能由历史回放自动批准，客户端请求与响应ID分开识别。

外部CLI独立进程的实时订阅还需单独证明；官方Remote Control网页可用不等于提供第三方订阅接口。ACP会话成功加载也不自动证明共享执行者或避免双写。先在私有专用会话确认所有权、精确身份、实际流和取消，再替换既有发送路径。

### Claude 宿主工具清单与逐次审批

节点device_harnesses配置中的claudeTools为显式工具名数组，默认空；仅在claude-jsonl原会话且已通过精确Harness授权时传给执行端。操作端请求和模型提示不能自行扩大清单。最多32项，仅接受规范工具名，不接受通配符、默认整包或命令行参数；空配置保留无工具模式。非空配置经官方stdio审批通道逐次确认，ask规则覆盖已有allow；收到请求时再次核对工具名。取消、过期、无法确认执行者身份和未知结果不得自动批准。

本轮仅验证Read；工具名清单不是文件目录沙箱，仍须核对每次调用的具体参数，不能据此宣称路径隔离或任意写入已验收。执行接纳前持久保存tool_authorization；批准时取原始清单与当前宿主清单交集，缺失旧快照只允许拒绝。此检查针对批准提交前撤销，不撤回已批准或已执行动作；批准后到工具执行间的原子撤销仍未实现。实际证据见CURRENT_STAGE §3.6。官方规则优先级依据：https://code.claude.com/docs/en/permissions 。


### 外部运行中会话的接口边界（实测补充）

历史、执行状态、正文增量、审批决策、成果引用分别验收，不以状态可读推断全文可订阅。Claude 官方 `agents --json --all` 可核对 busy/waiting/idle；终端 attach/logs 不作为结构化事件接口。AGY ACP 已有会话续接与外部进程热附着是不同能力；不得二次启动已有执行中的会话。官方 Hooks 适合作为生命周期观测候选，需核对当前版本载入方式、会话标识与去重后再启用，不能当作 token 流或授权代理。参见 [Claude 官方会话管理](https://code.claude.com/docs/en/agent-view)、[AGY ACP](https://antigravity.google/docs/cli/acp/)、[AGY Hooks](https://antigravity.google/docs/hooks/)、[AGY Sidecars](https://antigravity.google/docs/sidecars/)。
