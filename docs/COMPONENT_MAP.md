# Bob — Component Map

> 把逻辑组件映射到真实代码。统一项目新源码见本页末段；前身落点仅供只读参考，去留判断（retain、adapt、replace、merge、retire，均为 proposed，在 BP1 去留表获批后才生效）。
> §2–5 的旧项目路径相对于 `<workspace_root>`；本仓库代码路径相对于仓库根。历史观察时间：2026-10-28，旧 App HEAD `a4ff5fd`，旧 Online HEAD `d14a527`，13个tracked修改、111个untracked项（只读采样）。详细表、状态、凭据及测试清单见[WP-01](evidence/2026-10-28-wp01-deep-inventory.md)。

## 1 统一项目（本仓库）

| 逻辑组件 | 落点 | 状态 |
|---|---|---|
| 文档与治理 | `AGENTS.md`、`docs/`、`references/` | 已建立 |
| bob-kernel / bob-transport | `crates/bob-kernel`、`crates/bob-transport` | 已有领域状态机与传输；完整验收见 CURRENT_STAGE |
| Tauri / Vue / 直连 | `apps/bob-app/src-tauri`、`apps/bob-app/src` | Windows/Android 已有实现；工作区增量另验 |
| Online 过渡客户端 | `src/online-client.mjs`、`src/views/WorkView.vue`（均在 apps/bob-app 下） | JS 配对/读取适配；原生认证、缓存与真实服务待闭环 |
| bob-runtime / bob-node / Web bridge | XP-01–10 | 尚未建立 |

## 2 旧 App（`bob-agent/`，Tauri v2 + Vue 3，0.9.30）

| 能力 | 主要落点 | 初判 |
|---|---|---|
| 设计系统、视图、i18n | `src/`（views、components、locales），`design_principles.md`，`design_system.html` | retain（ONL-D-003 已认定其为 Canonical UI） |
| Bridge 适配层 | `src/tauri-bridge.js` | adapt |
| LLM 引擎、Provider、工具循环 | `src-tauri/src/llm.rs`、`tools.rs` | adapt（Provider 注册改为配置驱动） |
| Persistent Work Core | `src-tauri/src/work_core/`（`repository.rs`、`project_links.rs`、`decision_change.rs`） | adapt（并入 bob-kernel，与 Online 的语义对齐） |
| Goal Runtime、Complexity Router | `src-tauri/src/goal_runtime/`、`complexity_router.rs` | retain 或 adapt |
| 捕获、笔记、知识库、知识图谱 | `capture.rs`、`capture_router.rs`、`notebook.rs`、`kb_indexer.rs`、`kb_extractor.rs`、`kg.rs` | retain（个人事务域） |
| 日程与待办 | `calendar.rs`、`scheduler.rs` | retain |
| Dream、记忆、演化 | `dream.rs`、`evolution.rs`、`assistant_context.rs` | retain，需评估 |
| PC↔Android 同步与设备信任 | `sync_engine.rs`（530KB）、`device_trust.rs`（324KB）、`sync_protocol.rs`、`sync_diagnostics.rs` | **replace/adapt建议**：双向LWW/冲突副本→Home事件与intent；work_*/goal_*未同步；不整文件迁入 |
| HTTP API | `http_api.rs`（285KB） | 待评估 |
| 中继（生产实例在 VPS3） | `relay/`（权威源码）；`bob-relay.js`、`src-tauri/src/bin/bob-relay.rs` 为遗留实现 | adapt（改为可替换的 Connection Provider） |
| 远程 Agent（DSH）与 Core 客户端 | `src-tauri/src/remote_agent/`（`dsh.rs`、`live.rs`、`core.rs`） | adapt（变成 Harness runner 或契约客户端） |
| 设备凭据保护 | `src-tauri/plugins/tauri-plugin-device-vault`；`keychain.rs`（已停用） | adapt：Remote Agent用DPAPI/Android vault；LLM Key仍明文；crypto设备PIN方案不是OS保护，Linux vault尚缺 |
| 微信网关 | `src-tauri/src/wechat/` | 待定（开源合规） |
| 其他：Web Drop、PPTX 导出、Daily Brief、MCP、Doctor、llama sidecar | `web-drop/`、`exports/pptx.rs`、`daily_brief/`、`plugins.rs`、`doctor.rs`、`sidecar.rs` | 逐项评估 |
| 旧交换契约（UDES） | `docs/contracts/bob_cloud_agent_interchange.yaml` | retire（已被 ONL core-client-contract-v1 取代） |

