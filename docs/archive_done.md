# Bob — Completed Backlog Archive (`archive_done.md`)

> 本文件承接从 `todo.md` 归档的已完成事项（`[x]`）及已完成阶段的历史差距核对记录，遵循 `docs/standards/project_documentation_standard.md` §3.7 & §3.14。
> 已获得的产品能力登记于 [FEATURES.md](FEATURES.md)，当前施工合同见 [CURRENT_STAGE.md](CURRENT_STAGE.md)，具体验证事实见 [evidence/](evidence/)。

---

## 归档时间: 2026-11-02

### F2/F5 侧栏来源与接续（已完成部分）
- [x] **侧栏横向溢出、搜索、项目折叠和最近活动排序已发布**：真实平台目录核验 69 个历史导入项目 / 81 条会话；见 `docs/evidence/ui-capture-20261031/sidebar-release.json`。
- [x] **网页接通 X1T DSH 真实目录与原 DOS 会话**：统一对话侧栏、设备/项目短标题、子会话缩进和原会话草稿/选择恢复已验证；Winter Shelter 超 4MiB 历史读取与错误分类已修复并上线。
- [x] **BOB-D-006 连接分层设计确认**：设计已获用户确认（真实安全/跨网验证留待后续任务）。

### BP0 → BP1（架构与规格定案）
- [x] **WP-01**：静态深度盘点完成，见 `docs/evidence/2026-10-28-wp01-deep-inventory.md`。
- [x] **WP-02**：Schema / 预热 Outbox 设计完成，见 `docs/wiki/kernel-schema.md`；25 项内存 DDL 约束通过。
- [x] **WP-03**：连接配置与威胁模型设计获批（见 `docs/wiki/threat-model.md`）。

### BP3 真实侧栏与目录基座（已完成项）
- [x] **BP3-01.a**：真实项目/会话目录、新建与切换、原会话迁移保留、逐设备/会话草稿和阅读位置持久化（`0005.sql` + `catalog.rs`，内核与 IPC 回归通过）。
- [x] **BP3-01.b**：Home 新会话接纳与范围授权、在途回复归属、逐会话轮询/停止，以及原会话有界上下文续聊（内核与原生 IPC 回归通过）。
- [x] **BP3-01.c**：侧栏接真实数据；设置首页分组与设备子页，顶栏设备图标直达；通用偏好持久化和真实版本信息（32 项 Node 测试、Vite 构建与 Playwright 5 视口浏览器测试通过）。
- [x] **BP3-01.a/c 增量**：同一真实目录支持按项目/任务、按执行设备分组；保留未绑定设备的普通对话。离线灰化设备/执行能力，不禁用已缓存历史和草稿（内核与浏览器 5 视口验证通过）。

### 愿景/功能/架构审查修正队列（RV 已完成项）
- [x] **RV-06 UI 夹具与 Rust 序列化结构的契约测试**：将 `PersonalSnapshot` / `ScheduleItem` / `NoteItem` / `CaptureItem` / `MemoryItem` 真实 serde 输出写成 JSON 快照（`apps/bob-app/tests/fixtures/personal_snapshot.json`），供浏览器与 Node 测试共用。
- [x] **RV-08 收窄 CURRENT_STAGE / DEVELOPMENT_ROUTE 的超范围陈述**：清理未实现函数名引用，明确区分代码完成与真机验收。
- [x] **RV-09 FEATURES / ARCHITECTURE 状态与事实对齐**：F1–F6 状态改为 `partial` 并分列已有证据与未完成验收项；补齐 Android Keystore 实现记录与 5 主视图 + 设置 6 子页决定。
- [x] **RV-10 CURRENT_STAGE 瘦身**：将历史阶段流水迁入 `docs/evidence/`，使 `CURRENT_STAGE.md` 聚焦当前施工合同与诚实边界。
- [x] **RV-16 决定记录 IPC 与 UI**：暴露 `app_decision_record` / `app_decision_list` / 状态变更 IPC 与前端决定列表。
- [x] **RV-17 “对话 → Work” 升级入口**：在对话中提供升级为 Work 动作，创建/关联 Project 与 Task 并保留原 conversation ID。
- [x] **RV-18 轮询改为退避/事件驱动**：前台活跃会话 1.5s，无在途执行退避至 10–30s，`visibilitychange` 页面隐藏暂停，`syncHome` 失败指数退避。
- [x] **RV-20 北极星指标本地基线**：本地记录续接成功/重复执行次数、首次对话配置步数、unknown 状态停留时长、成果找回次数，并在“关于与诊断”展示。

