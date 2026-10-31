# todo — 需求池

## F2/F5 侧栏来源与接续增量

- [x] 侧栏横向溢出、搜索、项目折叠和最近活动排序已发布，真实平台目录核验69个历史导入项目/81条会话；见 docs/evidence/ui-capture-20261031/sidebar-release.json。
- [x] 网页已接通X1T DSH真实目录与原DOS会话；统一对话侧栏、设备/项目短标题、子会话缩进和原会话草稿/选择恢复已验证；Winter Shelter超4MiB历史读取与错误分类已修复并上线，发布状态见CURRENT_STAGE。
- [/] Tauri原生DSH接线与本地回执核对已实现并定向验证；真实原DOS单次续聊、390px刷新恢复通过。手机真机、附件、跨设备回执与DSH权威未知回执查询仍待完成；未上线/未打包，详见CURRENT_STAGE及2026-10-31-dsh-mobile-recovery证据。
- [ ] 补充设备视图大量会话收纳与手机真机验收。


- [/] F2/F5 DSH原生会话：Android684cbac真机Winter缓存正文358ms、侧栏34ms且后台刷新不清空、640px缩略图与原图预览、10→20条上翻及固定输入框通过；64MiB/256项/7天阅读缓存、响应式副本持久化及实际not_modified无文件字节已核对。网页本轮未部署。继续补服务端缩略图/正文筛选、冷网性能、结构化成果及不可变版本。见CURRENT_STAGE与2026-11-01-dsh-mobile-network证据。
- [ ] F2/F5 验收：指定旧会话只问进度，核对原session_id不变、无新建会话；发送不确定不自动重派；刷新和断线明确缓存时间。旧合成执行/验证记录需识别并标缺证据，不删除原始数据。


- [/] F2/F3 界面追补：灵感速记、统一强调色、日历时间栏和连接目标标识已发布网页；当前手机真机仍待验，APK范围单独核对，证据见CURRENT_STAGE。
- [ ] F2 设备标识：补可靠友好名、硬件类型、最后观测时间；连接Home与任务执行节点分开，不把目录默认online或凭据存在当作实时执行能力。


> 尚未进入当前施工步序的需求、风险与待判断事项。当前已排期的详细执行步骤见 [docs/CURRENT_STAGE.md](docs/CURRENT_STAGE.md)。

## BP3 / F1–F3 界面与快速捕获增量

- [/] 首屏不被历史导入抢占、客户端恢复、单圆点、灵感项目归属与失败不丢稿：代码及本地真实浏览器通过；网页已发布、手机真机待验。详细状态见 CURRENT_STAGE。
- [ ] 闪念项目选择增加搜索；大量项目时不依赖超长原生列表。
- [/] F1/F3 图片捕获：网页已上线，固定签名 Android 开发包已生成（源码 be04b73 / Run 41174093233）；手机相机/权限拒绝/图库及外部视觉服务质量仍待验。扫码只在设备连接，图片跨 Home 同步另列；详见 CURRENT_STAGE。
- [ ] RV-13 图片延续：原图/关联跨 Home 同步、离线可用性与节点缺图提示；未引用草稿原图的可恢复清理。当前仅保存到当前节点，不能视为跨端完成。
- [ ] 验证项目归属随个人记录在 Replica/Home 之间完整恢复，以及多客户端同时编辑同一会话草稿的冲突处理。

## 待用户裁决

- [x] BOB-D-006设计已获用户确认；真实安全/跨网验证仍待做
- [ ] BOB-D-007 文档权威位置：是否需要在 Google Drive 放只读镜像供跨端 Bot 读取

## 核心待办队列（按里程碑）

### BP0 → BP1（架构与规格定案）
- [x] **WP-01**：静态深度盘点完成，见 [Evidence](docs/evidence/2026-10-28-wp01-deep-inventory.md)；旧测试/现场数据库/全历史secret扫描未运行
- [x] **WP-02**：Schema/预热Outbox设计草案完成，见 [kernel-schema](docs/wiki/kernel-schema.md)；25项内存DDL约束通过；后续已有内核运行实现，见BP2当前证据
- [x] **WP-03**：连接配置与威胁模型设计获批；不代表真实安全测试通过，BOB-D-007镜像选项仍待定

### BP2 → BP3（1.0.1 骨架与个人工作环境 MVP）
- [/] **BP2-01**：搭建 `1.0.1` Rust Workspace（`crates/bob-kernel`）+ Tauri v2（Desktop & Android）+ Vue 3 工程骨架
- [/] **BP2-02**：实现 `BOB-D-008` 懒式休眠、输入框 `@focus`/`@input` 静默预热、单飞 `inFlightWakeupPromise`、本地 `intent_outbox` 先落盘必达与 `not_dispatched` / `unknown_send` 分离对账
- [ ] **BP3-01**：打通 X1C Home 主库 ↔ 手机 Tauri Android Replica 的会话、捕获、待办与远程文件只读浏览（F4）；异地扫码配对与重连须补齐公网通道，不要求长期同一Wi-Fi（当前未实现）
- [ ] **BP3-02**：通过进程边界接入 Harness Runner（DSH / AGY / Codex / Generic API），在统一 UI 中跑通 Task → Execution → Artifact 预览与 Review 闭环

当前证据：16e34c2已在Pixel完成真实扫码、发送、Home只读Codex回复及两端落盘；同request单执行completed/exit0/cleanup=true，用户确认收到回复。成功状态界面0399df1已云端构建并覆盖安装，旧配对和回复仍保留。仅覆盖当前可达Wi-Fi/私网；公网异地、离线恢复、停止和F4仍未验收。

- [/] BP2/BP3：本机只读执行与固定Stop入口；继续完成停止按钮真实操作、窄屏与可用性验收。
- [/] Home认证基础：真实Pixel配对与手机发送/回复落盘已通过当前可达私网；继续离线/重启恢复、撤销、停止操作及公网通道，不把CLI登录当设备配对。

## BP3 紧邻开发队列：真实侧栏与 Online（2026-10-30）

