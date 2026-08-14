# Phase 5.5-A/B：个人助手上下文恢复实施计划

> 状态：实施中；路线对齐、基线脚本、上下文契约、确定性解析与默认影子模式已完成，高置信度注入和受控性能/真机验证待完成
>
> 日期：2026-08-14
>
> 设计依据：`docs/BOB_ARCHITECTURE_V3.md`
>
> 实施范围：Phase 5.5-A 基线与 Phase 5.5-B Context Resolver v1

## 1. 本轮只解决什么

本轮只验证一件事：

> 用户主要表达目的时，Bob 能否从现有 Work Core 和 Today 状态中恢复正确的当前工作对象与最少必要事实，同时在不确定时不猜测执行。

本轮完成后，Bob 应具备以下最小改进：

- 用户说“把这个项目推进到可评审状态”时，高置信度场景能关联到唯一的当前项目；
- 注入模型的是带来源、时效和预算的结构化上下文包，不是整个项目或更多聊天长文本；
- 存在两个合理候选时，不把任意一个伪装成确定事实；
- Direct、Deep、Advanced 仍由现有 Complexity Router 决定，不改变执行权限；
- Work Core、Goal Runtime、Today 和 Markdown 的现有权威边界不变。

本轮不实现：

- PowerShell、Git、浏览器或 Android 能力探测；
- `local_execute | pc_handoff | ask | defer` 行动选择器；
- 新的数据库表、向量数据库、通用事件平台或任务图；
- 自动 Skill、浏览器自动化、额外后台服务或用户侧运行时；
- UI 大改、跨端协议改造或旧上下文系统一次性删除。

## 2. 不可突破的工程边界

- 不修改 `docs/PRODUCT_VISION.md`；
- 不在实现前把 Phase 5.5 写成已完成功能；
- 不新增 Cargo、npm、Python、Lua、Node、Playwright 或容器依赖；
- 不新增 SQLite 权威状态；Context Resolver 只读现有事实；
- 不持有数据库锁跨越模型调用或网络等待；
- 不把聊天摘要当作 Project、Decision、Task 或 Commitment 的权威来源；
- 不让上下文置信度改变 R0–R3 权限；
- 不修改用户未提交或无关文件；
- 新增生产路径不得使用 `unwrap()` 或 `panic!()`；
- 关闭新上下文注入后，现有聊天、Capture、Work、Goal 与 Today 必须继续可用。

## 3. 交付策略

采用“确定性只读解析 + 高置信度启用 + 其余保守降级”的方案。

首版不让模型从所有项目中自由挑选对象。Rust 先根据显式引用、项目标题、当前活跃状态、Today 焦点和最近更新生成候选并评分。只有唯一候选越过高置信度门时，才向主模型注入该项目的最小上下文。中低置信度只记录原因和候选，不自动绑定，也不改变工具权限。

这样可以先验证“少问且不选错”最核心的一半，同时把真正的澄清交互留给 Phase 5.5-D 的 Action Selector，避免 Context Resolver 直接变成第二个编排器。

## 4. Batch 0：登记已批准方向

### 4.1 路线图与决策

修改：

- `docs/BOB_EVOLUTION_ROADMAP.md`
- `docs/DECISIONS.md`
- `todo.md`
- `progress.yaml`

动作：

- 在 Phase 5 与 Phase 6 之间正式插入 Phase 5.5；
- 将 Phase 5.5 标记为进行前/进行中，不改变 Phase 6 的未开始状态；
- 接受并登记 D-015 至 D-019：目的优先、能力有界、最轻可行路径、现有状态保持权威、成长始于纠正；
- `todo.md` 只列 Phase 5.5-A/B 当前可执行项；
- `progress.yaml` 只记录真实状态，不登记尚未实现的能力。

验收：路线图、任务表和进度源不再与 V3 冲突；`ARCHITECTURE.md`、`README.md`、`FEATURES.md` 暂不宣称新能力可用。

## 5. Batch 1：Phase 5.5-A 基线与回放夹具

### 5.1 新增无依赖基线脚本

