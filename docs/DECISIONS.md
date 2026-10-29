# Bob — Decisions

> 命名空间 `BOB-D-###`。引用前身项目决策写作 `APP-D-…`（旧 App）/ `ONL-D-…`（旧 Online）。
> 状态取值：`proposed`（待用户裁决）、`accepted`（用户已确认）、`superseded`、`rejected`。已接受的决策不改写，变更通过新决策取代。

---

## BOB-D-001 — 统一项目在新目录重新组织，前身项目归档为 `legacy/v0.9`，新主线从 `1.0.1` 起步

- 日期：2026-10-28　状态：**accepted**（用户确认）
- **决定**：
  1. 统一后的 `bob-agent` 在独立的新开发目录（本仓库，位于工作区规定的本地高速区）中建设。
  2. 前身 `bob-agent`（旧单机 App，v0.9.30，提交 `a4ff5fd`）的完整 Git 历史拉入本仓库并归档至 **`legacy/v0.9` 分支**推送到 GitHub；前身原目录与 `cloud-agent`（旧 Online）保留原位作为只读参考和资产来源。
  3. 统一后的新一代 `bob-agent` 版本号从 **`1.0.1`** 起步推进开发（建立 **`1.0.1` 分支**与快进演进的 `main` 分支）。
- **理由**：避免在正有大量未提交改动的旧仓库里做结构手术；新目录不在 OneDrive 同步范围内，可以规避同步风暴；通过将旧历史归入 `legacy/v0.9` 分支并在其上快进开启 `1.0.1`，实现 GitHub 同名仓库 `bobbik1984/bob-agent` 的零冲突、不断代演进。
- **约束**：从旧仓库迁入代码、数据或文档必须走获批的工作包。旧产品（App 0.9.30、Hostinger 上的 Online）在新内核对应能力通过验收前继续服务。

## BOB-D-002 — 目标用户：白领与普通开发者，单用户多设备

- 日期：2026-10-28　状态：**accepted**
- **决定**：主要服务白领与普通开发者；不做公司级重型协同、多租户或团队权限。
- **影响**：数据模型以“一个用户、几台设备”为前提；默认路径不能要求用户拥有服务器；开发者向的能力（Harness、代码审阅）是能力层，不是首屏门槛。

## BOB-D-003 — 手机形态为 Tauri Android，iOS 后期再评估

- 日期：2026-10-28　状态：**accepted**
- **决定**：手机端沿用 Tauri（与桌面共享 Rust 内核和 Vue UI），先做 Android。iOS 在 BP7 再评估。旧 Online 的 Web/PWA 不再作为主要手机入口（取代 ONL Vision §8 的“PWA 默认”表述，仅对本项目有效）。
- **影响**：内核必须能嵌入 Android（体积、依赖受控）；后台运行、推送与唤醒受 Android 限制，需要单独设计。

## BOB-D-004 — 当前优先级：先完成作者本人完整的个人工作环境（主库在 X1C）

- 日期：2026-10-28　状态：**accepted**
- **决定**：近期交付目标是作者本人的设备与服务（**X1C 作为个人环境的 Home 主权威**，手机作为 Replica，常开节点与 Harness 主机按需接入）组成的可用环境。新项目位于本地高速区，**不依靠 OneDrive 或 Syncthing 同步，代码与文档通过 GitHub 同步**。开源通用化在 BP6 进行，但从第一天起不在代码中硬编码个人基础设施，避免日后返工。
- **影响**：X1C 睡眠或关机时，手机端涉及 Home 修改的操作进入排队，并诚实提示状态；需要 7×24 可达时再评估常开节点与 X1C 的主权威切换。新项目必须在 GitHub 建立远端，未推送的改动没有跨盘备份。

---

## BOB-D-005 — 内核拓扑：一个内核，多种部署形态

- 日期：2026-10-28　状态：**accepted**（用户确认统一内核）

### 问题

前身项目各有一个“工作内核”：旧 App 的 Persistent Work Core（Rust 加 SQLite，Project、Goal、Task、Decision、Artifact、Evidence 等，APP-D-008 / APP-D-018），旧 Online 的 Bob Core（TS 加 SQLite，Project、Task、Execution、Artifact、Verification、Review，ONL-D-001）。两者实体**同名不同义**。统一后应该是一个内核，还是保持两个各管一块？

### 对比（以 BOB-D-002 的用户定位为前提）

