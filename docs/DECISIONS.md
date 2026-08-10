# Bob 架构决策记录

本文件只记录会长期约束多个模块的架构决定。实现细节属于 `docs/ARCHITECTURE.md`，任务状态属于 `todo.md` 和 `progress.yaml`。

## D-007：现有产品渐进演进，不创建 Bob 2

- 日期：2026-08-10
- 状态：accepted

决定：保留现有 Tauri/Android、Capture、知识、日历、同步、工具和 UI，通过隔离的 Work Core 与稳定接口逐步接入和替换旧逻辑。

理由：现有模块已经解决轻量安装、跨端、模型接入和发布等高成本问题；重建会产生双线维护，并推迟 Work Continuity 的真实验证。

约束：新增模块不得继续向 `llm.rs`、`tools.rs` 或 Vue 组件堆叠隐式项目状态；旧模块只有在新路径等价验证后才能退役。

## D-008：按领域划分 Canonical State

- 日期：2026-08-10
- 状态：accepted

决定：Source、Note、Knowledge Point 和长期 Memory 以 Markdown 为权威来源，SQLite 是可重建索引；Project、Goal、Milestone、Task、Decision、Risk 和 Commitment 的运行状态以 Bob SQLite 与 append-only Work Event Journal 为权威来源，并生成可迁移 Markdown 快照。

理由：知识内容需要跨 Agent 可读和可迁移，频繁变化的工作状态需要事务、并发控制、幂等和恢复。单一技术不能同时最佳满足两种需求。

约束：现有 Markdown Project 以稳定 ID 注册进 Work Core，不进行无约束双向同步；项目快照不能反向覆盖更新的运行状态。

## D-009：Runtime 是资源，不是 Bob Core

- 日期：2026-08-10
- 状态：accepted

决定：Codex、AGY、Claude Code、模型 API、本地模型和确定性工具都通过 Agent Runtime Adapter 接入。Advanced Mode 必须在 API-only 环境成立，订阅与 Runtime Host 只提供资源优化。

理由：Bob 的长期资产是 Personal Work Model、Project State、Decision、History、Change 和 Evidence，而不是任何模型 session。

约束：Runtime 返回的 session ID、输出和 Artifact 只能成为执行记录，不能成为项目权威状态。

## D-010：Graph 管项目，Loop 完成有限工作

- 日期：2026-08-10
- 状态：accepted

决定：Dynamic Task Graph 管理长期依赖、并行、阻塞和重规划；局部 Agent Loop 只完成 bounded task。Knowledge Graph、执行 Graph 和 Goal Contract 使用不同数据契约。

理由：静态计划无法表达持续变化的项目，单个长循环也无法可靠支持跨时间恢复和局部重试。

约束：执行者不能单方面宣布完成；Goal 和节点完成必须绑定 Evidence 与验证结果。

## D-011：变化事实与影响判断分离

- 日期：2026-08-10
- 状态：accepted

决定：文件 fingerprint 或明确输入产生的 Change 可以作为观察事实自动记录，但它对 Decision、Goal、Task、Artifact 和 Risk 的影响必须进入独立 Change Review。只有用户接受后才建立 `affected_by`、`contradicts` 或 `supersedes` 关系。

理由：新信息存在不等于既有决定已经失效。把观察、判断和确认分开，既能主动提示，又能避免模型悄悄改写用户的项目现实。

约束：自动分析只能使用同项目的显式关系、Decision evidence、旧 Artifact 或经过校验的对象 ID；纯标题或语义相似度不得直接产生已确认关系。拒绝、延后和重新打开同样必须写入 Work Event。

## D-012：路由强度、执行权限与持久 Runtime 分离

- 日期：2026-08-11
- 状态：accepted

决定：Complexity Router 只判断 Direct、Deep 或 Advanced 处理强度，并输出 task kind、置信度、风险提示和持续性。确定性信号优先；只有语义模糊且可能改变处理方式时才限时调用 Clerk。工具权限继续由 R0–R3 Policy Engine 决定，持久执行继续由 Phase 5 Goal Runtime 负责。

理由：把“任务看起来复杂”“允许做什么”和“能否跨时间恢复”混为一体，会导致普通问答过度升级、模型越权或旧 Goal Loop 冒充持久执行。三者分离后，断网仍可回答，复杂只读分析不会获得写工具，Advanced 也不会虚假完成。

约束：长文本本身不能升级；重复日程本身是 Direct；Clerk 不能授予 Action 权限；Auto Advanced 不得自动进入旧 `goal.rs`。用户显式覆盖仍不能绕过 R2/R3 确认。