新增：

- `scripts/measure-phase55-baseline.ps1`

脚本只使用 PowerShell 和操作系统自带能力，输出 JSON，不安装工具。记录：

- 当前 commit、版本、测量时间和设备角色；
- PC 安装包、绿色版和 Android APK 中实际存在产物的字节数与 SHA-256；
- `package.json`、`package-lock.json`、`Cargo.toml`、`Cargo.lock` 的 SHA-256；
- Bob 数据库文件大小；
- 可自动测得时记录冷启动耗时、进程工作集和空闲 CPU；无法可靠测得的字段明确写为 `not_measured`，不得填估算值。

输出写入被忽略的本地测量目录，不写入用户数据库、不启动发布、不修改配置。

### 5.2 建立五个场景的回放数据

新增：

- `src-tauri/src/assistant_context.rs`
- 该模块内的 `#[cfg(test)]` 表驱动夹具

首批数据覆盖 V3 的五个场景，但 A/B 只断言理解结果：

1. 唯一活跃项目，高置信度绑定；
2. 手机本地提醒，不错误绑定项目；
3. “桌面那份周报”保留显式文件意图，不假定手机可读取；
4. 两个招商项目，返回歧义而非任选一个；
5. 请求提到 PowerShell 时只保留能力需求，不声明能力存在。

验收：基线脚本可重复运行；测试夹具不依赖网络、模型或用户真实数据库。

## 6. Batch 2：PurposeFrame 与 AssistantContext 契约

### 6.1 在 `assistant_context.rs` 定义轻量契约

新增 Rust 类型：

```text
PurposeFrame
  raw_intent
  desired_outcome
  explicit_constraints[]
  candidate_refs[]
  requested_capability_hints[]
  confidence

ContextFact
  kind
  object_id
  title
  summary
  source_ref
  source_revision
  updated_at

AssistantContext
  active_object
  relevant_facts[]
  conflicts[]
  confidence
  reason_codes[]
  generated_at
```

约束：

- 全部类型可序列化，字段使用稳定 camelCase；
- `PurposeFrame` 首版保留用户显式表达，不推断未说出的期限或权限；
- `desired_outcome` 首版可等于清理后的最后一条用户输入，不为“看起来更智能”而改写目的；
- `ContextFact` 必须带可追溯 `source_ref` 和 revision/更新时间；
- 不保存模型私有推理链；
- Context 对象只在当前请求内存中存在，不落新表。

### 6.2 定义上下文预算

在同一模块定义确定性预算：

| Route | 最大事实数 | 最大序列化字符数 | 默认来源 |
|---|---:|---:|---|
| Direct | 6 | 3,000 | 当前对象、一个进行项、必要决定 |
| Deep | 12 | 8,000 | 再加入相关风险、Change、Evidence |
| Advanced | 20 | 16,000 | 再加入 Goal、Commitment 与当前阶段 |

任何单条摘要最多 500 字符。超限按来源优先级、状态和更新时间确定性裁剪，并增加 `context.budget_truncated` reason code。

验收：序列化快照稳定；预算、截断、空输入和超长中英文输入单测通过。

## 7. Batch 3：确定性候选解析

### 7.1 扩展只读 Repository 查询

修改：

- `src-tauri/src/work_core/repository.rs`

新增只读查询，不改变表结构：

- 获取未删除且未归档的活跃 Project；
- 批量读取候选 Project 的最小聚合；
- 复用已有 `list_projects`、`get_project_aggregate` 和 Work Object 类型，不在 `daily_brief` 中复制第二套 SQL。

如果需要新的查询函数，返回现有 `WorkProject` / `ProjectAggregate`，并有 SQLite reopen 与空库测试。

### 7.2 候选信号与排序

`assistant_context.rs` 按以下确定性信号生成候选：

1. 用户显式给出的 Work ID、Project ID 或 source ref；
2. 完整项目标题或唯一规范化标题命中；
3. 当前未完成 Work Object 的标题命中并反查 Project；
4. Today/最近活动提供的当前焦点；
5. 活跃状态与最近更新时间，只能作为次级信号，不能单独形成高置信度。