| 维度 | 统一（一个内核） | 分开（App 内核加 Online 内核） |
|---|---|---|
| 用户心智 | 一个 Bob、一个事项列表 | 用户要理解“本地事项”和“在线事项”以及两者之间的晋升 |
| 无服务器起步 | 内核嵌入 App 即可完整运行 | App 内核可以，但 Work/Harness 能力必须依赖服务器 |
| 跨设备一致 | 同一套语义：一个主权威（home）加多个副本 | 两套语义、一层映射、一套晋升和交接协议 |
| 单人维护成本 | 一套 schema、迁移、测试 | 两套实体、两套迁移，映射层永久存在 |
| 迁移代价 | **高**：Online 的治理语义（Task/Execution/成果版本/Review）要在新内核重新实现 | **低**：基本维持现状 |
| 长期风险 | 一次性重写风险，可以用渐进替换缓解 | 双重权威漂移（同名实体已经出现） |
| 常驻执行 | 同一内核以无界面形态跑在常开设备上 | Online 服务器承担 |

### 推荐

**统一的优势更大。** 你的用户是单人多设备、不能假设有服务器；分开方案最大的好处（迁移便宜）只是一次性的，而它的代价（两套语义加映射层）会伴随整个产品生命周期。推荐形态：

1. **一个内核（`bob-kernel`，Rust）**：领域模型、状态机、存储、事件日志、权限与契约只实现一次。选 Rust 的理由：旧 App 已经是 Rust；能嵌入 Android；能编译成单一二进制，白领安装门槛低；常开节点也能跑同一个二进制。
2. **多种部署形态**：嵌入 Tauri 桌面和 Android App；无界面的 `bob-node` 跑在常开设备或 VPS 上。三者是同一份代码。
3. **执行适配走进程边界**：Harness runner（DSH、AGY、Codex、Generic API）通过版本化契约与内核通信，可以用任何语言实现。过渡期可以直接复用旧 Online 的 TS 适配器作为 runner，不必先重写。
4. **数据：一个主权威加多个副本，不做多主合并**。每个用户指定一个 home 内核作为主权威（作者的个人环境可以是常开节点；普通白领默认是自己的 PC）。其他设备保存只读缓存和带 `expected_version` 的意图队列（outbox）。追加型对象（捕获、消息）可以在任意一端创建后汇入 home；修改已有对象必须由 home 接受，版本过期就拒绝并解释原因。
5. **渐进替换**：旧 Online 继续在 Hostinger 服务，新内核先实现契约和领域语义；某项能力在新内核验收通过后，再迁移数据、退役旧路径。旧 Online 的约 6 万行 TS 中，直接复用的主要是设计、不变量和测试场景，而不是代码本身。

### 需要验证（WP-02）

- 旧 App 的 PC↔Android 同步引擎是什么模型（多主还是主从、冲突怎么处理），能否改造成“home 加副本”；
- 两边同名实体的语义对齐表（Task、Decision、Artifact、Evidence、Goal 对 Execution）；
- 把 Online 的治理语义移植到 Rust 的工作量估算；如果代价明显失控，退回“分开加契约优先”的方案，作为本决策的回退条件。

### 被比较的其他选项

- 单内核 TS，App 内嵌 sidecar：违背轻量交付，Android 基本不可行，**不推荐**。
- 保持双内核，只做契约对齐：迁移最便宜，作为 BOB-D-005 的**回退方案**。

## BOB-D-006 — 跨设备连接：分层连接，开源默认不依赖任何人的服务器

- 日期：2026-10-28　状态：**accepted（设计）**；用户本轮明确同意 connectivity 与 threat-model 两份文档。现场安全与跨网验收未运行。
- **决定**：连接抽象为 Connection Provider，按顺序尝试：局域网直连 → P2P 打洞直连 → E2EE 中继（可替换：社区中继、项目中继或自建）→ 借用用户自己的云盘做异步信箱。常开节点是可选增强。分析与依据见 [wiki/connectivity.md](wiki/connectivity.md)。
- **个人环境（BOB-D-004）**：可以先用作者已有的自建中继和 Headscale，但必须作为可替换的 Provider 配置接入，不写死在代码里。
- **待验证**：在作者的真实网络（家庭 Wi-Fi、4G/5G、办公网）上实测 P2P 直连成功率；评估云盘信箱的延迟和可靠性；威胁模型设计已获批，仍需按其中门槛验证实现。

