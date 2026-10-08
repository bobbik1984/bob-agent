# Bob 当前开发清单

> 当前工作树是从 v0.9.15 建立的 `mobile-pc-rebuild` 独立施工线，不包含旧 A1/A2/A3 后续版本的全部代码。下面原双线勾选和测试数是历史线路记录，**不能当作本施工线已具备或已验收的能力**；当前状态以 [稳定重建计划](docs/superpowers/plans/2026-10-01-mobile-pc-startup-baseline-plan.md)、[真机启动证据](docs/evidence/track-a/S1-inplace-upgrade-preflight.md) 与本分支 `progress.yaml` 为准。

## 当前施工：手机→PC 稳定重建

- [x] 以 v0.9.15 为代码基线建立独立施工线；保留旧诊断线与手机原数据。
- [x] 0.9.22 候选签名包在 Pixel 8 同包名保留数据覆盖安装；三次启动进入实际对话界面，旧会话可见。不据此断定旧卡屏的唯一根因。
- [x] 统一主应用与 PC 安装器的 0.9.22 版本元数据，并为双端候选 CI 加只读版本门禁及 Artifact-only 发布隔离；本地验证通过，尚未执行双端云端构建。[证据](docs/evidence/track-a/S1-dual-platform-version-contract.md)
- [ ] 完成 S1 未配对、无网、PC 离线、错误配置与失败晚到事件的启动矩阵；高风险工具监听也需实机验证。
- [/] 0.9.24 已完成双端版本同步与本地测试；待同一提交的 PC 安装包、签名 APK、SHA 与签名核验。用户已授权创建候选预发布；双端安装验收前不更新官网稳定入口。
- [/] 手机解绑后移除撤销入口、对话标题居中与默认 Logo 品牌蓝：代码及自动化测试通过，待新候选包实机核对。
- [ ] 按已确认的用户流程重新接入可信设备配对与手机→PC 只读委派；不整批拣选旧 A2/A3 提交。
- [ ] 只读委派通过后，再接入受控修改、审批、停止与断线恢复，最后做完整双端真机验收。
- [x] **自进化与做梦引擎 v2.0 闭环升级**：
  - [x] **Step 1 (P0 稳定性与幂等性)**：引入防重入原子锁 (`ROUTINE_RUNNING`, `DREAM_RUNNING`)；Clerk 失败/超时时不丢失未分析错误；避坑指南独立解耦至 `memory/AVOIDANCE.md`（带上限 20 与去重合并，并无损迁移旧 `SOUL.md` 内容）；清理过时与合并旧记忆时级联执行 `DELETE FROM wiki_fts` 清除倒排索引；笔记语义消化安全连接、防重及唯一事件 ID。
  - [x] **Step 2 (P1 记忆生命周期状态机)**：frontmatter 升级支持 `status: candidate | active | superseded | rejected`；反思/纠错特权保护（`type: feedback` 或 `protected: true` 永久免除 30 天清理）；同类感知 Bigram 相似度合并，杜绝异构知识误删。
  - [x] **Step 3 (P1 遥测闭环与动态作用域)**：联动 `session_observations` 与 `execution_errors` 闭环计算工具故障率变化（标记 `validated` / `unverified` / `decayed`）；提供 `get_avoidance_rules_for_tool` 动态作用域并在 `llm.rs` 中自动注入系统提示词。
  - [x] **Step 4 (P2 人在回路与全貌可视化)**：SOUL 精炼提案机制（生成 `SOUL_PROPOSAL.md`，提供 `system_get_soul_proposal` 与 `system_review_soul_proposal` 审阅命令）；`dream_report.json` 结构化持久化直接激活 Daily Brief 洞察卡片；自动化单元测试全绿（341 项 Rust + 72 项前端全部通过）。
- [x] **跨端同步鉴权与模型注册表收口 (ERR-SYNC-05 & DeepSeek V4.1 Flash 修复)**：
  - [x] **局域网同步鉴权闭环**：重构 `verify_rpc_request_auth` 为签名优先、滑动窗口 TTL（活跃期间不超期）与已配对受信设备新 Session 自动登记，彻底消除 `ERR-SYNC-05` 阻断拉取 PC 待办缺陷。
  - [x] **心跳与状态日志语义校准**：`formatSyncLogDetail` 支持识别 `heartbeat` / `心跳`，准确呈现“连接事实已确认”，消除心跳被误当作“数据已在本机确认写入”的日志不对等现象。
  - [x] **模型注册表升级与防降级防护**：`resources/model_providers.json` 正式收录 `deepseek-flash`（DeepSeek V4.1 Flash）并设为 default，升级 schema_version 为 3；`merge_synced_config` 增加防降级屏障，防止远端旧配置将 `deepseek-flash` 覆盖回 `deepseek-v4-flash`；343 项 Rust 测试与 72 项前端测试全绿。