排序规则必须满足：

- 显式稳定 ID 优先于所有推断；
- 标题精确命中优先于包含关系；
- “这个项目”“继续推进”等指代表达本身不产生项目身份；
- 两个候选接近时返回 conflict；
- 仅凭“最近更新”不得跨过高置信度门；
- 同一输入和数据库快照得到相同结果。

高置信度门使用命名常量和 reason code，不把魔法数字散落在代码里。阈值由表驱动测试固定，后续只能依据回放误判数据调整。

验收：中英文标题、别名、显式 ID、空库、归档项目、同名项目、两个活跃项目和无项目日常请求测试通过。

## 8. Batch 4：最小事实包组装

### 8.1 事实选择

在唯一高置信度 Project 上组装：

- Project mission、current phase、summary；
- 未完成 Goal、Task、Commitment；
- 仍有效的 Decision；
- Deep/Advanced 才加入相关 Risk、Change、Artifact、Evidence；
- recent Work Event 只用于 freshness，不整段注入事件历史。

排序优先级：本次显式引用 > 阻塞或待确认 > 未完成承诺 > 当前阶段 > 有效决定 > 最近更新。

每条事实保留 object ID、revision 和 updated_at。无法判断有效性的 Decision 只标记为可能相关，不声称仍然有效。缺失表或损坏 JSON 返回稳定 reason code，并降级为空上下文，不阻断普通聊天。

### 8.2 生成模型可读摘要

提供纯函数：

```text
render_context_packet(PurposeFrame, AssistantContext, ContextBudget) -> String
```

输出使用固定、短小、可测试的结构；只包含目的、确认的当前对象、必要事实、冲突和来源引用。不得包含完整聊天历史、完整 Project dump、模型推理或未经证实的能力。

验收：同一输入产生稳定文本；无活动项目时输出为空；敏感配置值和无关项目不会进入包；预算严格生效。

## 9. Batch 5：接入聊天主链，先影子后启用

### 9.1 `llm.rs` 接入点

修改：

- `src-tauri/src/llm.rs`
- 必要时 `src-tauri/src/lib.rs`

接入顺序：

1. Complexity Router 仍先决定 Direct/Deep/Advanced；
2. 从最后一条用户输入生成 PurposeFrame；
3. 通过 `DbState` 在短锁范围内完成确定性只读解析，立即释放锁；
4. 记录候选、置信度、reason code、事实数和字符数，不记录完整用户内容；
5. 高置信度时把 `render_context_packet` 作为独立 system message 加入；
6. 中低置信度不绑定对象、不注入猜测项目；
7. 继续使用现有 `apply_context_tiering` 处理聊天历史，首轮不删除旧摘要逻辑。

### 9.2 灰度开关

使用现有配置存储增加内部布尔开关，默认策略分两步：

- `assistantContextShadow=true`：只解析和记录指标，不注入；
- 回放测试和本地验证通过后，关闭 shadow 并启用高置信度注入。

开关关闭或解析失败时完全退回当前聊天路径。不得新增 UI 设置，不要求普通用户理解该开关。

### 9.3 与现有系统提示的关系

本轮只将“当前项目事实”迁出无限增长的通用 Prompt，不重写整个 `llm.rs`。Skills、Memory、Wiki、设备提示和工具描述保持原状；后续按真实效果逐项拆分。Context Packet 必须位于用户消息之前，且不会被旧聊天摘要覆盖。

验收：Direct 简单问答没有项目时不增加上下文；唯一项目请求注入正确引用；歧义请求不注入任一项目；模型/网络不可用不影响 resolver 单测和普通降级。

## 10. Batch 6：可观测性与防误选门

### 10.1 稳定 reason code

至少覆盖：

```text
context.explicit_ref
context.exact_title
context.active_focus
context.ambiguous_candidates
context.no_candidate
context.low_confidence
context.source_unavailable
context.budget_truncated
```