以下按局部实现与完整验收分别标注；详细合同唯一入口为 [CURRENT_STAGE](docs/CURRENT_STAGE.md) 当前施工入口。用户已要求实现后打包到手机，不能用计划或界面夹具标为完成。

- [x] **BP3-01.a**：真实项目/会话目录、新建与切换、原会话迁移保留、逐设备/会话草稿和阅读位置持久化（`0005.sql` + `catalog.rs`，内核与IPC回归通过）。
- [x] **BP3-01.b**：Home 新会话接纳与范围授权、在途回复归属、逐会话轮询/停止，以及原会话有界上下文续聊；保留精确授权检查（内核与原生IPC回归通过）。
- [x] **BP3-01.c**：侧栏接真实数据；设置首页分组与设备子页，顶栏设备图标直达；通用偏好持久化和真实版本信息（32项 Node 测试、Vite 构建与 Playwright 5视口浏览器测试通过）。
- [/] **BP3-01.d**：内核/IPC/传输/浏览器检查及 GitHub 固定签名 Android 包（run `40937679735` / artifact `12926235871` / APK SHA256 `d57237f3b97b7eb6306e4482f2fad727b04e169e11832e02fe7579a28555f273`，16KB ELF PASS）均已构建并下载校验通过；后续已有 Pixel 多会话及 db9a9de IPC 核验，见既有 Evidence；当前未提交增量的包与完整真机旅程仍待核对。
- [/] **BP3-02.a**：已完成旧 Online 兼容 Provider 内核状态机与单测（`online_provider.rs`，含只读会话目录缓存、`native_resume_verified` vs `bounded_history_context` 诚实降级、活跃写入禁止并发抢占）；待连接真实 Online 端点验证。
- [ ] **BP3-01.e / BP3-02.b**：公网传输与 Online 往返、断线对账、授权失效及PC离线行为验证；需要部署时提交具体对象与回退，不能假称当前私网验收已覆盖。
- [ ] **BP4/F6 决策前置**：若要求PC关机后App仍经云端处理，评估常开Home的主权威迁移/恢复/撤销；保留现有BOB-D-004，不能默认为两主或自动切换。
- [/] **F1 直接模型调用（BYOK）与已配对手机离线直连幂等汇合**（代码与本地 Loopback 测试已完成；真实 Key 真机验收、历史检索、附件/图片未完成，见 RV-11 / RV-21）：已完成 `0006.sql`、`direct_provider.rs`、`direct_llm.rs`、系统安全存储凭据槽位（Windows DPAPI / Android Keystore）、OpenAI/Anthropic/Gemini 三协议适配、设置子页「直接模型 API」与执行路径切换器，以及已配对手机离线直连后的 `HomeRequest::ReplicaDirectTurn` 双向 TLS 幂等汇入（前序报告记录81项 `bob-kernel`、24项 `bob-transport`、31项 `bob-app` 原生测试、33项 Node 与5视口；不代表本轮未提交增量已复验）；真实供应商两轮和当前增量真机验收待补；已安装历史包不能覆盖本轮增量。
- [/] **F3 个人助理内核能力与旧能力逐项对照**（已完成 `0006.sql` + `0007.sql`、`personal_ops.rs` 状态机、Replica→Home 个人数据同步 `ReplicaPersonalOp`、决定记录 IPC/UI 与 5 视口回归）：已在 `docs/COMPONENT_MAP.md` §6 建立旧 App（0.9.30）与旧 Online（cloud-agent）逐项能力继承与验收表，并在 `crates/bob-kernel/src/personal_ops.rs` 实现捕获（`captures`）、显式记忆生命周期（`memories`）、笔记（`notes`）、日程（`schedule_items`）与决定（`decisions`）状态机及回归测试。
- [ ] **F1/F2 设置后续**：按真实能力逐项补在线服务认证、执行许可/位置、数据导出与脱敏诊断；余额/订阅选型仍属后续，避免占位按钮。

当前审计基线为 899b2bb 加未提交增量；最新 Pixel 报告对应 db9a9de，不能覆盖当前 dirty 代码。当前部分能力已实现，未验范围见 CURRENT_STAGE §1。X1T/B4与生产保持原边界。

## 愿景/功能/架构审查修正队列（2026-10-30 审查 RV）

来源：2026-10-30 对 `PROJECT_VISION` / `FEATURES` / `ARCHITECTURE` / 代码 `fbcbe0b` 的只读审查。以下为真正需要完成的任务，**每项须按“验证路径”取得证据后才能勾选**；不得以 UI 夹具通过、计划写入或文档描述代替。优先级：P0 立即修复（已安装版本功能损坏或文档失真）→ P1 → P2 → P3。

### 当前复核状态（2026-10-31）

`[/]` 表示已有代码/局部证据但原条款尚未全部验收，不代表实现被回退。RV-01–04/07 的契约修正保留，Pixel报告主要是原生IPC；完整 UI 操作项待补。RV-05 写入吞错仍存在；RV-06 当前测试会覆写快照；RV-12 当前增量需复查所有可点控件；RV-13 待真实双端与新PeerSync边界；RV-16 待完整字段/表单；RV-17 新建会话不满足原ID保留；RV-18 仅有隐藏暂停与6秒非聊天空闲，尚非完整退避。OL-02–04 为JS/界面部分接线，未完成原生凭据/缓存/现场验收。具体证据、优先级与施工步序统一见 CURRENT_STAGE S0–S4。

- RV-05 / RV-13 / OL-02 / OL-04 增量：默认共享配置不得携带秘密；包级校验先于配置副作用；Online token 不随普通IPC快照回传；实现明确清槽/撤销结果与离线状态。需要凭据跨端转移时先明确产品决定，未批准不隐式开启。用假canary与拒绝路径验证，不读取真实凭据。

### P0-A：前端视图 ↔ 内核契约错位（`fbcbe0b` 引入，Pixel 8 已安装版本受影响）