> 历史双线计划（2026-09-22）：[A 线独立开发步骤](docs/superpowers/plans/2026-09-22-track-a-security-and-client.md)。以下内容保留作旧线路参考，不作为当前重建分支的完成状态。

> 当前发布线：v0.9.6
> 产品方向：`docs/PRODUCT_VISION.md`
> 阶段顺序：`docs/BOB_EVOLUTION_ROADMAP.md`
> 当前计划：`docs/superpowers/plans/2026-09-21-mobile-pc-remote-delegation-plan.md`

本文件只保存当前实施批次和紧邻下一批的任务。目标架构不得写成已实现能力。

## 2026-09-22 安全审计收口（优先于新增 Harness）

当前实现已有恢复日志和生产取消核心，不机械沿用上一轮缺陷结论。以下安全与真机质量门尚未通过；下方历史勾选只记录实现，不代表整条通道已安全验收。

- [x] A1 / P6-H 恢复、审批与取消闭环加固（A1_RECOVERY_STATE_MACHINE_ACCEPTED / P6_H_GATE_PASSED；53 项同步恢复测试全绿）
- [x] A2 / SEC-01 设备发现与可信身份分离（R16 本地自复核通过、sha2 依赖已获授权；[R13 独立复核](docs/evidence/track-a/A2-independent-review-20260923.md) / [R16 证据](docs/evidence/track-a/A2-r16-self-review.md)）。撤销出件箱已接入 Relay/LAN 重试与签名 Ack，长期离线证书、生产 handler 复用和恢复 Fail-Closed 已整改；A2 本地节点已完成；真机验收归 A4。A3 仅开放本地审计与设计，A4 继续锁定。
- [x] A3 / SEC-02 分开审查 LAN/Relay 同步、远程指令、Web Drop 加密边界（[A3 证据](docs/evidence/track-a/A3-transport-data-boundary-review.md)）：实事求是断定 LAN (明文 HTTP + Ed25519 签名) 与 Relay (TLS 终结于中继服务器，明文 JSON) 均非 E2EE，Relay 节点对中转载荷完全可见；仅 Web Drop (WebRTC DTLS / AES-128-GCM Hash 密钥) 实现端到端加密；修复 LAN REST 请求缺少签名信封漏洞，补齐 WebSocket 连接主体绑定；重构 LAN/Relay Outbox 推送协议为“确认收到对端 commit_ack 才删本地暂存”，在网络失败、HTTP 400/401/500 或无回执时严格保留待重试；推送失败全面阻断并不再误报完成，向界面显式报告“部分同步失败”，底层返回 Err；测试校准为发送端持久化状态机。
- [x] A3 / SEC-03 同步载荷脱敏、日志凭据防护与接收端语义闭环（[A3 证据](docs/evidence/track-a/A3-transport-data-boundary-review.md)）：彻底废除黑名单模式，改为严格正向白名单 (Constructive Allowlists)；`export_sync_data_from_conn` 从零构造安全配置，剥离所有未知键与嵌套凭据，自定义模型仅保留脱敏元数据，settings 采用 `WHERE key IN (...)` 显式白名单；非对象配置在 `read_config_checked_at`、`merge_synced_config`、`import_sync_data_to_conn_atomic` 实施统一 Fail-Closed 阻断，杜绝未清洗远端配置倒灌；统一 `ALLOWED_REMOTE_PUSH_OPS` 为 `&["set_config"]` 消除与底层执行器割裂；根治接收端无法识别合法 JSON 漏洞 (Fail-Closed HTTP 400)，禁止静默丢弃；确立 `PushCommitReceipt` 强类型契约，严格区分 `applied`（已生效）与 `pending_apply`（待应用）并阻止界面打勾；磁盘写入失败与重试收敛通过端到端测试验证；完成 `ActiveSyncOutcome` 强类型返回值升级，诊断与历史记录严格区分 `applied` 与 `pending_apply` 绝不虚报成功；解耦配对与首次同步状态，修复首次同步失败或非预期时仍报成功漏洞；连接面板准确映射状态并实现迟到事件防护；本地导入历史接入 `build_sync_import_history_entry` 绝不提前宣称成功；单元测试切入真实生产处理函数全覆盖验证；修复 App.vue 启动自动同步非 applied 误报成功漏洞 (仅限明确 status === 'applied' 标记成功与更新时间，未知/非预期/空值报错阻断时间戳更新并启动静默监听)；消除测试用例内状态复制，提取并接入生产 `applyStepTransition` 函数；修复 Vitest 缓存目录只读引发的 Exit Code 1 缺陷；全套 Rust (36项http_api + 48项device_trust + 61项sync_engine) 与 62 项前端自动化测试全绿通过。A3 本地复核通过；A4 真机验收尚未执行，A4 保持锁定。


