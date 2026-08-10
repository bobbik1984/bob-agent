# Phase 2：现有输入接入 Work Core 实施计划

> 日期：2026-08-10  
> 前置规格：`docs/superpowers/specs/2026-08-10-project-link-candidates-design.md`  
> 完成目标：Capture、行动、知识与文件可以更新同一个 Project State，且可追溯、可恢复、不复制权威事实。

## 统一约束

- 不增加 Python、Node、sidecar 或客户端第三方依赖。
- 不读取或迁移真实用户数据完成自动测试。
- 所有自动项目关联只接受有效 Project ID 或唯一精确标题。
- 歧义关联不弹窗，进入 WorkView 待归属。
- 外部对象保留各自真相源；Work Core 只保存稳定引用与项目意义。
- SQLite 多表写入必须原子化；Markdown 在原子写入成功后登记引用。
- 每一步完成后运行相关测试，阶段结束运行完整回归。

## Phase 2A：Project Link Candidate

1. 新增 Candidate 模型、表、索引与状态校验。
2. 抽出 Work Object transaction-scoped primitive。
3. 实现活动项目精确解析，不做模糊或 LLM 项目猜测。
4. 扩展 Capture 路由的 `work_task` 与 `decision` Proposal。
5. 实现自动解决、待归属、用户解决、忽略和恢复。
6. 增加 Tauri Commands、Bridge Mock 与 WorkView 待归属 UI。
7. 覆盖幂等、revision、重名、无匹配、项目失效、故障回滚和重启测试。

## Phase 2B：Todo / Event 引用

1. 新增 `work_external_links`，保存 Project/Work Object 与外部真相源引用。
2. 项目化 Todo：Calendar Event 为执行事实源，Work Task 保存语义与稳定引用。
3. 项目化 Event：Calendar Event 为时间事实源，Work Milestone 保存项目意义。
4. 无项目提示的 Todo/Event 保持现有路径。
5. 歧义项目不阻止日程或待办落库；Candidate 独立等待归属。
6. 完成、取消或改期时写 Work Event，不复制日期和完成状态。

## Phase 2C：Note / Source / Knowledge 引用

1. Project Note 保持 Markdown 唯一归属，并登记 Project external link。
2. Source 与 Knowledge Point 使用多项目 external links，不写 `project_id` 归属。
3. 同一 Source 被多个 Project 引用时不复制正文。
4. Capture、Markdown 稳定 ID、Project 和 Work Event 可互相追溯。
5. Markdown 写入失败时不得登记链接；链接失败时保留 Markdown 并进入可恢复队列。

## Phase 2D：File / Meeting / Change / Commitment

1. PC 文件只保存绝对路径、流式哈希、大小和修改时间，不复制原文件。
2. 文件创建 Artifact；相同路径内容变化创建 Change Candidate，不自动改决定。
3. Meeting Proposal 可原子派生 Decision、Task 和 Commitment。
4. Commitment 必须有 owner 与 dueAt；缺失字段进入待归属/待补全。
5. 一次 Capture 的多个 Work Object、links、events 和 refs 在同一 transaction 中提交。

## 验证门

- Rust：模块单测、故障注入、数据库重开、完整 `cargo test --lib`。
- Vue：待归属与引用摘要可渲染，中英文完整，`npm test` 与 `npm run build`。
- 静态：`cargo check --lib`、`git diff --check`、无依赖清单变化。
- 数据：不访问 AppData 真实知识库；全部使用内存数据库和临时目录。
- 文档：更新 `ARCHITECTURE.md`、`LLM_WIKI.md`、`todo.md`、`progress.yaml` 和路线图真实状态。