- [/] **RV-01 日程状态值对齐**
  - 问题：`ScheduleView.vue` 勾选完成发送 `done`，`personal_ops::update_schedule_status` 仅接受 `scheduled|completed|cancelled`，失败被 `.catch(()=>null)` 吞掉。
  - 要做：前端所有状态改用 `completed`（含 `toggleDone`、徽标 `item.status === 'completed'` 判定与 `.done` 样式类）；i18n 文案不变。
  - 验证：① 新增 Rust IPC 测试 `app_schedule_status(id,'done')` 返回错误、`'completed'` 成功并 `version+1`；② Node 测试断言桥接透传 `completed`；③ Playwright 夹具改为按内核真实结构返回（见 RV-06），点击勾选后条目显示“已完成”；④ Pixel 8 真机：新建 1 条待办 → 勾选 → 杀进程重开仍为已完成。

- [/] **RV-02 日程/待办区分字段**
  - 问题：`ScheduleItem` 无 `kind` 字段，前端按 `item.kind === 'event'` 分栏，导致“本周日程”永远为空、全部落入“待办”。
  - 要做（二选一，需在 `kernel-schema.md` 记录决定）：(a) `0007.sql` 为 `schedule_items` 增加 `kind TEXT NOT NULL DEFAULT 'event' CHECK(kind IN('event','todo'))`，`create_schedule_item` 与 IPC 增加 `kind` 参数并回填旧数据；或 (b) 不改表，约定 `end_at IS NULL AND reminder_at IS NULL` 视为待办，前端据此计算。推荐 (a)。
  - 验证：① 迁移测试：旧库升级后已有行 `kind='event'`；② 内核测试：创建 todo/event 各一条，`list_schedule_items` 返回正确 kind；非法 kind 被拒；③ Playwright：两个子页各显示 1 条；④ 真机：两类各建一条，切换子页显示正确。

- [/] **RV-03 时间字段名与单位统一**
  - 问题：前端读 `due_at_ms`，内核返回 `start_at`；`kernel-bridge.mjs` 回退值 `Math.floor(Date.now()/1000)`（秒），而 `src-tauri/src/lib.rs::now()` 为毫秒。
  - 要做：在 `kernel-schema.md` 写明所有 `*_at` 均为 Unix 毫秒；桥接回退改 `Date.now()`；`ScheduleView` 改读 `start_at` / `end_at` / `reminder_at`；`datetime-local` 输入转毫秒。
  - 验证：① Node 测试断言不填时间时 `startAt` 与 `Date.now()` 差值 < 5s 且数量级为 13 位；② Playwright 显示的日期与输入一致；③ 真机新建条目显示当天日期而非 1970 年。

- [/] **RV-04 记忆字段与状态值对齐**
  - 问题：`KnowledgeView` 与设置“工作间”记忆列表读 `mem.summary` / `mem.scope`，内核 `MemoryItem` 为 `body`、无 scope；按钮发送 `confirmed` / `retracted`，内核仅接受 `active|superseded|forgotten`。
  - 要做：显示改为 `mem.body` 与 `mem.state`（状态经 i18n 映射）；按钮改为“启用 (active)”“遗忘 (forgotten)”；`superseded` 仅展示不提供按钮；同步更新 zh-CN / en-US 键。
  - 验证：① 内核测试已有状态机覆盖；新增 IPC 测试 `app_memory_state(id,'confirmed')` 报错；② Playwright 夹具含 `proposed` 记忆，点击启用后显示 active；③ 真机：通过内核 `propose_memory`（测试入口或后续 UI）产生一条，验证启用/遗忘后重开仍保持。

- [/] **RV-05 去除静默吞错，如实显示失败**
  - 问题：`App.vue` 中 `handleCreateSchedule` / `handleUpdateScheduleStatus` / `handleSaveNote` / `handleCreateCapture` / `handleUpdateMemory` / `loadPersonalOps` 等用 `.catch(()=>null)` 吞错，违反“Honest status”。
  - 要做：统一错误通道（例如 `personalFault` ref + 视图顶部 `role="alert"` 提示 + 重试按钮）；错误码经 i18n 显示，不暴露凭据；失败时不做乐观更新。
  - 验证：① Playwright 夹具让 `scheduleStatus` reject，断言出现 alert 且条目状态未变；② `rg "catch\(\(\) => null\)" apps/bob-app/src` 中个人事务与模型中心路径为 0（只读预取类调用如需保留须加注释说明理由）。

- [x] **RV-06 UI 夹具与 Rust 序列化结构的契约测试**
  - 问题：Playwright 全绿但真机损坏，原因是夹具按前端期望编写。
  - 要做：在 `src-tauri` 测试中把 `PersonalSnapshot` / `ScheduleItem` / `NoteItem` / `CaptureItem` / `MemoryItem` 的真实 serde 输出写成 JSON 快照（`apps/bob-app/tests/fixtures/personal_snapshot.json`），`ui-browser.py` 与 Node 测试读取该文件作为夹具；快照字段变化时 Rust 测试失败提醒同步。
  - 验证：故意在 Rust 结构中改名一个字段，确认 Rust 快照测试失败；恢复后全部通过。

- [/] **RV-07 设置“工作间”去掉编造数值**
  - 问题：“会话”统计显示的是 `readiness.saved_messages`（消息数）；目录/节点缺省回退 `'Local SQLite Kernel'` / `'bob-local-node'`。
  - 要做：会话数改为 `catalog.conversations.length`，另列“已存消息”；目录/设备无数据时显示 i18n “未配置/未知”；设备标识改读真实 `device_identity` 公开指纹（新增只读 IPC，若无则显示未知）。
  - 验证：Playwright 在空 catalog 夹具下断言显示“未知”而非英文常量；真机核对设备指纹与“连接”页一致。

### P0-B：文档失真修正