- [ ] P6-DEVICE 固定双端版本，验收真实手机—PC 指令、追问、批准/拒绝、停止、断线、撤销和重启恢复；本地单元测试不代替真机。
- [ ] P6-DOC 验收后才更新完成状态和 README 安全声明，区分工作树、测试报告与已发布产物。
- [ ] DSH-ACP（上述收口之后）：评估独立 ACP Adapter，先只读后受控修改；只复用 MCP 的进程/传输设施，Bob 保留项目、权限、Review 和工作记录权威。
- [ ] TRANSPORT-OPTION（后续可选）：评估 Tailscale/受控代理，不作为开源用户必装条件、不代替应用授权；Noise 等方案尚未选型。

## 已完成主线：Phase 0–4 Work Core、变更审查与复杂度路由

- [x] 建立 Work 功能 Tab、按项目分组滚动、独立多项目展开及折叠数量摘要的第一版组件。
- [x] PC 与 Android 统一为同一套 Work 功能 Tab 和项目分组，移除旧口号式页面；Quick Note 采用共享宽度与 3–5 项等宽动作网格；安装器系统命令静默运行。
- [x] Work 响应式导航复用既有页面模板：宽屏进入左侧二级栏，窄屏与 Android 顶部贴边展示，选中项使用主题色实色块。

### WC-001 文档与术语收口（P0，已完成）

- [x] 将“Bob 让复杂工作不断线”确立为产品北极星。
- [x] 明确 Bob Core、Orchestration、Runtime 与 Integration 边界。
- [x] 建立 canonical state、渐进演进、Runtime 可替换和 Graph/Loop 分层的 Decision Log。
- [x] 重写当前路线图，保留 `v0.8.0` 为不可修改的 Capture 历史基线。
- [x] 完成文档一致性检查并提交 Phase 0。

**验收**：愿景、路线、架构、代码导航、架构决定、任务和进度各有唯一职责；规划能力没有写成现状。

### WC-101 Work Object 契约（P0）

- [x] 定义 Project、Responsibility、Goal、Milestone、Task、Decision、Artifact、Evidence、Risk、Change、Commitment。
- [x] 冻结类型前缀、状态、revision、时间、软删除、来源和幂等字段。
- [x] 明确 Decision 的 alternatives、evidence、participants、owner 与 revisit condition（Phase 3）。
- [x] 明确 Work Object 与 Note、Source、Event、Todo、File 的引用关系。

**验收**：schema 可序列化、可版本化；非法状态和缺失关键字段无法进入 Repository。

### WC-102 SQLite Repository 与 Work Event Journal（P0）

- [x] 增加向前兼容 migration，不修改真实 Markdown 文件。
- [x] Repository 统一负责事务、幂等、乐观 revision 和软删除。
- [x] 所有状态变化写入 append-only `work_events`。
- [x] 同一幂等键返回原回执；跨对象失败必须整体回滚。
- [x] 增加 schema、事务、冲突、软删除和事件顺序单元测试。

**验收**：进程重启后状态不丢失；重复请求不创建第二对象；失败不留下半套 Project State。

### WC-103 Project 聚合与可迁移快照（P1）

- [x] 聚合目标、当前阶段、开放任务、决定、风险、近期变化和下一步。
- [x] 兼容现有 Markdown Project 稳定 ID，只注册不迁移真实数据。
- [x] 生成只读 Markdown 项目快照，禁止快照反向覆盖较新运行状态。
- [x] 增加新会话恢复、重启恢复和快照稳定性测试。

