# Phase 3 Decision Memory 与 Change Review 实施计划

> 状态：implemented
> 设计：`docs/superpowers/specs/2026-08-10-phase-3-decision-change-design.md`

## A. Decision 契约

- [x] 新增强类型 `DecisionData` 与 rejected alternative 契约；
- [x] 标准化字符串、数组、去重和旧数据兼容；
- [x] Work Object 校验调用 Decision 契约；
- [x] Capture Router、会议派生和候选解析传递完整字段；
- [x] Snapshot 展示 owner、participants、alternatives、evidence 和 revisit condition。

## B. Change Review 核心

- [x] 新增 `work_change_reviews` 表、索引和状态结构；
- [x] 实现显式关系、Decision evidence、Artifact source 和 affected IDs 分析；
- [x] 实现 unknown-scope Review；
- [x] 实现 accept/reject/defer/reopen 与 revision/idempotency；
- [x] 接受后创建 `affected_by`、`contradicts` 或 `supersedes` 关系。

## C. 文件修订接入

- [x] 相同 path/hash 幂等；
- [x] 不同 hash 原子创建新 Artifact、Change、版本关系候选；
- [x] 保留旧 Artifact 与前后 fingerprint；
- [x] 路径入口只在事务成功后指向新 Artifact；
- [x] Capture refs 可追溯 Artifact、Change 和 Review。

## D. API 与界面

- [x] 增加待审 Change 列表和处理 Commands；
- [x] Bridge 与浏览器 mock 同步；
- [x] WorkView 增加非阻断 Change Review 区；
- [x] 中文和英文文案同步，不使用 Emoji。

## E. 验证与文档

- [x] Decision 契约单元测试；
- [x] Change 分析、状态机、幂等、跨项目和回滚测试；
- [x] 前端测试与生产构建；
- [x] Rust 全量测试与 `git diff --check`；
- [x] 更新 README、ARCHITECTURE、LLM_WIKI、路线图、todo 和 progress；
- [x] 核对依赖文件未变化，保留用户未跟踪文件。
