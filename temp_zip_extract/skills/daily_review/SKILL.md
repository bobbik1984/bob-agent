---
name: daily_review
description: 每日工作闭环：晚间总结 + 晨间启动，通过共享状态文件衔接而非互相调用
version: 1.0.0
tags: [Workflow]
related_skills: []
---

# Daily Review 技能 — 设计思考（Draft）

> ⚠️ 本文件为设计笔记，待进一步细化后实现。

## 核心问题

OpenClaw Agent 需要两个互补的日常流程：

| 流程 | 触发时间 | 输入 | 输出 |
|:---|:---|:---|:---|
| **晚间总结** (evening_review) | 每日工作结束时 | 当天对话记录、完成的任务 | `daily_summary.md` |
| **晨间启动** (morning_kickoff) | 隔天上午 | 昨日 summary + 项目介绍 + 今日计划 | 工作上下文初始化 |

## 设计原则：避免循环与冗余

### ❌ 反模式：互相调用

```
evening_review → 生成明日计划 → morning_kickoff → 更新计划 → 又触发 review...
```

这会导致：
- 无限循环风险（A 调 B，B 又触发 A）
- 信息重复（两个地方都维护"明日计划"）
- 职责不清（谁是计划的 source of truth？）

### ✅ 正确模式：共享状态文件 + 单向数据流

```
                    ┌─────────────────────────┐
                    │   workspace/daily/       │
                    │   ├── YYYY-MM-DD.md      │  ← 每日状态文件（唯一数据源）
                    │   └── today_plan.md      │  ← 当日活跃计划（可变）
                    └─────────┬───────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               │               ▼
     evening_review           │       morning_kickoff
     （只写 summary）          │       （只读 summary + 写 context）
              │               │               │
              ▼               │               ▼
     YYYY-MM-DD.md           │        Agent 工作上下文
     写入：                   │        加载：
     - 完成了什么             │        - 昨日成果（从 summary 读）
     - 遇到的问题             │        - 项目背景（从 blueprint 读）
     - 明日建议方向            │        - 今日计划（从 plan 读/生成）
```

**关键**：两个流程通过**文件系统**衔接，而不是互相调用。各自只负责自己的写入区域。

## 数据流设计

### 1. Evening Review（写入者）

**触发**：用户说"总结一下今天的工作" 或 定时触发（如 22:00）

**读取**：
- 当天的对话历史（chat.history API）
- `today_plan.md`（检查计划完成情况）

**写入** → `workspace/daily/YYYY-MM-DD.md`：
```markdown
# 2026-02-21 工作总结

## 完成事项
- ✅ OpenClaw WebSocket 握手成功（Ed25519 签名格式修复）
- ✅ chat.send 参数修正
- ✅ 创建 weather 技能包

## 未完成 / 遗留
- 会话隔离（sessionKey）
- 聊天历史加载

## 遇到的问题
- 国内 VPS 无法稳定访问 wttr.in

## 明日建议方向
- 优先完成会话隔离
- 部署 weather 技能包到 VPS1
```

**不做**：不生成具体的"明日计划"，只给出**方向建议**。

### 2. Morning Kickoff（读取者 + 上下文初始化者）

**触发**：定时触发（如 08:00）或用户上线时

**读取**（只读，不修改）：
1. `workspace/daily/YYYY-MM-DD.md`（昨日 summary）
2. `Project_Blueprint.md` / `Implementation.md`（项目介绍，当前阶段）
3. `task.md`（全局任务列表，找到待办项）

**生成** → `workspace/daily/today_plan.md`（覆盖写入）：
```markdown
# 2026-02-22 工作计划

## 上下文回顾
昨日完成了 OpenClaw WebSocket 连接和 weather 技能包创建。
当前处于 Phase 6: Project Claw 阶段。

## 今日优先事项
1. 🔴 会话隔离 — 为 Home Panel 分配独立 sessionKey
2. 🔴 聊天历史 — 调用 chat.history API
3. 🟡 Weather 技能部署

## 参考资料
- Implementation.md > Project Claw 章节
- task.md > Phase 6
```

**不做**：不修改昨日的 summary 文件。

## 防循环机制

| 规则 | 说明 |
|:---|:---|
| **单向写入** | evening 只写 `YYYY-MM-DD.md`，morning 只写 `today_plan.md` |
| **不互相触发** | 两者都是独立的定时任务或手动触发，互不调用 |
| **幂等性** | 多次运行同一个流程，结果相同（覆盖写入） |
| **时间锁** | evening 只处理今天的数据，morning 只读昨天的 summary |
| **文件是桥梁** | 两个流程唯一的耦合点是 `workspace/daily/` 目录下的文件 |

## 目录结构

```
openclaw/Skills/daily_review/
├── SKILL.md              # 本文件（设计 + 使用说明）
├── config.json           # 配置（触发时间、项目文件路径等）
├── scripts/
│   ├── evening_review.sh  # 晚间总结脚本
│   └── morning_kickoff.sh # 晨间启动脚本
├── templates/
│   ├── daily_summary.md   # summary 输出模板
│   └── daily_plan.md      # plan 输出模板
└── assets/
    └── README.md
```

## 待明日细化的问题

1. **对话历史如何读取？** — 需要确认 `chat.history` API 返回的格式，以及如何筛选当天的对话
2. **项目文件路径怎么配置？** — 不同项目可能有不同的 blueprint/task 文件位置
3. **多项目支持？** — 如果同时在做多个项目，summary 和 plan 怎么组织
4. **定时触发机制？** — OpenClaw 的 cron 配置，还是系统级 crontab
5. **summary 的详细程度？** — 太详细会噪声太大，太简略会丢失上下文

## 与 weather 技能的对比

| 特性 | weather | daily_review |
|:---|:---|:---|
| 外部依赖 | 天气 API | 文件系统 + chat history |
| 触发方式 | 用户询问 / 定时 | 手动 / 定时 |
| 写入位置 | 无（直接回复） | `workspace/daily/` |
| 幂等性 | 天然幂等 | 需要设计 |
| 复杂度 | 低 | 中（涉及上下文理解） |