**验收**：不读取旧对话上下文也能恢复准确项目摘要；不同 Agent 可通过 Markdown 快照理解项目。

### WC-104 最小 Project API 与 UI（P1）

- [x] 增加 Project/Goal/Task/Decision Tauri Commands。
- [x] 只通过 `tauri-bridge.js` 暴露给 Vue。
- [x] Project 页面只展示 Goal、状态、变化、任务、Decision 和用户需关注项。
- [x] 同步中英文 i18n，继续使用 Lucide 和设计变量。
- [x] 完成前端测试、Rust 测试和生产构建。
- [ ] 在下一次 PC/Android 发布产物上完成客户端体积对比和真机紧凑布局验收。

**验收**：用户能创建并重新打开 Project，查看为什么作出决定以及下一步；UI 不暴露内部 DAG、prompt、token 或进程。

## WC-201–204 现有入口关联（P1，已完成）

- [x] Capture 可事务性产生或关联 Project、Task、Decision、Meeting、Change 和 Commitment。
- [x] 项目归属使用有效 ID 或唯一精确标题；歧义保存为 WorkView 待归属项，不弹窗打断。
- [x] Note 只登记单项目归属引用；Source/Knowledge Point 可被多个 Project 引用且不复制正文。
- [x] Todo/Event 与 Work Task/Milestone 建立稳定引用，Calendar 保持状态真相源。
- [x] 文件只记录原路径、流式 hash、大小和 mtime；同路径内容变化生成待确认 Change。
- [x] 日程完成、取消、改期、删除追加 Work Event，不复制外部状态。
- [x] 覆盖幂等、revision、重名、缺字段、多对象回滚和跨项目知识引用测试。

**验收**：Capture、Candidate、Work Object、外部真相源和 Work Event 可相互追溯；重复处理不创建第二对象；歧义不阻止 Todo/Event/Markdown 先可靠落库。

## WC-301–303 Decision Memory 与 Change Review（P1，已完成）

- [x] Decision 补齐 alternatives、rejected alternatives、evidence、participants、owner 与 revisit condition，并兼容旧数据。
- [x] 同路径新版文件保留旧 Artifact，原子创建新版 Artifact、Change 和影响 Review。
- [x] 基于显式关系、Decision evidence、旧 Artifact 和同项目对象 ID 分析受影响 Decision、Goal、Task、Artifact 与 Risk。
- [x] 提供用户确认、拒绝、延后、重新打开和影响说明；选择写入 Work Event。
- [x] 只有确认后才建立 `affected_by`、`contradicts` 或 `supersedes`，不自动改写既有事实。

**验收**：新版文件能够指出前后 fingerprint、旧/新 Artifact、受影响对象、证据和待确认关系；无证据时明确显示影响范围未知；重复处理、revision 冲突和事务失败不会产生半套状态。

## WC-401–403 Complexity Router（P2，已完成）

- [x] 定义 Direct、Deep、Advanced 的结构化路由结果、task kind、置信度、风险、持续性和原因代码。
- [x] 确定性信号优先，真正模糊语义才限时调用 Clerk；断网或解析失败保守只读降级。
- [x] 复杂只读分析与复杂 Action 分离，路由和用户覆盖均不改变 R0–R3 权限。
- [x] Auto Advanced 不自动调用旧 Goal Loop，不宣称跨时间目标完成。
- [x] 建立 30+ 个中英文回放场景并在回复中展示低干扰路由标签。

**验收**：普通问答、长文本和重复提醒不被过度升级；复杂分析进入 Deep；持续、跨时间、恢复和阶段依赖进入 Advanced；Clerk 不可用不阻止基本问答和单步操作。

## 已完成批次：Phase 5 Advanced Project Loop

- [x] Goal Compiler 生成 outcome、evidence、scope、constraints、budget、risk policy 和 blocker policy。
- [x] 用 SQLite 持久化 Goal 状态、尝试、证据、审批、事件和检查点，应用重启后恢复安全 R0/R1。
- [x] 建立单 Agent `observe → plan → act → verify → repair → finish` 有限循环，全局最多一个活动执行切片。
- [x] Done 必须绑定 Evidence；等待用户、阻塞、超预算、失败和取消具有明确状态。
- [x] R0–R3 Policy Engine 继续作为唯一权限边界，R3 handoff 不视为批准。
- [x] Chat/WorkView 展示状态、下一步、预算、恢复点、结构化选项和本地化错误。
- [x] 前端测试、生产构建与 Rust `cargo check --lib` 通过，未新增依赖。
- [x] 完整 `cargo test --lib --offline` 通过：140 passed、0 failed、1 个真实数据审计测试按设计 ignored。