### 近期轨道 OL：新 App 接入在线工作服务（已完成项）
- [x] **OL-01 只读核验生产端点能力**：核对 `/api/client/v1` 与 `/capabilities`。
- [x] **OL-02 Rust 端 Online 客户端（设备码配对 + Bearer）**：实现 PKCE / 设备码轮询与系统安全存储凭据槽位 `online-credential-<server_instance_id>`。
- [x] **OL-03 只读目录接入侧栏与“工作”视图**：接入 `/projects`、`/tasks`、游标增量消息与快照缓存。
- [x] **OL-04 设置 → 在线工作服务页**：展示服务地址、连接状态、授权项目、能力位及吊销操作。
- [x] **OL-05 云端补齐写入能力**：在 `bob-node` `/api/client/v1/operations` 实现 `submit`/`query`/`cancel` 并部署 `bob-online.bobbik.org`。
- [x] **OL-06 新 App 受控续聊**：经 Outbox 与 Online Provider 幂等提交与对账。
- [x] **OL-07 云端节点作为 DSH 等 Harness 的入口评估**：评估并落盘于 `DECISIONS.md`。

### 长期轨道 XP：同一 Rust 内核运行于云端节点并统一前端（已完成项）
- [x] **XP-01 平台能力抽象（Platform traits）**：在 `crates/bob-runtime` 定义 `SecretStore`、`HttpClient`、`ProcessSandbox`、`Notifier`、`Clock`、`DeviceIdentityStore`。
- [x] **XP-04 Linux 服务器 0600 密钥存储（`FileVaultSecretStore`）与进程边界**：实现 `v2:` 随机盐+nonce+MAC 防篡改密钥库与进程清理确认。
- [x] **XP-05 Workspace 与 CI 统一**：统一 Cargo workspace 与 GitHub Actions CI / `android-dev.yml`。
- [x] **XP-06 `bob-node` 无界面常开节点**：新增 `crates/bob-node` 并部署于 Hostinger VPS2 (`bob-online.bobbik.org`)。
- [x] **XP-07 Web 适配层（浏览器复用同一 Vue 前端）**：实现 `http-bridge.mjs` + `bob-node` `/api/rpc/*`，同一前端覆盖 Tauri 与 Web。
- [x] **XP-08 Web 认证与授权边界**：Caddy `common_auth` 保护浏览器入口，`/api/client/v1/*` 设备 Bearer 鉴权。
- [x] **XP-09 部署形态与旧 Online（`cloud-agent`）只读幂等导入**：69 个项目与 81 条历史会话已只读幂等导入 `bob-online.bobbik.org`。
- [x] **XP-10 前端多端自适应收尾**：能力清单驱动渲染 + 5 视口浏览器与 `bob-node` E2E 通过。

### 已实现代码的阶段里程碑基线（留待真机或跨端终验的已交付底座）
- **F1/F3 票据卡夹、日程/便签删除、主/Flash 模型收敛（DeepSeek 单模型 `v4.1-flash-1017`）与 `model-scout` 巡检技能**：代码、单测、VPS2 线上验证及 Android APK（Run `41328042697` / commit `d740979`）构建完成。
- **BP4-04 项目画像胶囊（`project_context.rs` / `workspace_context.rs`）**：本地宿主授权、5 秒来源变化刷新、`SourceCategory` 分类、`SymbolSkeletonItem` 符号骨架、TaskCard 装配及 23 项 Tauri IPC 挂载已完成。
- **BP4-05 有界 API 子代理（`subagent_pool.rs`）**：1 架构师 + 1 只读子代理、深度=1、并发=1、只读沙箱、重启防重放对账与结构化结论收敛已完成（见 `docs/evidence/BP4_05_SUBAGENT_POOL_VERIFICATION.md`）。
- **BP4-06 原生 Claude 适配器与 X1T 实机 V1→V2 续接底座（`claude_harness.rs`）**：离线 Fixture 闭环与 X1T 实机 `--resume` V1→V2 快照验证已完成（见 `docs/evidence/BP4_06_CLAUDE_HARNESS_VERIFICATION.md`）。
- **BP4-07 节点能力登记、海外模型出口与 GPU 渲染借用内核（`capability_borrowing.rs`）**：内核与节点回归已通过。

