# Bob 当前开发清单

> 当前开发线：v0.8.1
> 产品方向：`docs/PRODUCT_VISION.md`
> 阶段顺序：`docs/BOB_EVOLUTION_ROADMAP.md`
> 完整计划：`docs/superpowers/plans/2026-08-10-work-continuity-evolution-plan.md`

本文件只保存当前实施批次和紧邻下一批的任务。目标架构不得写成已实现能力。

## 当前主线：Phase 0–1 Persistent Work Core

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
- [ ] 明确 Decision 的 reason、alternatives、evidence、participants、owner 与 revisit condition。
- [ ] 明确 Work Object 与 Note、Source、Event、Todo、File 的引用关系。

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

## 紧邻下一批：入口关联与 Change

- [ ] Capture 可事务性产生或关联 Project、Task、Decision、Meeting、Change 和 Commitment。
- [ ] Note 只属于零个或一个 Project；Source/Knowledge Point 可被多个 Project 引用。
- [ ] Todo/Event 与 Work Task/Milestone 建立稳定引用，不复制事实。
- [ ] 新文件识别版本并生成 Change 候选；冲突或决定影响必须确认。

## v0.8.0 遗留质量门

- [ ] 使用真实 PC/Android 完成 Capture 三入口与 Relay trace 对账。
- [ ] 记录 PC 主程序、绿色包、安装器和 Android APK/AAB 字节数。
- [ ] Source 正文提取、Knowledge Point 蒸馏和证据关系进入 Phase 2，不阻塞 Work Core 数据层。

## 暂缓

在 Persistent Work Core 和单 Agent Advanced Loop 被验证前，暂缓多 Agent、Runtime Host、订阅调度、完整 DAG UI、iOS、独立 Web UI和新增通讯渠道。

## 完成规则

任务只有在代码、测试、用户可理解错误、恢复、权限、同步影响、依赖/体积和权威文档同时通过后才能标记完成。