- [x] **RV-08 收窄 CURRENT_STAGE / DEVELOPMENT_ROUTE 的超范围陈述**
  - 要做：删除或改正不存在的函数名（`triage_capture`、`activate_memory`、`supersede_memory`、`forget_memory`、`complete_schedule_item`）；“知识分块检索”“多模态附件”标为未实现；DEVELOPMENT_ROUTE “已补齐 F1”改为“代码与本地测试完成，真机/真实 Key 验收待做”。
  - 验证：`rg -n "triage_capture|activate_memory|supersede_memory|forget_memory|complete_schedule_item" docs` 结果为 0 或仅出现在“未实现”语境；人工复读每处“已完成/已落地”均有对应文件路径或 Evidence。

- [x] **RV-09 FEATURES / ARCHITECTURE 状态与事实对齐**
  - 要做：FEATURES 表格 F1/F2/F3/F4 改为 `partial` 并在各节列出“已有证据 / 未完成验收项”；ARCHITECTURE 末尾“Android Keystore 尚未实现”改为已实现（`android_vault.rs` + `DeviceVaultPlugin.kt`），保留“同用户恶意进程”等限制说明；§4 “UI 收敛”记录实际决定（演进旧 App UI，5 主视图 + 设置 6 子页），必要时新增 `DECISIONS.md` 条目。
  - 验证：三份文档交叉阅读无互相矛盾的完成度表述；`progress.yaml` 同步状态。

- [x] **RV-10 CURRENT_STAGE 瘦身**
  - 要做：当前 66KB，按 `docs/standards/project_documentation_standard.md` 将已完成阶段流水迁入 `docs/evidence/` 或 `CHANGELOG.md`，CURRENT_STAGE 只保留当前施工合同、执行队列与诚实边界（目标 < 15KB）。
  - 验证：文件大小达标；所有被移出内容有链接可追溯；todo / progress 中引用的锚点仍可打开。

### P1：功能真实性与核心验收

- [ ] **RV-11 F1 真机验收（真实 Key）**
  - 要做：在 Pixel 8（未配对状态或路由 `direct_byok`）与 Windows 各填一家真实 Provider Key（建议低价模型，按 model-registry 选择），完成两轮问答；断网时发送一条，确认显示“未发送/本机已存”。
  - 验证：Evidence 文件 `docs/evidence/YYYY-MM-DD-f1-device-acceptance.md` 记录设备、APK SHA256、Provider/模型 ID（不含 Key）、请求/回复截图、断网表现、重启后历史仍在；Key 不出现在日志（`adb logcat` 过滤 Key 前缀为 0）。

- [/] **RV-12 下线或标注虚假控件**
  - 范围：本地模型卡片（HF/CN 切换、自定义下载链接、物理模型目录、“引擎已休眠”）；“牛马/视觉”角色（内核无消费方）；Tavily/TinyFish 工具密钥（无调用方）；模型搜索框硬编码英文占位符。
  - 要做：未实现项置灰并显示 i18n “即将支持”或整体隐藏；角色卡注明“当前仅主力模型生效”；占位符改 `t('modelFilterPlaceholder')`。
  - 验证：Playwright 断言上述按钮 `disabled` 或不存在；`rg "Filter models" apps/bob-app/src` 为 0；人工点击每个可点控件均产生真实效果。

- [/] **RV-13 个人数据 Replica→Home 同步规则**
  - 问题：消息有 `ReplicaDirectTurn` 汇合，但笔记/日程/捕获/记忆在已配对设备上 `acceptance='local'` 后无任何回传路径。
  - 要做：先在 `kernel-schema.md` 设计：捕获/新建笔记/新建日程为追加型（全局 ID，同 ID 同摘要幂等、不同内容冲突）；修改已有对象走 `intent_outbox` + `expected_version`，Home 拒绝过期版本并给出原因；随后实现 `HomeRequest::PersonalOp` 与 Replica 重连汇入。
  - 验证：内核测试覆盖：离线新建→上线汇入、重复投递幂等、版本冲突拒绝、Home 侧 `acceptance` 升级为 `home`；真机：手机离线新建笔记，电脑上线后可见，手机状态由“本机”变为“已同步”。

### P2：架构整洁与体验一致性

- [ ] **RV-14 拆分 App.vue（2842 行为前序审查快照，不作当前验收指标）**
  - 要做：拆出 `views/ChatView.vue`、`components/settings/SettingsModelPanel.vue`、`SettingsWorkspace.vue`、`SettingsTasks.vue`、`SettingsAppearance.vue`、`SettingsAbout.vue`、`ConversationDrawer.vue`；状态经 composable（如 `useModelHub`、`usePersonalOps`、`useCatalog`）共享。
  - 验证：App.vue < 600 行；Node / Playwright 5 视口全绿；行为与截图对比无回归。

- [ ] **RV-15 桌面横屏左侧导航栏 + 设置作为主视图**
  - 要做：`layout-desktop-wide` 下渲染左侧 5 项导航栏、隐藏底部栏；设置由模态框改为与其他 4 项并列的主视图（移动端保持全屏页），保留 Esc/返回键关闭语义与焦点管理。
  - 验证：Playwright 1180×757 断言 `.side-nav` 可见、`.bottom-nav` 隐藏；390×844 反之；键盘 Tab 循环与 Esc 行为测试通过。

- [x] **RV-16 决定记录 IPC 与 UI**
  - 要做：暴露 `app_decision_record` / `app_decision_list` / 状态变更 IPC；在“知识”或“工作”中增加决定列表（理由、备选、证据、重审条件、状态 `proposed|accepted|superseded|rejected`）。
  - 验证：IPC 测试与 Playwright；真机新建一条决定并改为 accepted，重开保持。

- [x] **RV-17 “对话 → Work” 升级入口**
  - 要做：在对话中提供“升级为 Work”动作，创建/关联 Project 与 Task，保留原对话、材料、决定引用；不扩大权限。
  - 验证：内核测试：升级后原 conversation ID 不变且被 Task 引用；UI 中 Work 视图可见并能返回原对话。

- [x] **RV-18 轮询改为退避/事件驱动**
  - 问题：每 1.5s `reload()` 且每次触发 `syncHome()`，Android 耗电，Home 离线时持续重试。
  - 要做：前台活跃会话 1.5s，无在途执行时退避至 10–30s，页面隐藏（`visibilitychange`）暂停；`syncHome` 失败指数退避；后续评估 Tauri 事件推送。
  - 验证：Node 测试模拟定时器断言退避序列；真机 `adb shell dumpsys batterystats` 或日志计数对比改前后 10 分钟内请求次数。