---

## 六、 Phase A / B / C 渐进开发首批落地归档（2026-11-02）

- [x] **Phase A Step A1 — 成果预览分层返回与手机安全区 (`DshFilePreview.vue`, `DshSessions.vue`)**：左上角新增显式返回按钮并注入 `env(safe-area-inset-top/bottom)` 避开手机状态栏；接入 `history.pushState` 与 `popstate` 监听，实现“图片放大 (`zoom`) $\rightarrow$ 成果预览 (`file`) $\rightarrow$ 原会话”逐层退栈（不直接退出 Bob）；关闭预览后精确恢复 `reader.scrollTop` 且不重载会话。
- [x] **Phase A Step A2 — 旧会话缓存秒开与后台增量同步状态机 (`DshSessions.vue`)**：进入旧会话立即恢复缓存正文与已缓存预览（首次进入定位底部最新消息）；角落状态机统一流转 `正在检查更新…` $\rightarrow$ `正在同步…`（仅增量合并变化记录） $\rightarrow$ `已同步最新记录`（1.8s 自动消隐），断网时显示 `暂未连接，正在显示上次记录` 并完整保留正文与草稿；上滑阅读历史时收到新消息显示 `↓ 有新消息` 浮动按钮而不强滚到底。
- [x] **Phase A Step A3 — 消息真实时间与 Markdown 复制操作区 (`DshSessions.vue`, `App.vue`, `bob-kernel`, `bob-runtime`)**：后端 `AppMessage` 与 `dsh_session::Message` 暴露真实 `created_at` 时间戳（无可靠时间返回 `None` 绝不编造）；前端跨天自动带日期、当天显示 `HH:mm`；用户与助手消息均提供轻量复制按钮（默认复制原始 Markdown，桌面端悬停显示、移动端常驻易点）。
- [x] **Phase A Step A4 — 五阶段发送与流式状态区分 (`DshSessions.vue`, `App.vue`)**：严格区分 `正在发送／等待接收…` $\rightarrow$ `已送达，等待回复…` $\rightarrow$ `节点正在处理…` $\rightarrow$ `正在回复…` $\rightarrow$ `完成`，无虚假打字机模拟，`unknown_send` 与断线缓存独立展示。
- [x] **Phase B Step 1 — DSH 会话 Cookie 优先复用与受控日志尾部确定性恢复 (`dsh_transport.rs`, `harness_auth.rs`, `runtime_rpc.rs`)**：实现 `DshClient::connect_or_recover`（先复用本地原子持久化的 `dsh-auth-*` Cookie 做零模型消耗 `check_auth`，网络不可达保留凭据不误判失效，仅认证拒绝时交换 token 或从受控日志尾部 64KB 确定性提取最新启动 token）；新增 `harness_auth.rs` 覆盖 7 类工具（`dsh / agy / codex / claude / pi / hermes / openclaw`）的五项统一内部动作与 `runtime_rpc` 接线。
- [x] **Phase C — 事前锦囊 (`ExecutionAssertion`)、全绿写锁与热重启状态 (`context_capsule.rs`)**：落盘 `freeze_execution_assertion` 与 `evaluate_critique_against_assertion`（当置信度 $\ge 0.90$ 且验证全绿锁写时，拦截无证据空投质疑的盲目推翻重写；提供具体报错/行号证据时放行定点修复）；在 `bob-task-card-v1` 注入内联精简防膨胀约束，并实现 `WarmRestartState`（持久化已定位文件、失败假设与 diff 摘要）。
