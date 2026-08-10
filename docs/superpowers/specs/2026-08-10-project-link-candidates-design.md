# Phase 2A：Project Link Candidate 设计

> 状态：已实现并通过回归（Phase 2A）
> 日期：2026-08-10  
> 上位计划：`docs/superpowers/plans/2026-08-10-work-continuity-evolution-plan.md`  
> 产品原则：确定性直写，歧义待归属；先保存，不打断，不猜项目。

## 1. 目标

Phase 2A 建立 Capture 与 Persistent Work Core 之间的第一条可靠关联通道。

用户明确说“在某项目中新增任务”或“某项目决定采用某方案，因为……”时，Bob 应在能够唯一确认项目和工作对象内容时自动写入 Project State。项目缺失、重名、已归档或 Decision 缺少理由时，Bob 不弹出阻断式确认，也不猜测归属，而是保存原始 Capture，并在“工作”页面显示一个可稍后处理的待归属项。

本阶段证明以下闭环：

```text
Capture
  → Project Link Proposal
  → deterministic project resolution
  → resolved Work Object，或 pending candidate
  → append-only Work Event
  → Capture derived_refs
  → WorkView 可见结果
```

## 2. 范围

### 2.1 本阶段实现

- 为 Capture 路由增加 `work_task` 与 `decision` 两种明确意图。
- 只处理带明确 `project_id` 或明确 `project_hint` 的工作输入。
- 根据 Work Core 中的活动项目执行本地确定性匹配。
- 新增 `project_link_candidates`，保存已解决和待处理关联。
- 项目唯一且 Proposal 完整时自动创建 Work Task 或 Decision。
- 项目不唯一、找不到、失效或 Decision 缺少 reason 时进入待归属。
- WorkView 展示待归属项，支持选择项目、补齐 Decision reason、确认或忽略。
- 所有确认写入具有幂等、事务、revision 和事件证据。
- 中英文 UI 和错误信息使用现有 i18n 机制，不存固化的展示语言。

### 2.2 本阶段不实现

- 普通 Todo/Event 与 Work Task/Milestone 的双向引用；该项属于 Phase 2B。
- Note、Source、Knowledge Point 的项目引用；该项属于 Phase 2C。
- 文件 Artifact、版本和会议多对象派生；该项属于 Phase 2D。
- LLM 根据内容相似度猜测项目。
- 自动创建项目、自动合并项目或自动修改已有 Work Object。
- Change Detection、Decision 挑战与影响分析；这些属于 Phase 3。

## 3. 不破坏现有行为

Phase 2A 不接管普通日程和待办。

- “提醒我明天提交 Bob 报告”继续进入现有 Todo 路径。
- “明天下午三点参加 Bob 项目会”继续进入现有 Event 路径。
- “在 Bob 项目中新增任务：完成同步回放”进入 `work_task`。
- “Bob 项目决定保留现有架构，因为迁移风险更低”进入 `decision`。

只有明确的“项目工作对象”表达才进入 Work Core。这样不会在 Phase 2B 完成前制造 Calendar Todo 与 Work Task 两套未关联事实。

## 4. 数据契约

### 4.1 ProjectLinkProposal

Proposal 是路由结果中的结构化候选，不是已提交事实：

```json
{
  "intent": "work_task",
  "title": "完成同步回放",
  "projectId": null,
  "projectHint": "Bob",
  "description": null,
  "reason": null,
  "confidence": 0.96,
  "reasonCodes": ["explicit_project_task"]
}
```

`decision` 必须同时具有非空 `title` 和 `reason` 才能自动提交。模型输出永远只是 Proposal；项目匹配和最终写入由本地代码决定。

### 4.2 project_link_candidates

每个 Capture 最多有一个当前 Phase 2A Candidate：

