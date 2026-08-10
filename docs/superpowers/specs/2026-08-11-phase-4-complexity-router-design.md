# Phase 4 Complexity Router 设计

> 状态：implemented
> 日期：2026-08-11
> 上游：`docs/PRODUCT_VISION.md`、`docs/BOB_EVOLUTION_ROADMAP.md`、`docs/GOAL_RUNTIME.md`

## 1. 目标

用户默认只需自然表达。Bob 选择最轻量且足够可靠的处理方式，并返回可解释的结构化判断：

- `Direct`：一次回答或单步、低复杂度动作；
- `Deep`：当前会话内可完成的有限多步分析、工具调用与验证；
- `Advanced`：需要跨时间、阶段、依赖、恢复或持续推进。

Phase 4 只完成路由，不冒充 Phase 5 的持久 Goal Runtime。自动识别为 Advanced 时，当前执行器只能完成有边界的启动、上下文检查或下一步说明，不得宣称持续目标已经完成，也不得自动进入旧 `goal.rs` Maker–Checker 原型。

## 2. 方案选择

采用独立的纯 Rust `complexity_router`，而不是继续在 `llm.rs` 中堆叠关键词，也不采用全模型分类。

路由分两层：

1. 确定性层提取显式动作、复杂分析、跨时间、阶段/依赖、恢复、风险、附件和用户覆盖信号；
2. 只有确定性结论低置信度且语义确实可能改变处理模式时，才限时调用已配置的 Clerk。Clerk 不可用、超时或输出无效时保守降级，不阻塞 Direct 基本能力。

## 3. 数据契约

每次判断返回 `RouteDecision`：

- `mode`：`direct | deep | advanced`；
- `taskKind`：`answer | action`；
- `confidence`：0–1；
- `risk`：`r0 | r1 | r2 | r3 | unknown`，只用于说明，不替代 Policy Engine；
- `duration`：`instant | session | persistent`；
- `source`：`override | deterministic | clerk | conservative_fallback`；
- `reasonCodes`：稳定、可本地化、可测试的原因代码；
- `requiresProjectState`：是否需要持久 Project/Goal 状态；
- `semanticFallbackRecommended`：内部控制字段，不作为执行授权。

任何模型输出只能在本地枚举和约束内解析。Clerk 不得授予权限、修改用户模式或直接启动工具。

## 4. 确定性规则

优先级从高到低：

1. 用户显式覆盖：只回答、帮我做、Goal 原型；
2. 强持续性：明确要求持续跟进、跨多天/多周、直到完成、中断后恢复、分阶段长期推进；
3. 强 Deep：多步动作、研究后比较并交付、批量对象、多个相互依赖步骤、需要验证；
4. 强 Direct：天气/事实问答、单个日程或待办、单步可逆动作；
5. 不确定：保持最小权限，并仅在 Clerk 可用时请求语义分类。

重复日程本身是单步工具操作，不因“每天/每周”自动升级 Advanced。长文本本身也不构成升级依据。复杂分析可以是 `Deep + answer`，因此仍只获得只读工具。

## 5. 执行映射

- `Direct + answer`：R0 白名单，最多少量只读查询；
- `Direct + action`：Quick Action 工具范围，最小步骤；
- `Deep + answer`：只读工具，但允许更高的研究/验证预算；
- `Deep + action`：Planned 工具范围，当前会话内有限循环；
- `Advanced`：使用 Planned 权限边界执行有限启动，不自动调用旧 Goal Loop；系统提示明确禁止虚假完成；
- `agent_mode=goal` 继续作为用户显式选择的历史原型入口，直到 Phase 5 替换，但 UI 和文档必须标注其非持久边界。

路由结果不绕过 R2/R3 确认。风险等级仍由工具调用时的 Policy Engine 最终裁决。

## 6. 用户体验

默认保持 Auto。每条新回复可显示一个低干扰模式标签：直接处理、深度处理或持续任务；用户不需要先理解模式才能发送。

高级覆盖入口保留：

- 只回答：强制只读；
- 帮我做：强制 Deep Action，但不绕过权限；
- Goal：历史 Maker–Checker 原型，明确标为实验性且不承诺跨重启恢复。

标签只使用 Lucide 图标、主题色和灰色；中英文文案同步，不使用 Emoji。

## 7. 失败与降级

- Clerk 未配置、断网、超时或 JSON 无效：返回确定性保守结果；
- 无明确动作信号：默认 `Direct + answer`；
- 可能是动作但授权不清：保持只读并让主模型澄清；
- Advanced Runtime 尚未实现：返回 Advanced 路由，但只进行 bounded kickoff；
- 任何分类异常不得阻止用户发送普通问答。

## 8. 测试与验收

建立可审阅的中英文 JSON 回放集，至少覆盖：

- 普通问答、天气、解释、翻译；
- 单个待办、日程、文件动作；
- 复杂分析、批量处理、比较后交付；
- 持续跟进、跨时间项目、恢复和阶段依赖；
- 长文本不误升级；
- 重复日程不误升级；
- 用户覆盖；
- Clerk 合法、非法和失败降级。

完成门槛：

1. 路由契约、确定性规则和 Clerk 合并均有单元测试；
2. 中英文回放集全部通过；
3. 自动 Advanced 不进入旧 Goal Loop，也不宣称持久目标完成；
4. 路由结果进入响应和 UI，可解释但不干扰；
5. 前后端构建与全量测试通过；
6. 不增加客户端依赖或运行时，用户已有未跟踪文件不受影响。

## 9. 当前非目标

- 不实现持久 Goal Contract、节点、Evidence、Checkpoint；
- 不实现 DAG、多 Agent 或 Runtime Adapter；
- 不把路由结果写成 Project canonical state；
- 不自动迁移或删除旧 Goal 原型；
- 不改变工具风险等级或确认策略。
