# Bob 当前开发清单

> 当前发布线：v0.9.0
> 产品方向：`docs/PRODUCT_VISION.md`
> 阶段顺序：`docs/BOB_EVOLUTION_ROADMAP.md`
> 完整计划：`docs/superpowers/plans/2026-08-10-work-continuity-evolution-plan.md`

本文件只保存当前实施批次和紧邻下一批的任务。目标架构不得写成已实现能力。

## 已完成主线：Phase 0–4 Work Core、变更审查与复杂度路由

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

## v0.8.0 遗留质量门

- [ ] 使用真实 PC/Android 完成 Capture 三入口与 Relay trace 对账。
- [ ] 记录 PC 主程序、绿色包、安装器和 Android APK/AAB 字节数。
- [ ] Source 正文提取、Knowledge Point 蒸馏和证据关系继续独立演进，不阻塞已完成的 Work Core 引用层。

## 暂缓

在 Persistent Work Core 和单 Agent Advanced Loop 被验证前，暂缓多 Agent、Runtime Host、订阅调度、完整 DAG UI、iOS、独立 Web UI和新增通讯渠道。

## 完成规则

任务只有在代码、测试、用户可理解错误、恢复、权限、同步影响、依赖/体积和权威文档同时通过后才能标记完成。