## 已完成产品纵切片：Conversation-first Today Layer

- [x] 对话首屏显示一个焦点、最多两个关注项和可展开详情，不新增独立工作首页。
- [x] 只读聚合 Calendar、Todo、Work Core、Goal Runtime、Session 与 Dream；来源独立降级。
- [x] SQLite 缓存 fingerprint、revision 与逐设备已读；内容不变不制造更新。
- [x] Chat、桌面/移动入口与 Quick Note 共用唯一 Today Surface；速记交接保留草稿。
- [x] 手机使用紧凑非全屏弹层和内部滚动；支持 Escape、焦点恢复和 reduced motion。
- [x] 不新增客户端依赖，不调用大模型完成常规排序；中英文 i18n 与 Lucide 图标一致。
- [x] 9 项前端测试、生产构建、12 项 Daily Brief Rust 测试及 140 项完整 Rust 回归通过。
- [ ] 在下一次 PC/Android 发布产物中完成真机 UI、客户端体积和跨端已读/刷新验收。

## 当前主线：Phase 5.5 可靠个人 Agent 闭环

- [x] 收敛 Bob V3 目标设计，并形成 Phase 5.5-A/B 可执行计划。
- [x] 建立无依赖基线脚本，记录现有产物、manifest 和数据库大小；既有产物来源未验证，性能字段明确为未测。
- [ ] 在受控安装版上补测冷启动、空闲内存/CPU，并记录 Android 当前版本产物。
- [x] 定义 PurposeFrame、AssistantContext、来源、时效、冲突、置信度与上下文预算。
- [x] 复用 Work Core 实现确定性候选解析，不新增数据库表或依赖；Today focus 尚未接入。
- [x] 以默认开启的影子模式记录候选和 reason code；中低置信度不绑定项目。
- [ ] 完成 PC 真实场景误选检查后，才启用唯一高置信度 Context Packet。
- [x] 全量工具权限回归证明 Context Resolver 不改变 Complexity Router、R0–R3 权限和现有降级路径。
- [x] 11 项上下文测试、完整 Rust 回归、19 项前端测试和生产构建通过。
- [ ] 完成至少一次 PC 真实场景验证；Android 行为留到 Capability Snapshot 阶段验证。

**验收**：用户只说目的时，唯一明确项目能够恢复；两个合理候选时不会误选；上下文包有来源、revision、时效和严格预算；关闭开关可以无损退回当前聊天路径。

### Phase 5.5-C–E 内部闭环（P0/P1）

- [x] 停止 Dream 自动重写 SOUL，并将重复工具失败分流为待审阅 diagnostic candidate。
- [ ] 用真实 PC Work Core 数据完成 shadow 误选检查，证据通过后启用唯一高置信度 Context Packet。
- [x] 让 Auto Advanced 优先绑定解析后的真实 Project；歧义时不创建正式 Goal，只询问一次关键问题。
- [x] 实现纯 Rust Capability Snapshot；区分可调用、降级、不可用、文件授权范围与已连接 PC。
- [x] 将模型工具表与当前能力求交集；不可用能力不得暴露，检测到但缺少适配器的 PowerShell/Git 不进入工具表。
- [x] 实现 `local_execute / pc_handoff / ask / defer` 确定性 Action Selector。
- [x] 建立错误分类和有界修复：只读瞬时故障确定性重试一次，Advanced 最多一次改变策略，未知副作用立即停止。
- [x] 为有副作用的 Direct Action 增加最小幂等 ResultReceipt；Advanced 复用 Goal Evidence、Attempt 与 Event。
- [x] 收紧记忆准入：明确纠正和长期偏好可生效，验证成功只生成待审阅经验候选；单次例外和工具失败不升级。
- [ ] 通过五个日常场景、故障注入、PC/Android 真机、体积与资源质量门。

**验收**：Bob 能从目的恢复正确对象，感知真实能力，选择最轻路径，以回执证明完成，并在不污染人格和不增加用户配置的前提下成长。

