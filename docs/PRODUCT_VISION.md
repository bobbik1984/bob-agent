# Bob 产品愿景

> 愿景成熟度：已确认，持续验证
> 当前开发线：v0.8.1 Work Continuity

## 一句话愿景

**Bob 让复杂工作不断线。**

Bob 是面向普通知识工作者的个人工作编排层。用户只需说一句话或分享一份内容，Bob 就能理解它与现有工作有什么关系，把它转化为知识、决定、任务、日程或可持续推进的目标，并在跨对话、跨时间、跨设备和更换模型后继续维护真实工作状态。

## 面对谁

Bob 服务于每天面对大量信息、文件、沟通、任务和日程，但不愿学习复杂 AI 配置与工作流的普通白领。他们需要的不是另一个聊天框，而是一个能长期理解其职责、项目和决策背景，并持续跟进的个人助理。

## 核心问题

真实工作不是一次 `question → answer`，而是：

```text
历史背景 → 新信息 → 文件变化 → 决策 → 行动 → 验证 → 状态更新 → 再决策
```

现有聊天助手、知识库、待办、日历和专业 Agent 相互割裂。用户仍需判断信息放在哪里、重复解释背景、寻找旧决定、核对版本并人工追踪后续。Bob 要接管这些连接与连续性成本。

## Bob 必须持续回答的问题

1. 现在真正重要的事情是什么？
2. 这件事之前发生过什么？
3. 为什么当时做了这个决定？
4. 新信息改变了什么？
5. 下一步谁需要做什么，Bob 能安全推进什么？

## 差异化

Bob 不追求在编程上超过 Codex，也不追求在终端操作上超过 Claude Code。Codex、AGY、Claude Code、模型 API 和确定性工具都是可替换的执行资源；Bob 自己持有的是 Personal Work Model、Persistent Project State、Decision、History、Change 和 Evidence。

Bob 与通用 Agent 的本质区别是：

> 通用 Agent 帮用户完成一次任务；Bob 帮用户长期掌握和推进所有重要工作。

## 理想状态

用户安装 Bob，选择受支持的大模型并填入 API Key，即可开始使用。日常只需自然表达，Bob 自动选择 Direct、Deep 或 Advanced 的最轻量可靠路径，并清楚展示自己理解了什么、更新了什么、证据是什么、哪里需要用户确认。

随着时间推移，Bob 基于有来源、可纠正的工作结果越来越了解用户，使用户越来越少整理、配置、重复说明、寻找历史和催办，却能更稳定地推进复杂工作。

## 不可偏离的原则

- **Continuity over cleverness**：长期连续比单次聪明更重要。
- **State over conversation**：项目状态独立于模型会话，聊天不是数据库。
- **Decisions are first-class data**：决定必须保存理由、替代方案、证据和重审条件。
- **Reality is the final state**：生成 Artifact 不等于完成，结果必须更新真实工作状态。
- **Models are replaceable**：任何模型、CLI、订阅或 Runtime 都不得成为 Bob Core 的前提。
- **Escalate only when needed**：简单任务不进入复杂 Graph 或多 Agent。
- **Evidence before acceptance**：复杂任务只有在证据和验证满足后才能完成。
- **Automatic but accountable**：关键外部行为可审批、可追踪、可恢复。
- **Minimum setup**：核心能力不要求用户配置 MCP、Python、Node 或额外服务。
- **Lightweight delivery**：PC 保持零外部运行时和绿色使用；Android 不增加沉重依赖；安装包体积必须受控。
- **One Bob across devices**：PC 与手机共享一致状态，失败必须能定位。
- **Evolve, do not rewrite**：复用已验证模块，通过契约逐步替换旧逻辑。

## 当前边界

Bob 已有 Capture、知识、笔记、日历、待办、工具、同步、记忆和会话内 Goal Loop 原型，但尚未具备完整 Persistent Project State、Decision Memory、Dynamic Task Graph、Runtime Adapter 和基于真实结果的 Dream。规划中的能力不得写成现状。

## 北极星指标

- Continuity：重新进入项目时需要重复解释多少背景；
- Recall Quality：能否正确关联旧决定、旧文件与新变化；
- Decision Traceability：能否解释结论的来源和理由；
- Follow-through：能否把信息和会议转化为后续行动；
- Recovery：中断或更换 Runtime 后能否无损继续；
- Human Attention Saved：减少多少寻找、解释、核对和催办。

## 待验证问题

- 普通白领最先愿意持续维护的是 Project、Decision 还是 Goal；
- Bob 自动关联新信息时，何种确认方式干扰最小；
- Persistent Project State 是否能显著减少用户重复解释；
- Advanced Mode 相比单次 Deep 回答能否提高真实任务闭环率。

## 变更记录

| 日期 | 变化 |
|---|---|
| 2026-08-09 | 建立低摩擦、跨端、知识与行动统一的个人助理愿景。 |
| 2026-08-10 | 将“让复杂工作不断线”确立为北极星，明确 Personal Work Model、Persistent Project State、Decision 和可替换 Runtime 边界。 |