- [ ] **RV-19 提醒通知通道**
  - 要做：`reminder_at` 到期时 Windows Toast / Android 通知（需评估 Android 后台限制，关联“产品缺口 G14”）。
  - 验证：设置 2 分钟后提醒，桌面与 Pixel 均收到通知；应用被杀后行为如实记录。

### P3：度量与后续

- [x] **RV-20 北极星指标本地基线**
  - 要做：本地记录（不上传）续接成功/重复执行次数、首次对话所需配置步数、unknown 状态停留时长、成果找回次数；在“关于与诊断”展示。
  - 验证：指标表有 schema 与测试；诊断页可查看与清空。

- [ ] **RV-21 F1 历史检索与附件/图片**
  - 要做：会话内/全局消息搜索（SQLite FTS5）；图片与文件附件（受平台权限约束），三协议适配多模态。
  - 验证：搜索命中测试；真机发送图片得到视觉模型回复并在历史中可回看。

## 一套代码三端运行（手机 / 电脑 / 网页服务器）差距与施工队列（2026-10-30 审查 XP）

来源：用户追问 BOB-D-005“同一 Rust 内核嵌入 App 或运行于常开节点”及“一套代码覆盖手机、电脑、网页服务器端”的实现程度。基于代码 `fbcbe0b` 只读核对。

### 现状核对（事实）

| 端 | 现状 | 判定 |
|---|---|---|
| Android 手机 | Tauri Android + 同一 Vue 前端 + 同一 Rust 内核，已在 Pixel 8 运行 | ✅ 已实现 |
| Windows 电脑 | Tauri Desktop，同一前端与内核；DPAPI 密钥、Windows Job 只读 Runner | ✅ 已实现（仅 Windows） |
| macOS / Linux 电脑 | `device_vault.rs` 在非 Windows 返回 `VaultError::Unsupported`；`runner.rs` 依赖 Windows Job（26 处 Windows 专属） | ❌ 未实现 |
| 云端节点 `agent.bobbik.org`（服务器端） | 由旧 Online（`cloud-agent`，Node/TypeScript + 独立 Vue UI `packages/ui`）承担，与新 Rust 内核是**两套代码**；新 Rust 内核没有 headless 二进制（`bob-node`） | ⚠️ 有服务、非同一代码 |
| 新 App ↔ 云端节点接入 | 已有 online-client.mjs PKCE/只读HTTP封装和Work面板；原生凭据槽已有增量，但普通快照回传token、持久缓存/分页及真实端点未闭环 | 部分实现；OL-01–04仍待验 |
| 网页端（浏览器访问新 Bob 前端） | 新前端 `native-bridge.mjs` 仅支持 Tauri；浏览器访问的是旧 Online 自己的 UI | ❌ 未统一 |

**有利基础**：`bob-kernel` / `bob-transport` 只依赖 rusqlite / serde / sha2 / rustls，不依赖 Tauri，可直接编译到 Linux 服务器；前端桥接 `createKernelBridge(invoke, isAvailable)` 已是可注入设计，可替换为 HTTP 适配器；`home_host.rs`、`home_config.rs`、`device_vault.rs` 本身不引用 Tauri。

**主要障碍**：约 160KB 业务逻辑与 Tauri `AppHandle` 耦合（`direct_llm.rs` 35 处、`pairing_commands.rs` 50 处、`lib.rs` 48 处、`device_identity.rs` 10 处）；平台能力（密钥、HTTP、进程隔离、扫码、通知）没有抽象成 trait；桌面 HTTPS 通过调用 `curl` 子进程实现；`apps/bob-app/src-tauri` 被排除在 workspace 外；CI 仅有 `android-dev.yml`。

**完成度依据**：按 CURRENT_STAGE 的能力/证据矩阵判断，不用无验收分母的百分比。以上旧端点能力仅为已有文档/源码记录，本次未探测生产。

**两条轨道**（本地源码核对，生产实际部署版本与端点能力尚未只读核验）：
- **近期轨道 OL**：新 App 通过云端已有 `/api/client/v1` 接入 `agent.bobbik.org`，先只读、后补写入；云端仍是 Node 实现。
- **长期轨道 XP**：把 Rust 内核做成可在云端无界面运行的 `bob-node`，并让浏览器复用新前端，逐步替代旧 Online 的代码（BOB-D-005 单一内核目标）。

### 近期轨道 OL：新 App 接入 `agent.bobbik.org`（复用云端已有能力）

- [x] **OL-01 只读核验生产端点能力（需用户批准对外请求）**
  - 要做：从 `service_map.md` 取地址；仅调用无副作用接口核对实际部署版本是否包含 `/api/client/v1`、`GET /capabilities` 返回的协议版本与能力位、反向代理是否额外要求网页登录层（Cookie/Basic）；记录与本地源码差异。不触碰生产任务、不创建项目。
  - 验证：Evidence `docs/evidence/YYYY-MM-DD-online-client-api-probe.md` 记录请求路径、状态码、能力位与时间；不含任何凭据。

- [x] **OL-02 Rust 端 Online 客户端（设备码配对 + Bearer）**
  - 要做：在 `bob-runtime`（或暂在 src-tauri，后随 XP-02 下沉）实现 `bob-core-client/1.0` 客户端：生成 verifier/challenge → `POST /pairings` → 在 App 中显示 `user_code` 与“在浏览器打开 /client-connect”按钮 → 轮询 `/exchange`（遵守 `interval_ms`、202 继续、过期/失败计数）→ 凭据存入系统安全存储（新槽位 `online-credential-<server_instance_id>`）；处理 401（失效重配）、403（无授权项目）、429/503（退避，`retryable`）；支持 `credentials/self/revoke`。
  - 验证：用本地 `cloud-agent` 测试服务（`NODE_ENV=test` loopback）跑通配对→读取→吊销全流程的集成测试；凭据不出现在 SQLite、日志、Outbox；24h 过期后 UI 显示“需重新连接”而非空列表。