## 3 旧 Online（`cloud-agent/`，pnpm monorepo，Node/TS）

| 能力 | 主要落点 | 初判 |
|---|---|---|
| 协议类型 | `packages/shared/` | 作为契约参考，进入新 schema |
| Core：State Store、Task/Execution/Artifact/Verification/Review | `packages/server/src/` | 语义移植到 bob-kernel（BOB-D-005） |
| Runtime 与 Harness 适配（DSH、AGY、Codex、pi） | `packages/server/src/runtime/`（本轮按正式入口只读核对，冲突副本不迁入） | 过渡期作为 runner 复用（进程边界） |
| 节点原生会话 | `packages/server/src/routes/node-work.ts`、`runtime/dsh-web-session.ts` | 参考协议/UI；内存回执不承担新Outbox可靠性 |
| WebUI（Studio、NodeWork、Review、Dual-Diff） | `packages/ui/src/` | merge：治理视图并入统一 UI |
| 部署与发布脚本 | `deploy/`（工作树状态以本次Evidence为准） | 参考；不进入统一项目 |
| Lab、实验 | `packages/server/src/lab/` | 待评估（含个人主机引用） |

## 4 其他相关

| 项 | 落点 | 说明 |
|---|---|---|
| 独立 relay 目录 | `bob-relay/`（不是 Git 仓库） | 与旧 App 的 `relay/` 关系待核 |
| 搬移的 worktree | 工作区规定的外置 worktree 缓存目录下的 `cloud-agent-r06d-r05e` | 2707 项未提交改动，含 `.ssot-staging/b4*`；**禁止 prune** |

## 5 WP-01 → WP-02 的复用边界

- 保留语义：Goal/Milestone分组、唯一Task、Execution尝试、不可变成果父版、检查与人审、请求幂等、签名/吊销及进程归属。
- 必须适配：旧App的rpc_staged_outbox为接收配置暂存；write_mobile_outbox是JSON写命令；旧Online四条governed路径是AGY/Pi/DSH/Codex，Generic API仍是旧PTY路径。
- 必须重写：Home/Replica同步、跨认证会话的持久请求回执、单飞预热与Outbox、统一领域写入入口。不能迁入磁盘失败退回内存或吞SQL错误的“成功”路径。
- 代码迁入尚未获文件级批准；本表不是搬迁许可。该段为WP-01时点记录；当前运行实现见下方BP2落点，Schema检查不替代产品验收。

## 历史源码落点（2026-10-28 至 29，非当前完成度）

以下带日期条目保留历史验证范围；当前以§1、§6及 CURRENT_STAGE 为准。

### 1.0.1 新写源码落点（2026-10-28）

- `Cargo.toml`、`crates/bob-kernel/`：持久消息/Outbox、迁移、预热纯状态机；Windows离线内核9项测试通过；Unix/真实Provider未验证。
- `apps/bob-app/src/send-controller.mjs`：输入框本地保存确认控制器；注入bridge，尚无Tauri/Provider接线。
- `apps/bob-app/tests/send-controller.test.mjs`：6项本地模拟bridge测试，不证明真实内核IPC或网络。

## BP2运行链路（2026-10-29）

- `crates/bob-kernel/src/wake.rs`：运行时共享Future、订阅代际和自动休眠；`prewarm.rs`保留纯状态模型，实际运行只由WakeCoordinator管理连接。
- `src/inbox.rs`（同crate）：Home原子消息/回执与受控停止请求。
- `src/dispatch.rs`、`src/deadline.rs`（同crate）：持久领取与结果提交、有界异步等待，无跨await数据库借用。
- `tests/wake.rs`、`tests/inbox.rs`、`tests/dispatch.rs`：10+8+7项运行回归。实际网络/Tauri adapter仍未接。