| 字段 | 说明 |
|---|---|
| `id` | 稳定 ID，确定性地由 Capture ID 生成 |
| `capture_id` | 原始 Capture，唯一且不可更换 |
| `intent` | `work_task` 或 `decision` |
| `title` | 冻结的工作对象标题 |
| `proposal_json` | 描述、Decision reason 等结构化候选 |
| `project_hint` | 用户原始项目提示，可为空 |
| `candidate_project_ids` | 本地匹配得到的活动 Project ID 数组 |
| `selected_project_id` | 自动或用户最终选择的 Project ID |
| `status` | `pending`、`resolved`、`dismissed` |
| `reason_code` | 为什么自动解决或为什么等待 |
| `confidence` | Proposal 置信度；不代替确定性项目校验 |
| `resolved_object_id` | 成功创建的 Work Object ID |
| `last_error` | 最近一次应用失败，供诊断而非直接展示堆栈 |
| `retry_count` | 应用失败次数 |
| `revision` | 用户处理时的乐观锁版本 |
| `created_at` / `updated_at` / `resolved_at` | 生命周期时间 |

Candidate 保存最小派生数据，不复制原始长文本；完整输入继续以 `capture_journal` 为准。

## 5. 项目解析规则

项目解析只读取 `work_projects` 中未删除、未归档的项目：

1. `project_id` 非空：必须精确存在且为活动项目；否则进入 `pending/project_unavailable`。
2. `project_hint` 非空：使用统一的大小写、空格、连字符和“项目”后缀归一化。
3. 恰好一个标题精确匹配：自动选择，`reason_code=unique_exact_title`。
4. 多个精确匹配：进入 `pending/ambiguous_project`，保存全部候选 ID。
5. 没有精确匹配：进入 `pending/project_not_found`，候选数组为空。
6. 没有 Project ID 和 Hint：不创建 Candidate，保持原有 Capture 路由。

Phase 2A 不做模糊、向量或 LLM 项目匹配。旧 Markdown 项目只有先以稳定 `project_` ID 注册进 Work Core 后才参与自动匹配；不在 Capture 路径中偷偷创建项目。

## 6. 提交流程与事务边界

### 6.1 自动解决

当项目唯一且 Proposal 完整时，在一个 SQLite transaction 内：

1. 以稳定 Candidate ID upsert Candidate。
2. 校验 Capture 仍存在且 Candidate 未 dismissed。
3. 以 `capture-work:{capture_id}:{intent}` 作为 Work Object 幂等键。
4. 创建 Work Task 或 Decision。
5. 推进 Project aggregate revision。
6. 追加 Work Event。
7. 将 Candidate 标记为 resolved 并记录 Work Object ID。
8. 将 `work:<object_id>` 合并进 Capture `derived_refs`。
9. 将 Capture 标记为 committed，清除可重试错误。
10. 提交事务。

任何一步失败都回滚本次 Work Object、Work Event、Candidate resolution 和 Capture refs 变化。回滚完成后，使用独立的小事务 upsert `pending` Candidate，并更新 `last_error` 与 `retry_count`；该诊断写入不得包含任何 Work Object。Markdown Project Snapshot 在主事务提交、释放数据库锁之后 best-effort 刷新；快照失败不回滚已确认的 SQLite 事实，但必须写日志，并允许用户手动重新生成。

### 6.2 待归属解决

用户在 WorkView 选择项目或补齐 reason 后，`resolve_project_link_candidate` 使用 Candidate `revision` 做乐观锁，并复用同一提交流程。重复点击返回原 resolved 回执，不创建第二个 Work Object。

### 6.3 忽略

`dismiss_project_link_candidate` 将 Candidate 标记为 dismissed，写一条 Capture activity event，并把 Capture 标记为 committed，`derived_refs` 保留 `project_link:<candidate_id>` 以便追溯。原始 Capture 不删除；dismissed 项不再自动重试，也不进入待归属列表。

待归属时 Capture 使用现有 `needs_clarification` 状态和稳定原因码 `project_assignment_pending`，但前端不弹窗，只由 WorkView 的待归属区域承接。自动解决和用户解决成功后 Capture 才进入 committed。

## 7. Repository 边界

现有 `work_core::repository::create_object()` 自己开启 transaction，不能直接嵌入 Capture 多表原子提交。本阶段将其拆成：

- 公共 wrapper：保持现有 Commands 行为不变；
- transaction-scoped primitive：供 Project Link Service 在调用方 transaction 中复用。

Candidate 与 Capture 编排放入独立的 `work_core/project_links.rs`，不把路由、匹配、事务和 UI 命令继续堆入 `capture_router.rs` 或 `repository.rs`。

模块职责：