- [x] **OL-03 只读目录接入侧栏与“工作”视图**
  - 要做：把 `/projects`、`/projects/:id/tasks`、`/projects/:id/messages?after=`（游标增量）、`/tasks/:id/snapshot` 写入 `online_provider` 缓存（保留 `source=online`、`server_instance_id`、远端原始 ID 与版本）；侧栏与 WorkView 增加“在线工作服务”来源分组，标注只读、缓存时间与离线标识；同名本地/Online 项目不合并。
  - 验证：内核测试覆盖游标增量、重复拉取幂等、`has_more` 分页、读取失败不清空缓存；Playwright 夹具改为真实响应快照（同 RV-06 原则）；真机连接后看到授权项目与历史消息，断网仍可浏览缓存并显示“离线/缓存于 …”。

- [x] **OL-04 设置 → 在线工作服务页**
  - 要做：显示服务地址、连接状态、`principal_id`、授权项目、凭据到期时间、能力位（只把 true 的能力显示为可用）；提供连接/重新连接/断开（吊销）；与“设备与连接”（PC 配对）严格分开，Online 可达不点亮 PC 在线。
  - 验证：Playwright 覆盖未连接/已连接/过期/吊销四态；真机吊销后网页端 `/api/work/client-authorizations` 列表中对应凭据状态变更。

- [x] **OL-05 云端补齐写入能力（已在 `bob-node` `/api/client/v1/operations` 实现并部署 `bob-online.bobbik.org`）**
  - 要做：为 `/api/client/v1/operations` 设计并实现 `submit`（向指定 project/task 追加指令，携带幂等 `request_id` 与 `parent_revision`）、`query`（按 request_id 查结果，用于 unknown 对账）、`cancel`；授权新增 `work.write` scope，由网页端管理员显式批准；能力位随实现逐项置 true。
  - 验证：cloud-agent 侧测试：同 request_id 重放返回同一结果不重复执行；无 `work.write` 返回 403；query 能区分 received/running/completed/unknown；部署按 cloud-agent 合同记录回退方案。

- [x] **OL-06 新 App 受控续聊（依赖 OL-05）**
  - 要做：按 connectivity §8：意图先经本机 Outbox（已配对时经 Home 接纳）再由 Online Provider 调用 `submit`；状态映射为已发送/对方已收到/执行中/完成/未知；未知先 `query`，禁止盲目重派；能力位为 false 时按钮不出现。
  - 验证：专用测试项目两轮续聊、断网中途对账、凭据失效、重复点击只产生一次远端执行；Evidence 记录 request_id 与远端回执。

- [x] **OL-07 云端节点作为 DSH 等 Harness 的入口评估**
  - 要做：云端现有 `/api/node-work/*`（DSH Web 会话：connect/list/history/prompt，需 `BOB_NODE_WORK_ENABLED=1`、同源与 `x-bob-node-work` 头）只面向其自身网页同源调用，不对外部客户端开放；评估是否经 `/api/client/v1` 暴露等价能力，或保持仅网页可用并在 App 中提供“在浏览器中打开”跳转。
  - 验证：决定写入 DECISIONS；若暴露，复用 OL-05 的 scope 与幂等规则并有对应测试。


### 长期轨道 XP：同一 Rust 内核运行于云端节点并统一前端（按依赖顺序）

- [x] **XP-01 平台能力抽象（Platform traits）**
  - 要做：在新 crate `crates/bob-runtime` 定义 `SecretStore`（put/get/clear 槽位）、`HttpClient`（带超时与大小上限的 HTTPS 请求）、`ProcessSandbox`（启动/取消/清理确认）、`Notifier`、`Clock`、`DeviceIdentityStore` trait；现有实现改为 trait 实现：Windows DPAPI、Android Keystore（经 Tauri 插件适配）、Loopback 测试实现。
  - 验证：`cargo test -p bob-runtime` 用内存/Loopback 实现覆盖每个 trait 的成功、超时、失败与“未支持须明确报错不降级明文”；`rg "tauri" crates/bob-runtime` 为 0。

- [ ] **XP-02 业务逻辑下沉，src-tauri 只做薄外壳**
  - 要做：把 `direct_llm.rs`（三协议组装/解析/执行流程）、`pairing_commands.rs` 的配对状态机、`home_host.rs`、`home_link.rs`、`home_config.rs`、`device_identity.rs` 的非 UI 部分迁入 `bob-runtime`（或 `bob-host`），仅依赖 XP-01 trait；`src-tauri/src/lib.rs` 只保留 `#[tauri::command]` 参数转换与平台实现注入。
  - 验证：业务依赖不再绑定 Tauri；命令壳只做参数/授权上下文转换和依赖注入（行数仅为维护提示）；原有 31 项 src-tauri 测试迁移到 `bob-runtime` 后仍通过；Android APK 与 Windows 构建行为不变（复跑 Pixel 8 发送/回复、直连模型冒烟）。

- [ ] **XP-03 跨平台 HTTPS 客户端替换 curl 子进程**
  - 要做：桌面与服务器使用 Rust 原生 rustls HTTPS 客户端（例如 `ureq`/`reqwest` + rustls + webpki-roots/系统根证书，按依赖审查后选定并锁版本）实现 `HttpClient`；Android 继续 `HttpURLConnection` 或统一到同一 Rust 客户端（需验证系统 CA 与代理）。
  - 验证：Loopback 三协议端到端测试在 Windows/Linux（CI）均通过；无 `curl` 可执行文件的环境仍可直连模型；密钥不出现在进程参数（`argv`）与日志中。