## App IPC源码落点（2026-10-29）

- `crates/bob-kernel/src/app.rs`、`migrations/0002.sql`：LocalApp、本机身份与默认会话，2项Rust测试通过。
- `apps/bob-app/src/App.vue`、`src/kernel-bridge.mjs`、`src/native-bridge.mjs`：Vue输入/状态和真实Tauri invoke映射；9项Node控制器/bridge检查，非原生窗口证据。
- `apps/bob-app/src-tauri/`：独立Tauri构建单元，避免内核离线回归意外解析GUI依赖；与根Workspace共用bob-kernel路径依赖，不是第二业务内核。已通过外置原生编译和开发进程启动；窗口交互待验。

## 本机受管执行落点（2026-10-29）

- crates/bob-kernel/src/local_execution.rs：同一Task/Execution/lease与回复事务、停止请求、可信退出恢复；不创建第二任务系统。
- apps/bob-app/src-tauri/src/runner.rs：门控Worker、Windows Job进程归属、取消/超时与子进程退出。
- apps/bob-app/src-tauri/src/lib.rs：app_execute/app_cancel/app_executions及持久完成回执恢复；前端不能提交伪造结果。
- apps/bob-app/src/execution-controller.mjs、App.vue：发送接线、单飞轮询、停止按钮和真实回复来源显示。
- apps/bob-app/src-tauri/tests/fixtures/fake_cli.rs：操作系统进程故障验证专用，不是模型。
- 当前检查与未验边界统一见[CURRENT_STAGE](CURRENT_STAGE.md)和[shell证据](evidence/2026-10-29-bp2-shell.md)。

## Home传输与权限落点

- `crates/bob-kernel/src/home_auth.rs`、`migrations/0003.sql`：可信本地授权管理、精确会话授权及每次业务操作校验；不负责配对或签发凭据。
- `crates/bob-transport/`：标准双向TLS、实际观测身份、有界帧与Home接收适配；测试证书仅位于测试代码。
- `apps/bob-app/src-tauri/src/device_vault.rs`：Windows DPAPI原生凭据槽，其他平台明确不可用。
- `apps/bob-app/src/App.vue`：输入框旁固定停止入口，与消息行复用同一取消命令。


## 6 旧 App（0.9.30）与旧 Online（cloud-agent）能力逐项继承与验收对照表（2026-10-30）

> 规则：所有旧版能力必须明确对应 `F1`–`F7`、当前 `1.0.1` 真实状态、新源码落点与视作“已完成继承”的退出验收门，严禁以“保留/待评估”静默遗漏，也不得用“内核状态机单测通过”冒充端到端继承完成。