```text
capture_router.rs          识别 work_task / decision Proposal
work_core/project_links.rs 项目解析、Candidate 生命周期、跨表事务
work_core/repository.rs    Work Object 持久化 primitive
work_core/commands.rs      list / resolve / dismiss Tauri Commands
WorkView.vue               非阻断待归属 UI
```

## 8. WorkView 交互

“待归属”是 WorkView 顶部的紧凑区域，不使用启动弹窗或全局 Modal。

每项只展示：

- 类型图标；
- 标题；
- 项目提示；
- 等待原因；
- 时间；
- 项目选择器；
- Decision 缺失时的 reason 输入；
- “归入项目”和“忽略”操作。

项目候选唯一但 Proposal 缺字段时预选项目；项目重名时列出候选；无匹配时允许选择任一当前活动项目。成功后该项从待归属消失，目标 Project 刷新并显示新增对象与活动记录。

移动端默认只展示最多三项，其余通过“查看全部”展开。整个界面继续使用 Lucide、设计变量和当前语言 i18n。

## 9. 错误与恢复

| 场景 | 行为 |
|---|---|
| Project 在确认前被删除或归档 | 保持 pending，刷新候选并显示“项目已不可用” |
| Candidate revision 冲突 | 返回当前 Candidate，前端刷新，不覆盖较新选择 |
| Work Object 已由重复请求创建 | 返回幂等回执并修复 Candidate/Capture 引用 |
| 事务中任一步失败 | 主事务全部回滚；随后只更新 pending Candidate 的 `last_error` |
| Snapshot 写入失败 | 数据提交成功；日志记录并允许手动导出，不虚报数据库失败 |
| Clerk 或网络不可用 | 原始 Capture 保留；确定性路径继续可用，复杂 Proposal 稍后处理 |
| 跨端重复 Candidate | 按 Capture ID 和幂等键归并，不创建第二 Work Object |

错误日志保存稳定错误代码和参数，UI 根据当前语言翻译，不在数据库中固化中文或英文句子。

## 10. Commands 与 Bridge

新增 Commands：

- `work_project_link_list_pending(limit)`
- `work_project_link_resolve(input)`
- `work_project_link_dismiss(input)`

Vue 只通过 `tauri-bridge.js` 调用：

- `workProjectLinkListPending(limit)`
- `workProjectLinkResolve(input)`
- `workProjectLinkDismiss(input)`

浏览器 Mock 必须覆盖无候选、重名候选、Decision 缺 reason 和成功解决四种状态。

## 11. 测试与验收

### 11.1 Rust 单元与集成测试

- 明确有效 Project ID 自动解决。
- 唯一精确项目名自动解决。
- 重名项目生成一个 pending Candidate，不创建 Work Object。
- 无匹配生成 pending Candidate，原 Capture 保留。
- 没有项目提示的普通 Todo/Event 不受影响。
- 缺 reason 的 Decision 不自动提交。
- Candidate resolve 同时更新 Work Object、Work Event、Candidate 和 Capture refs。
- 故障注入证明跨表事务无半套状态。
- 重复自动处理和重复用户确认只创建一个 Work Object。
- stale Candidate revision 不能覆盖新结果。
- 项目失效后确认保持 pending。
- 数据库关闭重开后 pending 和 resolved 状态仍可恢复。

### 11.2 前端与构建

- WorkView 空状态、三种待处理原因和成功移除可渲染。
- 中英文 key 完整；界面无 Emoji。
- PC 宽屏和 Android 纵屏不出现阻断式弹窗或横向溢出。
- `npm test`、`npm run build`、Work Core Rust 测试、完整 `cargo test --lib` 和 `git diff --check` 通过。
- 不新增 npm、Cargo、Python 或客户端运行时依赖。

### 11.3 完成定义

以下场景必须端到端成立：

1. 用户输入“在 Bob 项目中新增任务：完成同步回放”。
2. Bob 本地唯一匹配 Project，并自动创建 Work Task。
3. Capture、Candidate、Work Object 和 Work Event 可相互追溯。
4. 重启应用后项目中仍能看到任务。
5. 将项目改成重名后再次输入，Bob 不猜测、不弹窗；WorkView 出现待归属项。
6. 用户稍后选择项目，系统只创建一个任务并移除待归属项。

达到上述条件后才进入 Phase 2B；不得把普通 Todo/Event 双向同步或知识引用写成 Phase 2A 已完成能力。