- [x] **XP-04 Linux 服务器 0600 密钥存储（`FileVaultSecretStore`）与进程边界**
  - 要做：`SecretStore` 增加 macOS Keychain、Linux Secret Service（libsecret）实现；无桌面密钥环的服务器提供“受文件权限 0600 + 主密钥来自环境/systemd credential 的加密文件”实现，并在 threat-model 记录强度差异；`ProcessSandbox` 增加 Linux（进程组/cgroup 或 systemd-run scope）实现，保持“清理未确认 = unknown 且占用”的语义。
  - 验证：CI 矩阵 `windows-latest` / `ubuntu-latest`（macOS 视成本）运行 `bob-runtime` 测试；Linux 上取消执行后确认子进程树全部消失的集成测试通过；无可用后端时明确 `Unsupported` 而非明文回退。

- [x] **XP-05 Workspace 与 CI 统一**
  - 要做：将 `apps/bob-app/src-tauri` 纳入根 workspace（或单独 workspace 但共享 `Cargo.lock` 策略说明），新增 `crates/bob-node` 成员；GitHub Actions 新增 `ci.yml`：`cargo test --workspace`（Windows + Linux）、Node 测试、Vite 构建、Playwright；保留 `android-dev.yml`。缓存/target 仍按工作区规则放在仓库外。
  - 验证：一次 push 触发后各平台 job 全绿；本地 `cargo test --workspace` 一条命令覆盖全部 Rust 测试。

- [x] **XP-06 `bob-node` 无界面常开节点（已部署于 Hostinger VPS2 `bob-online.bobbik.org`）**
  - 要做：新增 `crates/bob-node` 二进制，先与旧 Online 在同一 VPS 上并行运行（不同端口/子域，不共享数据库）：加载同一 `bob-kernel` + `bob-runtime`，以 Home 或执行节点角色运行；提供 Home 双向 TLS 监听（复用 `bob-transport`）、配置文件（端口、数据目录、角色，不含个人域名/IP 硬编码）、systemd unit 样例与优雅退出/重启恢复；首轮仅支持 Linux x86_64。
  - 验证：Linux 容器内启动 → Windows/Android 客户端完成配对与一次直连模型或只读执行往返；`kill -9` 后重启，未完成执行显示 unknown、不重放模型（复用现有恢复测试语义）；数据目录权限 0700；按 BOB-D-004 不自动迁移 Home，角色切换须显式命令。

- [x] **XP-07 Web 适配层：浏览器复用同一 Vue 前端（`http-bridge.mjs` + `bob-node` `/api/rpc/*`）**
  - 要做：`bob-node` 暴露受认证的 HTTPS JSON API（与 Tauri command 一一对应，复用同一 serde 结构）+ 事件推送（SSE/WebSocket，用于执行状态，替代 1.5s 轮询）；前端新增 `http-bridge.mjs`：`createKernelBridge(httpInvoke, () => true)`，按运行环境（Tauri / 浏览器）自动选择；浏览器端不持有模型 Key，模型调用在节点侧执行（凭据留在节点边缘）。
  - 验证：同一 `dist/` 在浏览器打开节点地址后可完成对话、日程、知识、设置操作；契约测试遍历 `kernel-bridge.mjs` 每个方法，断言 Tauri 与 HTTP 两种适配器发送的命令名与参数结构一致；Playwright 5 视口在“真实 bob-node + 浏览器”模式下运行（不再只用夹具）。

- [x] **XP-08 Web 认证与授权边界（Caddy `common_auth` 保护浏览器入口，`/api/client/v1/*` 设备 Bearer 鉴权）**
  - 要做：浏览器无法使用设备 mTLS 证书，需设计“已配对设备扫码授权网页会话”（短期会话令牌、可吊销、绑定来源/有效期）或 Passkey；网页会话与设备配对、操作授权、Provider 认证仍三域分离；CSRF/CORS/速率限制；更新 `threat-model.md` 与 `connectivity.md`。
  - 验证：未登录访问所有 API 返回 401；吊销后旧令牌立即失效；网页会话不能读取任何 Key 明文；渗透清单（重放、跨站、令牌泄露）逐项记录。

- [x] **XP-09 部署形态与旧 Online（`cloud-agent`）只读幂等导入（69 个项目已导入 `bob-online.bobbik.org`）**
  - 要做：明确三种部署：①个人 PC 内嵌 Home（默认）；②云端 VPS 运行 `bob-node` 作为执行节点；③（需决定）`bob-node` 作为 Home。旧 Online 在 BP5 前继续生产，不双写同一对象。逐项盘点 `cloud-agent` 服务端能力的去留：Harness 适配器（`agy/codex/dsh/pi-agent-adapter.ts`、`ssh-adapter.ts`、`local-adapter.ts`）、`verification.ts`/`acceptance-reviewer.ts`、`preview.ts`、`pty-manager.ts`、`handoff.ts`、Lab；每项裁决“用 Rust 重写进 bob-node / 保留为独立 Harness 进程经窄接口调用 / 弃用”，并在 COMPONENT_MAP 记录。旧 Online 网页 UI（`packages/ui`）中有价值的视图（ProjectStudio、ExecutionRecords、ReviewModal 等）迁入新前端。制定替代里程碑与回退方案，写入 DEVELOPMENT_ROUTE 与 DECISIONS（新条目）。
  - 验证：部署文档在干净 Linux 上由非作者按步骤完成安装；替代前旧 Online 数据只读可查；回退步骤演练一次并记录 Evidence。

- [x] **XP-10 前端多端自适应收尾（能力清单驱动 + 5 视口浏览器与 `bob-node` E2E 通过）**
  - 要做：浏览器环境隐藏仅原生可用的能力（扫码、系统安全存储提示、本机 CLI），按“能力真实可用才出现”的原则由后端返回能力清单驱动显示；配合 RV-15 的桌面左侧导航、移动端底部导航，使同一前端在手机竖屏、电脑横屏、浏览器三种环境一致。
  - 验证：Playwright 在 Tauri 夹具与 HTTP 模式两套运行；能力清单缺项时对应入口不渲染。

### 推荐顺序与里程碑