| 旧版能力模块 | 旧版代码来源 | 对应功能树 | 去留裁决 | 当前 `1.0.1` 落点 | 当前真实状态 | 视作“完成继承”的验收门（Exit Gate） |
|---|---|---|---|---|---|---|
| **独立 BYOK 直接模型对话（无需配对/服务器）** | 旧 App `src-tauri/src/llm.rs` | **F1 基础助理** | `adapt`（API Key 改存系统安全存储，支持显式模型选择与连通测试） | `crates/bob-kernel/src/direct_provider.rs`、`migrations/0006.sql`、`apps/bob-app/src-tauri/src/direct_llm.rs`、`App.vue` 设置与对话入口 | **本轮落地内核与原生直连链路**（区别于本地 CLI 与本机权重推理） | Windows 与 Android 在未配对 Home、无 VPS 下配置 API Key 完成两轮连续对话、流式/取消/错误可见、重开保留；已配对手机在 Home 离线时直连对话并在重连后幂等汇入 Home |
| **本机模型权重推理（Llama sidecar）** | 旧 App `src-tauri/src/sidecar.rs` | **F1 扩展 Provider** | `evaluate`（作为独立本地权重 Provider，不与 API 直连混同） | 待独立 Provider 插件评估 | 未迁入（不阻塞 F1 API 直连） | 独立声明本地模型权重文件、硬件内存门槛与进程生命周期，不以 API 直连冒充 |
| **多项目/多会话目录、草稿 CAS 与双视图分组** | 旧 App `src/views/ChatView.vue` + 旧 Online `projects` | **F1 / F2 连续性** | `merge`（统一 Project / Conversation / ViewState） | `crates/bob-kernel/src/catalog.rs`、`migrations/0005.sql`、`App.vue` | **内核/IPC/浏览器已验，Pixel 已验 A/B 会话草稿切换** | 跨重启保留各会话历史、草稿、阅读锚点与“按项目/按执行设备”分组 |
| **PC ↔ Android 配对、双向 TLS 与离线发件箱** | 旧 App `sync_engine.rs`、`device_trust.rs`、`outbox.rs` | **F2 跨设备连续** | `replace`（改为 Home 主权威 + 单飞预热 + `intent_outbox`） | `crates/bob-kernel/src/outbox.rs`、`wake.rs`、`inbox.rs`、`replica.rs`、`crates/bob-transport/` | **私网 Pixel 扫码与真实回复已验**；离线直连汇合本轮落地；公网中继待验 | 私网与异地公网断线重连、未知回执先对账不重发、已配对手机离线直连模型后重连无冲突汇入 Home |
| **闪念捕获（Capture）与捕获路由** | 旧 App `capture.rs`、`capture_router.rs` | **F1 / F3 个人事务域** | `retain & adapt`（统一落盘 `captures` 表，断网先本地保存） | `crates/bob-kernel/migrations/0001.sql` (`captures`)、`src/personal_ops.rs` | **本轮落地内核 CRUD 与归属**；捕获 UI 已有接线，实际操作与跨端验收待补 | 离线/在线快速捕获文本与来源引用，支持转化为待办/笔记/记忆，从旧 `bob.db` 可无损读回 |
| **日程与待办（Calendar / Todo / Task）** | 旧 App `calendar.rs`、`scheduler.rs`、`work_core/` | **F3 个人事务域** | `retain & merge`（Task 统一权威，日程关联 `schedule_items`） | `crates/bob-kernel/migrations/0001.sql` (`tasks`)、`0006.sql` (`schedule_items`)、`src/personal_ops.rs` | **本轮落地内核日程/待办状态机与回归**；ScheduleView 已接 event/todo，Pixel IPC 创建/完成/重开有证据，提醒未验 | 待办与日程同一权威存储，支持起止时间、提醒时间与完成状态流转，旧库记录可迁入读回 |
| **Markdown 笔记与决定记录（Notebook / Decision）** | 旧 App `notebook.rs`、`work_core/decision_change.rs` | **F3 个人事务域** | `retain`（保留理由、备选、证据与重审条件） | `crates/bob-kernel/migrations/0001.sql` (`decisions`)、`0006.sql` (`notes`)、`src/personal_ops.rs` | **本轮落地内核笔记/决定持久化与版本递增**；KnowledgeView 已有笔记/决定入口，完整字段与真实表单验收待补 | 笔记支持 Markdown 正文与标签检索；决定记录支持 `proposed -> accepted -> superseded` 链式追溯 |
| **长期记忆与上下文提取（Memory / Dream / Context）** | 旧 App `dream.rs`、`evolution.rs`、`assistant_context.rs` | **F3 个人事务域** | `retain & adapt`（每条记忆必须绑定 `source_capture_id` 或 `source_message_id`） | `crates/bob-kernel/migrations/0001.sql` (`memories`)、`src/personal_ops.rs` | **本轮落地内核记忆提议/激活/更替（supersede）**；自动蒸馏管线待接 | 记忆变更可追溯至源消息/捕获、支持用户显式遗忘（`forgotten`）与版本更替，不静默污染提示词 |
| **知识库索引与知识图谱（KB / KG）** | 旧 App `kb_indexer.rs`、`kb_extractor.rs`、`kg.rs` | **F1 / F3 检索域** | `evaluate & adapt`（派生索引不进主同步流，源文档归属 Home/Workspace） | 待 BP3/BP4 检索切片接入 | 规划中（未宣称已完成） | 本地 FTS/图谱索引可从权威笔记与归档对话重建，损坏可重算，不阻塞基础聊天 |
| **附件与图片输入（Attachments / Media）** | 旧 App 对话附件与系统文件选择 | **F1 / F4 文件与多模态** | `retain`（按平台权限附加图片/文件引用与 SHA256） | `captures` / `messages.content_json` 附件引用契约 | 待接平台文件选择器与多模态上传 | 聊天支持附图/文件并校核平台权限与大小限制 |
| **手机让 PC 处理文件（远程只读浏览与批准导出）** | 旧 App `@pc` 指令 + 新增 F4 沙箱 | **F4 远程文件处理** | `replace`（受控工作区范围 + 明确执行位置） | `crates/bob-kernel/src/local_execution.rs`、`workspace_handoff.rs` | 已验受控临时目录只读 Codex；获准真实工作区浏览/导出待验收 | 手机选择获准工作区→读取指定文档→查看总结→人工批准导出，PC 离线明确显示等待 |
| **Work 治理闭环（Task / Execution / Artifact / Verification / Review）** | 旧 Online `packages/server/src/db/`、`routes/` | **F5 Work 与 Harness** | `merge`（语义迁入 `bob-kernel`） | `crates/bob-kernel/migrations/0001.sql`、`local_execution.rs`、`online_provider.rs` | 内核 DDL 与执行状态机已就绪；Artifact 版本对比与 Review UI 待接 | V1→V2 成果版本链、自动检查与人工 Review 门禁在统一 UI 跑通 |
| **多 Harness 进程边界接入（Codex / AGY / DSH / Pi）** | 旧 Online `packages/server/src/runtime/` + 旧 App `remote_agent/` | **F5 / F6 常开节点** | `adapt`（过渡期通过 Online Provider 接入，最终统一 Runner IPC） | `apps/bob-app/src-tauri/src/runner.rs`（Codex）、`crates/bob-kernel/src/online_provider.rs` | 本机与 Home 只读 Codex 已真机验证；Online 兼容契约已单测，真实 Online 端点待联调 | 各 Harness 单独列出 `resume`/`cancel`/`events` 真实证据；过渡期接通生产 Online，BP5 完成数据迁移后旧服务退役 |