## 已完成主线：跨端扫码配对与双向同步闭环 (v0.9.5-h)

- [x] T-2301 扫码配对 MVP (局域网直连 + VPS 信令降级 + Ed25519 握手)
- [ ] 跨端配对授权安全整改见 SEC-01：既有公钥字符串/名册放行不证明身份，撤回“根治”的验收表述。
- [x] 修复移动端日程自动聚焦到当前时间红线与重复标题 Bug
- [x] 修复移动端笔记列表底部安全区遮挡问题 ( safe-area bottom padding )

## 当前执行主线：Phase 6 移动端远程执行设备协同 (Mobile-to-PC Agent Delegation)

### 阶段一：设备选择与只读闭环 (Phase 1: Device Selection & Read-Only Loop)
- [x] **阶段一：输入框上方执行设备状态轨与切换器 (UI Rail & Device Selector)**：输入框上方紧凑常驻胶囊 `[💻 设备名 · 在线 ▾]`，触控区域 $\ge 44\text{px}$，使用纯 SVG 图标与实色表面，点击弹出设备选择底栏。
- [x] **阶段一：加号扩展菜单精简重构为三项 (附件 / 执行设备 / 对话模型)**：移动端聊天输入框 `+` 弹层收敛为“📎 添加附件”、“💻 选择执行设备”、“🧠 当前对话模型”。
- [x] **阶段一：会话级执行设备粘性绑定与离线显式报错防御**：支持首条指令 `@pc` 或手动选择设备后后续追问沿用设备；切到历史会话恢复绑定；目标设备离线时显式提示错误并提供重试/切换，绝对严禁静默回退手机。
- [x] **阶段一：移动端与 PC 端 RPC 指令协议与只读无头 Agent 闭环**：移动端封装含 `task_id`、`conversation_id`、`instruction`、`target_device_id` 的 RPC 消息；PC 端后台无头 Agent 调度执行只读文件读取与分析，返回结构化结果并记录日志。
### 阶段二：PC 端修改与审阅闭环 (Phase 2: Diff Preview, Mobile Review & Stop Ack)
- [x] **PC 端受控修改暂存与 Diff 计算**：写操作严禁静默覆写用户工作区文件，统一生成 Unified Diff 并在内存中暂存为 `StagedChange`，返回 `needs_approval`。
- [x] **移动端 Diff 审阅卡片 (`DiffReviewCard.vue`)**：支持路径缩略、`+N -M` 统计、语法增删高亮、超长行折叠、$\ge 44\text{px}$ 批准与拒绝触控交互。
- [x] **双向审批 RPC 流 (`rpc_approval_decision`)**：手机端批准后，PC 原子落盘并返回 `applied`；拒绝则销毁暂存并返回 `rejected`。
- [x] **显式中断与 Stop Ack (`rpc_cancel` / `rpc_cancel_ack`)**：手机端点击停止时向 PC 发送取消指令，PC 依托 Tokio Watch 通道即时中断正在运行的 Agent 任务并回传确认，确保双端状态 100% 对齐。
### 阶段三：模型与 Harness 设备能力动态发现与本地密钥隔离 (Phase 3: Capability Discovery & Session Model Isolation)
- [x] **本地密钥隔离与模型过滤铁律 (`SafeModelInfo`)**：双端各自管理本地 API Key，网络传输严禁携带任何 API 密钥或凭证；PC 动态脱敏并只返回已配置可用供应商的安全模型元数据。
- [x] **跨设备能力探针 (`rpc_discover_capabilities`)**：手机端向 PC 发起能力探测，PC 动态返回硬件与环境 Harness 快照（工作区全权限、系统终端、桌面浏览器、Git 管理、专业文档导出）。
- [x] **会话模型独立性与跟随设备重置**：手机端执行设备选定 PC 时，加号模型菜单动态展示 PC 可用模型列表；支持独立为当前会话指定生效模型，或随时重置为“跟随电脑默认模型”。
- [x] **执行轨状态与模型铭牌联动**：输入框上方执行轨直接展示当前会话在目标电脑生效的模型徽章，点击可直接拉起模型抽屉切换。
### 阶段四：接入统一工作记录 (Work Object / Journal / Evidence)
- [x] **跨端指令执行日志追加 (`remote.instruction.executed`)**：PC 端收到并执行移动端下发的指令后，原子写入 `work_events` 表，记录请求 ID、来源设备、执行耗时、生效模型及状态。
- [x] **变更对象化与证据链管理 (`WorkObjectKind::Change` & `Artifact`)**：当远程指令产生代码/文件修改时，自动创建 `Change` 工作对象（`needs_review`）；用户批准后状态推进至 `accepted` 并生成对应的 `Artifact` 工作产物与回执证据。
- [x] **中途取消与中止工作流对齐 (`remote.task.cancelled`)**：移动端点击停止后，PC 即刻同步更新关联暂存提案状态为 `cancelled`，并写入工作事件日志，保持双端状态强一致。
- [x] **统一工作视图与看板联动 (`WorkOverview` & `work_event_list`)**：前端 WorkView 动态加载并呈现远程指令与变更事件，多语言提示中英文完整对齐。
### 阶段五：深度闭环与真实安全加固 (Phase 5: Deep Closed-Loop Hardening / Track A Node A1 P6-H)
- [x] **真实基线校验与文件落盘引擎**：读取真实磁盘基线计算 Diff 与新内容，审批时校验基线 Hash 防外部篡改，原子写入与读回校验，支持空文件写入。
- [x] **工具层硬性安全拦截**：`ToolExecutionPolicy` 只读模式硬拦截一切写操作；受控模式拦截写操作转换为 StagedChange。
- [x] **PC 端多轮对话上下文持久化与历史回放**：基于 `remote_conv_{conversation_id}` 在 PC 端 SQLite 加载历史上下文并双向持久化，支持连续追问。
- [x] **SQLite 提案持久化与项目路由**：新增 `staged_changes` 表，重启不丢失，写入成功才标记 applied，遵循并透传 projectId。
- [x] **回滚失败保护与备份绝对留存**：`safe_restore_target_from_backup` 核验字节完全匹配后才删备份；重命名与复制均失败绝不删备份并触发全局降级。
- [x] **完整文件—数据库一致性与崩溃恢复自愈**：建立 `staged_write_recovery` 预写日志与应用启动自愈；applied 状态核验磁盘目标，外部冲突保留准入锁，崩溃自愈具幂等性。
- [x] **准入控制 Fail-Closed 与全局降级**：路径恢复阻塞查询失败硬性拒绝写入与审批；自愈故障全局降级拦截后续写操作。
- [x] **取消确认消除误报分支**：实现 `cancel_active_rpc_task_core`，严格区分确认中止 (`done=true`)、超时 (`confirmed: false`)、通道异常与未知任务，杜绝误报。
- [x] **自动化测试矩阵与故障注入全绿**：53 项 Rust 同步恢复测试、32 项 Work Core 事务测试与 44 项 Vitest 前端测试全绿。
- [x] **A1 独立复核与后续流转**：独立复核通过（A1_RECOVERY_STATE_MACHINE_ACCEPTED / P6_H_GATE_PASSED / PHASE_6_IN_PROGRESS / A2_SEC_01_AUTHORIZED_IN_LOCAL_SCOPE）。
- [x] **A2 / SEC-01 设备发现与可信身份分离**：R16 本地自复核及 sha2 依赖授权完成；Rust 全目标库测试 308 通过（1 ignored）、实际 Relay 测试 1 通过、前端 53 通过。A2 本地节点通过；真机与公网 Relay 未执行，不视作发布验收。A3 可开展本地传输与数据边界审计，A4 保持锁定。[R16 证据](docs/evidence/track-a/A2-r16-self-review.md)。

## v0.8.0 遗留质量门

- [ ] 使用真实 PC/Android 完成 Capture 三入口与 Relay trace 对账。
- [x] 记录 `0.9.0` PC 绿色包与安装器的字节数及 SHA-256。
- [ ] 记录 Android APK/AAB 字节数，并完成 PC/Android 真机对照验收。
- [ ] Source 正文提取、Knowledge Point 蒸馏和证据关系继续独立演进，不阻塞已完成的 Work Core 引用层。

## 暂缓

在 Phase 5.5 五个日常场景稳定通过前，暂缓 Dynamic Task Graph、多角色编排、Runtime Host、订阅调度、完整任务图 UI、通用 Shell、自动 Skill、iOS、独立 Web UI 和新增通讯渠道。

## 完成规则

任务只有在代码、测试、用户可理解错误、恢复、权限、同步影响、依赖/体积和权威文档同时通过后才能标记完成。