0. **里程碑 M0（先接上云端节点，只读）**：OL-01 → OL-02 → OL-03 → OL-04。不依赖 XP，可最先交付“手机/电脑 App 里看到 agent.bobbik.org 的项目与历史”；之后 OL-05 → OL-06 补写入。
1. **里程碑 M1（内核外壳分离）**：XP-01 → XP-02 → XP-03 → XP-05。完成后手机/电脑仍是一套代码，且业务逻辑不再锁在 Tauri 中。
2. **里程碑 M2（服务器节点）**：XP-04（Linux 部分）→ XP-06。完成后同一内核可在 VPS 无界面运行。
3. **里程碑 M3（网页端）**：XP-08（认证与授权先行）→ XP-07 → XP-10。完成后浏览器复用同一前端访问节点。
4. **里程碑 M4（替代旧 Online 代码）**：XP-09，依赖 BP5 数据迁移；完成后 `agent.bobbik.org` 运行的是同一 Rust 内核与同一前端。

> 依赖说明：OL-02 先在宿主使用可注入 HTTP/安全存储接缝，随后随 XP-01 下沉，不等待完整重构；OL-05 在 `cloud-agent` 仓库施工，需按该项目合同与部署批准执行；XP-02 与 RV-14（拆分 App.vue）可并行；XP-07 的事件推送可同时解决 RV-18 的轮询耗电问题；RV-13（个人数据同步）应先于 XP-06 投入生产使用，否则服务器节点上的笔记/日程同样无法汇合。

## 同项目多设备与可验证交接（沿 BP3/BP4/F6）

来源：2026-10-30用户关于设备/任务分组、关机与GitHub未同步的疑问。已有目录与内核交接实现，实际文件传输/跨节点验收待补；不表示已批准云端Home迁移或自动上传。具体任务卡见CURRENT_STAGE，详细规则见docs/wiki/kernel-schema.md。

- [x] **BP3-01.a/c 增量**：同一真实目录支持按项目/任务、按执行设备分组；保留未绑定设备的普通对话。离线灰化设备/执行能力，不禁用已缓存历史和草稿；显示等待或版本待核对的具体原因（内核与浏览器5视口验证通过）。
- [/] **BP4-01**：同Project多Workspace只读盘点；记录commit/分支、dirty、未push、未跟踪与必要素材、观测时间；不凭仓库URL/标题自动合并项目（`workspace_handoff.rs` 内核回归通过）。
- [/] **BP4-02**：显式交接检查点、受控传输、目标校验与写入权交接；`fencing_token` 原子递增，保留原生会话/上下文与父成果来源；不靠超时抢占unknown执行（`workspace_handoff.rs` 内核回归通过）。
- [/] **BP4-03**：验证忘记push后关机、同commit但dirty不同、缺LFS/素材/子模块、目标分叉、同步中断、源节点恢复；跨端查看和继续均不丢数据、不重复执行（`multi_session_and_handoff.rs` 5大故障场景通过）。
- [/] **BP4/F6 运行策略**：BOB-D-012已确认每个实例独立运行，共享对象有owner/source/version；Home迁移不是单实例或默认助理前置。继续补对象级共享/交接、owner离线边界和旧owner恢复防双写；不自动迁移数据或commit/push。
- [ ] **BP4-04**：最小项目画像胶囊：待实施；字段与验收见kernel-schema §10、CURRENT_STAGE §3.5。
- [ ] **BP4-05**：有界API子代理：排在单执行者成果闭环之后；见CURRENT_STAGE §3.8。
- [/] **BP4-06**：网页默认助理持久入口、真实目录关联、原文派发与request/turn绑定回流已真实验证并发布（assistant-instance-20261031）；重复检查、刷新和候选重启不重派。继续补原生App接线、任务卡去除路由元语句并保留原文、标题与实际工作锚点核对、超窗/未知回执、远端取消、单执行者V1/V2及对象级共享。公网独立登录与手机真机未验收；见CURRENT_STAGE及evidence/2026-10-31-assistant-instance.md。
- [ ] **BP4-07**：能力登记、凭据保护审计、API出口与受控渲染分段验收；见CURRENT_STAGE §3.7。

GitHub中的检出副本通常可关联原项目，但不自动包含未传出的工作或开发环境。版本未知时默认等待核对；用户明确选旧commit时可设计隔离候选路径，不能伪称恢复最新版本。当前不新增节点、不加载策略、不恢复X1T/B4、不部署。

## 前身项目需要修正（只记录，不在本仓库动手；需要时到对应项目执行）

- [ ] 旧 Online 统一提案 §2 中“worktree 注册项可 prune”的表述应改为“禁止 prune，其中有 2707 项未提交改动（已备份）”
- [ ] 旧 Online ONL-D-005 仍写 VPS3 为 Core，实际生产在 Hostinger，决策已过期
- [ ] 旧 App `docs/contracts/bob_cloud_agent_interchange.yaml` 应标为 superseded

## 开源前提（BP6，日常开发中就要避免返工）

- [ ] 选定许可证并补齐 LICENSE；审计依赖许可证
- [ ] 去个人化：域名、IP、节点名全部改为外部 Provider 配置注入
- [ ] 包名与商标：旧 Online 的 `antigravity-nexus` 不能沿用
- [/] LLM Key 已有 Windows DPAPI / Android Keystore；复核共享配置导出、IPC暴露、撤销及跨平台后端，未获批准不自动跨设备同步秘密
- [ ] 复用并修订旧 App 的 `OPEN_SOURCE_WORKFLOW.md`
- [ ] Git 历史 secrets 扫描结论与微信网关合规评估

## 产品缺口（统一审查 G14）

- [ ] 没有服务器时手机如何收到推送和提醒（Android 后台限制）
- [ ] BYOK 费用可见性与预算上限
- [ ] 两套 UI 的 i18n 键对齐（严禁硬编码中文字符串与硬编码颜色）
- [ ] 旧 App 0.9.x 已安装用户的数据迁移（`bob.db` → `1.0.1` Home 库）
- [ ] 遥测和崩溃上报的隐私策略
- [/] AGY / Codex / DSH接入策略按BOB-D-011确认；供应商许可、认证方式与版本适用性仍需分别记录官方来源和核查日期，不宣称100%合规。