日志只记录 conversation ID 的安全标识、候选数量、选中对象 ID、分数档位、reason code、事实数和字符数。不记录 API Key、完整环境变量、完整 Prompt 或完整用户数据库内容。

### 10.2 防误选测试门

必须通过以下断言：

- 100% 显式 ID 测试命中正确对象；
- 同名或接近候选 100% 返回歧义；
- 无项目日常请求 100% 不绑定项目；
- 归档、删除和其他项目的事实不进入上下文；
- 高置信度启用后，原有 R0–R3 决策结果完全不变；
- Resolver 失败不会阻断聊天，也不会扩大工具列表。

不把“首次理解正确率”写成生产结论；本轮只建立可测回放基线，真实用户指标留给后续观察。

## 11. Batch 7：验证、文档与提交

### 11.1 最小充分验证

按顺序执行：

```powershell
npm test
npm run build
cargo test assistant_context --lib
cargo test work_core --lib
cargo test --lib
cargo check
```

Rust 构建继续使用项目既有的 OneDrive 外 target 目录策略。若全量测试受现有环境阻塞，必须记录准确失败命令、错误和已通过的专用测试，不能用部分通过替代全量通过。

同时检查：

- `git diff --check`；
- Cargo/npm manifests 与 lockfiles没有依赖变化；
- 基线前后安装产物、启动与空闲资源数据可比较；
- 新开关关闭时行为回退；
- 没有无关文件或用户文件损失。

### 11.2 完成后才同步当前架构

实现和验证通过后再按职责更新：

- `docs/ARCHITECTURE.md`：仅写已经实现的 Context Resolver v1；
- `LLM_WIKI.md`：增加代码导航、数据流和扩展入口；
- `todo.md` / `progress.yaml`：A/B 真实状态；
- `CHANGELOG.md`：只记录用户能感知到且已验证的变化；
- `README.md`、`FEATURES.md`、`USER_GUIDE.md`：只有形成稳定用户行为后才更新。

每个 Batch 独立提交，不推送、不发布。建议提交边界：

1. `docs: adopt personal assistant intelligence phase`
2. `test: establish phase 5.5 context baselines`
3. `feat: add bounded assistant context resolver`
4. `feat: inject verified work context into chat`
5. `docs: align context resolver implementation`

## 12. Phase 5.5-A/B 最终完成门

- [x] 正式路线图包含 Phase 5.5，且不把未实现能力标记为完成；
- [x] 基线数据可重复采集，缺失项明确标为未测；
- [x] PurposeFrame 和 AssistantContext 契约有稳定序列化测试；
- [x] Context Resolver 不新增数据库表或依赖；
- [x] 唯一高置信度项目可从一句目的恢复；
- [x] 两个合理候选时不自动绑定；
- [x] 上下文包有来源、revision、时效和严格预算；
- [x] Context Resolver 不改变 Complexity Router 或 R0–R3 权限；
- [x] shadow 开启或解析失败时可无损退回当前聊天路径；
- [x] 专用 Rust 测试、全量 Rust 测试、前端测试和生产构建通过；
- [x] 无新增用户侧运行时、常驻服务或安装步骤；
- [x] 文档只描述真实完成状态；
- [ ] 在受控安装版上完成冷启动、空闲资源与当前 Android 产物基线；
- [ ] 用 PC 真实工作数据检查影子误选，并完成一次端到端场景验证；
- [ ] 证据通过后关闭默认 shadow，正式启用唯一高置信度 Context Packet。

## 13. 进入 5.5-C 的前置证据

只有 A/B 满足完成门并收集到以下证据，才开始 Capability Snapshot：

- 高置信度回放没有跨项目误选；
- 歧义场景能稳定保守降级；
- Context Packet 相比当前路径没有显著增加 Prompt 体积；
- 关闭开关可以完整回退；
- 安装包、冷启动和空闲资源没有越过 V3 质量门；
- 至少完成一次 PC 真实场景验证。

Phase 5.5-C 仍将单独设计和实施，不在本计划中顺手加入 PowerShell、浏览器或 Android Shell 能力。