## BOB-D-007 — 文档权威位置与命名空间

- 日期：2026-10-28　状态：**proposed**
- **决定（草案）**：统一项目的权威文档在本仓库 `docs/`。Drive `bob-online/docs` 和旧 App `docs/` 继续作为前身项目的权威和历史，不再承接统一项目的新结论。ID 加命名空间前缀（见 [wiki/glossary.md](wiki/glossary.md)）。
- **待定**：是否需要在 Drive 放一份只读镜像，方便手机端 Bot 和云端 Agent 读取。

---

## BOB-D-008 — 懒式响应（Lazy Response）与非即时模式下“发送必不失败”铁律

- 日期：2026-10-28　状态：**accepted**（用户确认）
- **背景与审计证据**：见 [evidence/2026-10-28-lazy-send-audit.md](evidence/2026-10-28-lazy-send-audit.md)。`legacy/v0.9`（提交 `a4ff5fd`）虽在 Work 面板引入了 `linkDormant` 懒式休眠同步，但仍存在 4 条会导致非即时模式下发送失败的漏洞（底层 Socket 刚断未进休眠时发送未出网即被误标为 `REMOTE_OUTCOME_UNKNOWN`、`connecting` 竞态跳过唤醒、唤醒重连后旧 `watchId` 触发 `REMOTE_STALE`、以及 Chat `@pc` 模式离线弹窗拒发）。
- **决定**：`1.0.1` 统一内核全面采用**懒式响应（Lazy Dormant Connection）+ 本地持久化发件箱（Local-First Durable Outbox）**架构，并强制执行以下四条不变式：
  1. **本地落盘即发送成功（Local-First Durable Outbox Acceptance）**：无论当前链路处于懒式休眠（`linkDormant`）、唤醒重连中（`connecting`）、L4 云盘信箱模式，还是 X1C Home 睡眠/暂不可达，用户点击“发送”时，只要消息写入本机 SQLite `intent_outbox`（带全局唯一 `request_id` 与幂等键，初始状态 `local_saved` → `queued_for_wakeup`）成功，UI 必须立即接纳消息上屏并清空输入框，**绝对禁止因为非即时连接状态而让发送动作失败或阻塞用户**。
  2. **输入框聚焦/打字静默预热（Speculative Pre-Warm on Focus/Typing）+ 单飞唤醒复用（Single-Flight Wakeup Await）**：
     - **打字即预热**：当用户点击输入框（`@focus`）或开始输入第一个字符（`@input`，带 15 秒节流冷却）时，若当前处于 `linkDormant`、连接未就绪或距上次心跳超过阈值，后台**静默启动预热探针与重连（Pre-Warm）**（`preserveView: true`，不置 `busy`、不闪报错条、不打断打字）。利用用户打字的 3～15 秒时间差提前完成隧道建连与 `session/watch` 握手，实现“打完字点发送即刻秒发”的无感体验。
     - **单飞 Promise 共享**：预热与发送共用同一个 `inFlightWakeupPromise`。若用户打字极快、点发送时预热握手尚未结束，`send()` 直接 `await` 该在途 Promise 并在拿到新 `watchId` 后立即投递，既不重复建连，也不会因 `connecting == true` 跳过唤醒或误用旧 `watchId` 触发 `REMOTE_STALE`。
     - **未发自动回眠**：若用户仅聚焦输入框但最终未发送消息，预热连接在空闲超时后自动回落至 `linkDormant` 休眠态。
  3. **严格区分 `not_dispatched`（未出网）与 `unknown_send`（已出网未确认）**：
     - 凡是在本地连接状态检查（`!alive`）、唤醒重连或 HTTP/QUIC 建连阶段失败（请求字节尚未写入网络流），一律归类为 **`not_dispatched`**，保留在 `intent_outbox` 中等待通道就绪后自动重发，**严禁打入 `REMOTE_OUTCOME_UNKNOWN` 锁死会话**。
     - 仅当请求字节已写入传输层、在等待 ACK 期间连接中断时，才标为 **`unknown_send`**；重连后由内核先携带同一 `request_id` 自动向对端查询幂等回执对账，未执行则自动补发，已执行则直接绑定结果流。
  4. **空会话一致休眠**：无论会话是否已有历史消息（含 `messages.length === 0` 的新建会话），只要处于静止阶段且无待确认交互，空闲断连一律平滑进入 `linkDormant` 懒式休眠。
