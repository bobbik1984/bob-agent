# Phase 4 Complexity Router 实施计划

> 状态：implemented
> 设计：`docs/superpowers/specs/2026-08-11-phase-4-complexity-router-design.md`

## A. 独立路由契约

- [x] 新增独立 `complexity_router` 模块和序列化契约；
- [x] 实现 Direct、Deep、Advanced、task kind、risk、duration、source 与 reason codes；
- [x] 从 `llm.rs` 移除旧结构化分类职责，仅保留执行映射。

## B. 分层判断

- [x] 实现用户覆盖和确定性信号优先级；
- [x] 区分复杂只读分析与复杂动作；
- [x] 区分重复日程和真正跨时间持续工作；
- [x] 低置信度时限时调用 Clerk，并严格解析枚举输出；
- [x] Clerk 不可用或失败时保守降级。

## C. 执行与界面接入

- [x] 将路由结果映射到工具范围、预算和 system prompt；
- [x] Auto Advanced 不调用旧 Goal Loop，并禁止虚假完成；
- [x] 响应返回结构化 route，前端显示低干扰模式标签；
- [x] 中英文文案同步，不增加 Emoji。

## D. 回放与验证

- [x] 建立中英文路由回放集；
- [x] 覆盖确定性判断、覆盖模式、Clerk 解析与降级测试；
- [x] Rust 全量测试、前端测试和生产构建通过；
- [x] `git diff --check` 与依赖文件检查通过；
- [x] 保留用户未跟踪文件。

## E. 文档对齐

- [x] 更新 README、ARCHITECTURE、LLM_WIKI、路线图、todo、progress 与 Decision Log；
- [x] 明确 Phase 4 已完成路由但 Phase 5 持久 Runtime 尚未完成。
