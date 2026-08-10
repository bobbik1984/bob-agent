# Phase 3 Decision Memory 与 Change Review 设计

> 状态：implemented
> 日期：2026-08-10  
> 上游：`docs/BOB_EVOLUTION_ROADMAP.md`、`docs/DECISIONS.md`、`docs/superpowers/plans/2026-08-10-work-continuity-evolution-plan.md`

## 1. 目标

Phase 3 让 Bob 从“知道项目里有哪些资料”升级为“知道一项新变化可能影响什么，以及为什么需要用户确认”。

完成后：

- Decision 原生保存决定、理由、备选方案、被否决方案、参与者、负责人、证据和重访条件；
- 新版文件保留旧 Artifact，登记新的文件修订事实，不覆盖既有决定；
- Bob 基于项目内的显式引用和关系找出受影响的 Decision、Goal、Task、Artifact 与 Risk；
- 不确定的 `supersedes`、`contradicts`、`affected_by` 关系进入 Change Review；
- 用户可以接受、拒绝或延后，并留下影响说明；所有动作写入 append-only Work Event。

## 2. 方案选择

### 方案 A：继续把所有字段塞入通用 JSON，不增加领域模块

改动最少，但字段校验、兼容和影响分析会继续堆进 `project_links.rs`，难以维护。

### 方案 B：为 Decision、Artifact revision 和 Change 全部建立独立规范化表

查询最强，但会过早复制 Work Object 状态、增加迁移复杂度，并破坏当前轻量兼容路线。

### 采用：方案 C，兼容 JSON + 独立 Change Review 状态机

- Work Object 仍是 Project State 的统一载体；
- Decision 使用强类型解析、标准化与校验，但仍序列化进 `work_objects.data_json`；
- Artifact 保持不可变对象语义，文件路径身份继续由 `work_external_links` 管理；
- 新增独立 `work_change_reviews`，只保存待判断影响，不复制被影响对象；
- 接受后才创建关系；拒绝和延后只改变 Review 状态并追加事件。

这能在不增加依赖、不破坏既有数据库的情况下完成 Phase 3，并为 Phase 4 Router 保留清晰接口。

## 3. 数据契约

### 3.1 DecisionData

必填：

- `decision`
- `reason`

可选但原生支持：

- `alternatives: string[]`
- `rejectedAlternatives: [{ option, reason }]`
- `participants: string[]`
- `owner: string | null`
- `evidence: string[]`，保存 Work Object ID、外部引用 ID 或稳定 URI；
- `revisitCondition: string | null`

所有字符串去除首尾空白，列表去空、去重并保持原顺序。旧 Decision 只有 `decision/reason` 时仍可读取。

### 3.2 Change 对象

Change 是“已观察到变化”的事实，至少记录：

- `changeType`
- `externalKind`
- `externalId`
- `previousFingerprint`
- `currentFingerprint`
- `observedAt`

创建 Change 不等于接受其影响。既有 Artifact 和 Decision 不被覆盖。

### 3.3 ChangeReview

每个 Review 对应一个 Change 与一个可能受影响对象：

- `change_id`
- `target_object_id`
- `target_kind`
- `proposed_relation`
- `reason_code`
- `explanation`
- `evidence_refs`
- `confidence`
- `status: pending | accepted | rejected | deferred`
- `revision`

同一 Change、目标和建议关系保持唯一，重复分析不产生第二条 Review。

## 4. 影响分析规则

按可靠性从高到低：

1. 文件路径现有 `artifact_source` 指向的 Artifact：确定性受影响；
2. 以该 Artifact 为端点的显式 Work Relation：关系另一端进入候选；
3. Decision `evidence` 中直接引用该 Artifact、外部链接或文件路径：进入候选；
4. Capture/模型提供的显式 `affectedObjectIds`：验证对象确实属于同一项目后进入候选；
5. 只靠标题或语义相似度：本阶段不自动采用，避免误改项目现实。

目标类型仅限 Decision、Goal、Task、Artifact 和 Risk。找不到显式影响时仍保留 Change，并生成一个项目级“影响范围未知” Review，不能假装没有影响。

## 5. 文件版本行为

- 相同路径、相同 hash：幂等，不生成 Change；
- 相同路径、不同 hash：创建新的 Artifact revision 和 Change，旧 Artifact 保留；
- 显式提供 `previousArtifactId`：允许路径变化时建立版本链；
- 只因文件名相同：只能成为低置信 Review 线索，不能自动建立 `supersedes`；
- 接受版本替代后建立 `new Artifact --supersedes--> old Artifact`；
- `work_external_links` 的路径入口更新为最新 Artifact，但 Change 数据保留前后 fingerprint 和对象 ID。

## 6. Review 状态机

```text
pending ──accept──> accepted
   │
   ├──reject──> rejected
   └──defer───> deferred ──reopen──> pending
```

- `accept`：事务内创建建议关系、更新 Review、追加 `change_review.accepted`；
- `reject`：不创建关系，追加 `change_review.rejected`；
- `defer`：保留候选，追加 `change_review.deferred`；
- revision 冲突返回稳定错误，UI 刷新后重试；
- 重复相同动作幂等返回当前状态。

## 7. 用户体验

WorkView 在“待归属”之后显示“变更待确认”：

- 变化摘要；
- 受影响对象及类型；
- 为什么认为受影响；
- 接受、拒绝、稍后处理；
- 可选影响说明。

界面不使用 Emoji，只使用 Lucide 图标；中文和英文文案同步。候选不弹窗、不阻断 Capture 或文件可靠保存。

## 8. 错误与恢复

- 文件读取失败：Capture 保留，进入现有 enrichment/候选失败路径；
- Change 创建失败：整个事务回滚，不产生半套 Artifact/Review；
- 单条无效显式对象引用：忽略该引用并在 Review explanation 中说明，不跨项目关联；
- Snapshot 写入失败：不回滚数据库事实，记录诊断，允许稍后重建；
- 所有生产路径返回可读 `Result`，不新增 `unwrap()` 或 `panic!()`。

## 9. 测试与完成门槛

- 旧 Decision 兼容读取，完整 Decision 标准化后可往返；
- 缺少 reason 仍进入确认，不产生残缺 Decision；
- 相同文件重复捕获幂等；不同 hash 创建新 Artifact、Change 和 Review；
- 显式 evidence/关系能定位受影响对象；无依据时显示影响未知；
- 接受创建关系；拒绝不创建；延后可重新打开；
- revision 冲突、跨项目对象、重复请求和事务回滚均有测试；
- Snapshot 展示完整 Decision 与待确认 Change；
- `npm test`、`npm run build`、Rust 全量测试通过；
- 不新增 npm/Cargo/Android 依赖，缓存继续写入 `D:\ignore_sync`。

## 10. 非目标

- 不做全文语义冲突判定；
- 不自动改写 Decision、Goal、Task 或 Risk；
- 不实现 Phase 4 Complexity Router 或 Phase 5 Goal Runtime；
- 不引入向量数据库、Python、Node 后台或新的客户端权限。
