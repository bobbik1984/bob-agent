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

## D-013：Phase 5 使用单活动执行切片与证据门

- 日期：2026-08-11
- 状态：accepted

决定：Work Core Goal 保存期望终态，独立 Goal Runtime 保存 Run、Attempt、Evidence、Checkpoint、Approval 和 Event。首版全局最多运行一个有预算的顺序切片；R0/R1 可自动推进，R2/R3 与关键歧义进入持久结构化选择。Repository 在没有已验证必需 Evidence 时拒绝 `done`。

理由：先验证一个可恢复、可审计、不会虚假完成的纵切片，比直接引入 DAG、多 Agent 或常驻 Host 更能暴露真实产品问题，同时保持 PC/Android 零外部运行时和客户端体积边界。

约束：应用关闭期间不承诺执行；未知副作用不得自动重放；R3 handoff 不等于批准；2–4 个选项协议必须适配触控、鼠标和手表旋钮/滚动，语音仅是可选输入；Phase 6 前不得把顺序 Plan 宣称为 DAG。

## D-014：Today 是对话内的只读投影，不是新的工作真相源

- 日期：2026-08-11
- 状态：accepted

决定：Bob 启动与新对话继续以 Conversation 为主界面。Today 以紧凑卡片和共享 Layer 投影 Calendar、Todo、Work Core、Goal Runtime、Session 与 Dream；悬浮速记仅提供入口并保留草稿。普通聚合使用确定性本地规则，不依赖 Main/Clerk。

理由：独立工作首页会增加导航与阅读摩擦，也容易形成第二套状态。统一 surface 能让桌面、手机和未来轻量终端共享同一信息层级，同时保留 Bob “一句话进入、按需展开”的产品特征。

约束：Today 不得改写源数据；一个来源失败不得污染全部状态；每设备已读相互独立；默认只显示一个焦点和最多两个关注项；移动端不得强制全屏；Quick Note → Today 切换不得丢失未提交文本；语义增强失败必须退回确定性结果。

## D-015：用户表达目的，Bob 恢复上下文

- 日期：2026-08-14
- 状态：accepted

决定：日常交互以“目的 + 本次例外”为主。Bob 从 Work Core、Decision、Change、Evidence、Today、Session 和结构化记忆中恢复完成当前目的所需的最小上下文；聊天记录只补充临时语义，不承担长期工作状态。

理由：成熟个人助手的价值是减少用户重复背景，而不是要求用户学习更完整的 Prompt 写法。结构化权威状态比长聊天拼接更适合跨会话、跨设备和重启后的连续工作。

约束：上下文事实必须带来源、时效和置信度；对象冲突时不得猜测执行；用户显式约束始终覆盖历史推断；首版不新增向量数据库或第二套状态源。

## D-016：能力只有在当前环境真实可用时才存在

- 日期：2026-08-14
- 状态：accepted

决定：Bob 根据当前设备、应用沙盒、授权目录、已配对设备和任务触发的健康探测形成有界能力视图。模型知道某种工具并不代表当前设备拥有它。

理由：PC、Android 和远程入口的执行条件不同。把假定能力暴露给模型会造成无效调用、虚假成功和用户对系统边界的误解。

约束：能力探测按需、白名单、短期缓存，不扫描秘密和无关环境；Android 应用沙盒不冒充通用 Shell；能力存在也不等于已经获得 R0–R3 授权。

## D-017：始终选择最轻的真实可行路径

- 日期：2026-08-14
- 状态：accepted

决定：同一目的优先使用确定性本地操作和已有 Rust 能力，其次才是已配对设备转交、用户已配置的外部能力，最后是询问或安全延后。简单任务不进入 Goal Runtime、任务图或模型修复循环。

理由：个人助手首先要快速、安静和稳定。更复杂的执行形态只有在能提高真实完成率时才有价值。

约束：Complexity Router 继续只决定处理强度；路径选择不得扩大权限；任何降级都必须说明真实状态，排队、转交和生成内容不得冒充完成。

## D-018：Phase 5.5 不建立第二套工作或事件真相源

- 日期：2026-08-14
- 状态：accepted

决定：Project、Goal、Task、Decision 等继续由现有 Work Core SQLite 负责；长期内容继续遵守 Markdown 权威边界；Goal Runtime 继续只保存执行进度。Phase 5.5 只增加理解、能力选择和结果回执所需的薄契约。

理由：全面事件化或复制状态会增加迁移、投影、恢复和一致性成本，却不能直接改善个人日常使用。

约束：首版 Context Resolver 只读现有事实，不新增数据库表；外部副作用使用幂等键和最小回执去重；只有真实故障数据证明必要时才扩展恢复边界。

## D-019：Bob 的成长从可撤销的纠正开始

- 日期：2026-08-14
- 状态：accepted

决定：近期个性化先区分本次例外、显式长期偏好和重复纠正候选。长期记录必须有作用域、来源、置信度并可查看、可撤销；不根据一次成功自动生成或激活 Skill。

理由：记住用户明确纠正比自动编写程序性能力更直接、更安全，也更符合长期个人关系中的信任建立顺序。

约束：Dream 不得修改 SOUL、Policy 或执行权限；不内置 Python/Lua 解释器；经验只有经过重复验证和用户审阅后，才可能成为未来 Skill 候选。