### F1/F3 图片捕获增量落点

| 职责 | 当前源码 |
|---|---|
| 原件、哈希、关联与事务 | `crates/bob-kernel/src/capture_media.rs`、`migrations/0008.sql` |
| 视觉角色与多协议图像载荷 | `crates/bob-runtime/src/vision.rs` |
| Node/Tauri 图片接口与识别宿主 | `crates/bob-node/src/lib.rs`、`apps/bob-app/src-tauri/src/lib.rs` / `direct_llm.rs` |
| 捕获编辑、项目关联与草稿 | `apps/bob-app/src/App.vue`、`kernel-bridge.mjs` |
| 已保存原图回看 | `apps/bob-app/src/components/CaptureImage.vue`、`views/KnowledgeView.vue` |
| 实际浏览器验证 | `apps/bob-app/tests/photo-capture-browser.py`（仅隔离节点，本机模拟视觉服务） |

F2/F5：`crates/bob-runtime/src/dsh_session.rs`负责原生身份/历史投影，`dsh_transport.rs`负责认证、目录、分页、文件读取及发送回执；Node/Tauri共用运行时。`ConversationDirectory.vue`与`DshSessions.vue`统一目录和原会话。`reading-cache.mjs`承载可淘汰的IndexedDB阅读缓存，`persistent-preview-cache.mjs`负责版本核对、原文件和缩略图缓存；它们不是任务或发送账本。网页/手机实际验收版本见CURRENT_STAGE，历史导入不冒充原生目录。

### 共用消息阅读

`apps/bob-app/src/components/MessageContent.vue` 与 `message-markdown.mjs`：App助手消息和Work Online历史的安全Markdown子集阅读。`tests/message-markdown.test.mjs` 覆盖结构、链接拒绝、代码与长文保留；远端附件传输不由此组件实现。
