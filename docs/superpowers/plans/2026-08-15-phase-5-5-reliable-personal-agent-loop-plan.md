# Phase 5.5-C–E：可靠个人 Agent 闭环实施计划

> 状态：代码闭环已实施；真实 PC Context Packet 启用门与 PC/Android 真机质量门待完成
>
> 日期：2026-08-15
>
> 设计依据：`docs/BOB_ARCHITECTURE_V3.md`
>
> 前置计划：`docs/superpowers/plans/2026-08-14-phase-5-5-personal-assistant-context-plan.md`

## 1. 本轮结论

Bob 已经有持久 Work Core、复杂度路由、单 Agent Goal Runtime、权限门、上下文解析和记忆整理等正确骨架，但这些模块尚未组成一条可靠的个人助手闭环。

当前真实状态不是“九项能力都已实现”，而是：

| 能力 | 当前成熟度 | 已有事实 | 关键缺口 |
|---|---|---|---|
| 持久执行 | 可用纵切片 | Goal Contract、Run/Attempt、Evidence、Approval、Checkpoint、启动恢复 | Auto Advanced 仍默认落入“个人工作区”，没有消费已解析的真实项目上下文 |
| 鲁棒性 | 局部可用 | 工具预算、重复调用熔断、Capture 退避、Goal 最多三次有界尝试 | 缺少统一错误分类；Goal repair 主要是再次提示模型，尚不是按错误类型改变策略 |
| 聪明路由 | 分层已实现，行动未闭环 | Direct/Deep/Advanced、Answer/Action、R0–R3 工具过滤 | 尚不知道当前设备的真实能力，也没有 `local_execute / pc_handoff / ask / defer` 决策层 |
| 上下文恢复 | 代码完成，用户收益未启用 | PurposeFrame、候选评分、歧义门、预算化 Context Packet | 默认仍为 shadow；Today focus 未接入；真实 PC 数据尚未通过启用门 |
| 结果可信 | Advanced 局部可用 | Evidence gate、工具摘要、审批持久化 | Direct Action 没有统一 ResultReceipt；生成文本、工具成功和真实状态提交仍未完全统一 |
| 记忆成长 | 有能力但边界不一致 | 显式纠正、learned facts、夜间合并与衰减 | Dream 仍会自动重写 `SOUL.md`，工具失败也会写入 SOUL；没有基于验证结果的记忆准入 |
| 事件与恢复 | 分域存在 | `work_events`、`goal_events`、Capture Journal、工具审计日志 | 没有统一 Turn Flow；近期也不需要建设可回放模型思考的通用事件平台 |
| Skill 进化 | 只具备读取与安装 | 本地 Skill 发现、读取、导入与同步 | 不应自动生成或激活 Skill；近期只形成可审阅经验候选 |

因此下一步不进入 Dynamic Task Graph。先完成“理解正确、能力真实、行动合适、结果可信、记忆安全”的最小闭环。

## 2. 用户价值目标

本阶段只证明五件事：

1. 用户主要说目的时，Bob 能恢复唯一明确的工作对象；
2. Bob 只选择当前设备真实存在、健康且已授权的能力；
3. 简单事情走最轻路径，复杂事情才进入持久 Goal；
4. 临时错误可以有界恢复，未知副作用和重大分歧才打扰用户；
5. 只有可验证结果和明确纠正能够进入长期记忆。

## 3. 明确不做

- 不实现通用 DAG、节点并行或多 Agent Swarm；
- 不增加 Python、Lua、Node、Playwright、容器或第二常驻服务；
- 不加入任意命令执行工具，也不因为探测到 PowerShell 就向模型暴露 Shell；
- 不建设记录模型思考的全量 SessionEvent 平台；
- 不自动生成、写入或激活 `SKILL.md`；
- 不扩大文件访问目录，不用能力探测绕过 R0–R3；
- 不在本阶段扩展外部消息渠道、网页登录或链接处理。

## 4. Batch 0：先修正进化边界

### 4.1 停止 Dream 自动修改 SOUL

修改 `src-tauri/src/evolution.rs`：

- Dream 不再调用模型重写 `SOUL.md`；
- 工具失败不再追加到 SOUL 的“避坑指南”；
- SOUL 只由用户显式编辑或未来经过明确审批的身份/交互原则变更修改；
- 不自动删除用户现有 SOUL 内容，避免把历史写入误判为可安全清理的数据。

### 4.2 失败进入诊断记忆

- 复用 `execution_errors` 或结构化 memory entry 保存错误类型、工具、可重现条件和处置状态；
- 失败诊断默认不进入普通 Prompt，只在相同工具或相同错误类型再次出现时按需检索；
- 单次偶发错误不形成长期规则；重复错误只生成可审阅候选。

验收：运行 Dream 不会改变 SOUL；工具失败不会形成身份或长期偏好；现有显式纠正仍可使用。

## 5. Batch 1：让上下文真正进入执行

### 5.1 完成 Context Resolver 启用门

- 用真实 PC Work Core 数据运行影子误选检查；
- 补齐唯一项目、两个相近项目、无项目日常请求、过期/归档项目和显式 ID 场景；
- 只有唯一高置信度项目才关闭 shadow 并注入 Context Packet；
- 保留一键回退开关，不改变工具权限。

### 5.2 Goal Runtime 使用解析后的项目

当前 Auto Advanced 总是调用 `ensure_personal_workspace()`。调整为：

1. 唯一高置信度项目：Goal Contract 绑定该 Project；
2. 无候选：继续进入个人工作区；
3. 多候选：不创建正式 Goal，返回一次最小澄清；
4. 用户明确给出 Project ID：优先使用并校验有效性。

Context Resolver 仍是只读解析器；Goal 创建由 Work Core Repository 事务完成。

验收：用户说“继续把这个项目推进到可评审”时，不会在个人工作区生成孤立 Goal；歧义时不误建 Goal。

## 6. Batch 2：Capability Snapshot v1

### 6.1 最小契约

新增纯 Rust `capability.rs`，定义：

```text
CapabilitySnapshot
  device_id / device_role
  os / architecture
  sandbox_scope
  granted_paths[]
  local_capabilities[]
  connected_peers[]
  captured_at / expires_at

Capability
  id
  state: available | degraded | unavailable
  permission_scope
  risk_class
  reason_code
  version?
```

### 6.2 首批能力

- 两端共有：Calendar、Todo、Note、Capture、Work Core 查询、受控文件访问；
- Windows：授权目录、系统信息、浏览器适配器状态、Git/PowerShell 的只读探测结果；
- Android：应用沙盒、系统权限、可用本地工具和已配对 PC；
- 外部连接器只有在已配置且健康时才为 available。

PowerShell/Git 只有“检测到程序”但没有受控执行适配器时必须标记为 `degraded`，原因是 `capability.adapter_missing`，不得进入模型工具表。

### 6.3 工具表收口

`get_filtered_tool_schemas()` 在现有意图和风险过滤之外，再与 Capability Snapshot 求交集。系统 Prompt 不再静态宣称所有工具都可用。

验收：不可用或未授权能力不会进入模型工具列表；探测不读取密钥、全部环境变量或无关目录；关闭能力层可回退到现有白名单。

## 7. Batch 3：Action Selector v1

新增确定性 `action_selector.rs`：

```text
PurposeFrame
  + AssistantContext
  + CapabilitySnapshot
  + RouteDecision
  + ToolRisk
  -> ActionDecision
```

只允许四种结果：

- `local_execute`：当前端真实可用且权限满足；
- `pc_handoff`：手机无法本地完成、可信 PC 在线且允许转交；
- `ask`：对象冲突、不可逆选择或必要授权缺失；
- `defer`：能力暂不可用、设备离线或等待外部条件。

规则优先于模型。模型可以帮助解释目的，但不能声明能力存在、降低风险或把 handoff 当作完成。

验收：五个日常场景的行动选择稳定可回放；一次只询问一个会改变路径的问题；简单提醒不进入 Goal Runtime。

## 8. Batch 4：错误分类与有界自愈

### 8.1 统一错误类别

至少定义：

```text
transient_network
sqlite_busy
invalid_arguments
permission_denied
capability_unavailable
verification_failed
unknown_side_effect
budget_exhausted
```

### 8.2 恢复规则

| 错误 | 默认处理 |
|---|---|
| 网络抖动、短超时、SQLite busy | 确定性退避一次 |
| 参数格式明确错误 | 纯规则修正一次 |
| Advanced 验证失败且副作用明确为 none | 最多一次模型诊断，并要求改变策略 |
| 权限拒绝、能力不存在 | ask、handoff 或 defer，不重复调用 |
| 未知副作用、不可逆风险 | 立即停止并请求用户决定 |

删除“同一提示最多再跑三遍”的表面修复。保留总预算、重复调用熔断、时间上限和取消能力。

验收：同一失败策略不会盲目重复；每次重试能说明依据；失败状态可定位且不会写入长期事实。

## 9. Batch 5：ResultReceipt 与安全成长

### 9.1 最小 ResultReceipt

```text
decision_id
status
verified_evidence[]
state_changes[]
side_effect_state
correction_refs[]
completed_at
```

- Advanced 复用 `goal_attempts`、`goal_evidence` 与 `goal_events`；
- Work 状态变化继续进入 `work_events`；
- Direct Action 仅在产生副作用时保存一个最小、幂等的 action receipt；
- 不保存模型私有推理，不重复建设全量事件流。

### 9.2 记忆准入

- 用户明确纠正：立即进入可撤销 correction；
- 用户明确长期偏好：按 scope 保存；
- 单次例外：只属于当前请求；
- 已验证成功策略：先生成经验候选，不自动生效；
- 工具失败：进入 diagnostic，不进入 SOUL；
- `SKILL.md`：继续只允许用户导入或未来显式批准的候选提升。

验收：没有 ResultReceipt 的任务不能触发结果学习；用户能区分“本次例外”和“以后都这样”；删除/撤销记忆后不再注入。

## 10. 验证顺序

每个 Batch 都执行最小专用测试，最终执行：

```powershell
npm test
npm run build
cargo test --lib
cargo check
```

并补充以下回放与故障注入：

1. 唯一项目的一句话目标；
2. 两个相近项目的歧义；
3. PC 可本地完成；
4. 手机可本地完成；
5. 手机需要转交 PC；
6. 能力缺失或适配器缺失；
7. 工具短超时后成功；
8. 参数错误后规则修正；
9. 未知副作用立即停止；
10. 显式纠正、单次例外和长期偏好的分流。

客户端依赖不得变化。安装包、绿色版、Android 产物、冷启动与空闲资源继续使用 Phase 5.5 基线比较。

## 11. 阶段完成门

- [x] Dream 不再自动修改 SOUL，失败只进入诊断边界；
- [ ] Context Packet 在真实 PC 场景通过后对唯一高置信度项目启用；
- [x] Auto Advanced 使用解析后的真实项目，歧义时不误建 Goal；
- [x] Capability Snapshot 能区分可调用、降级、不可用、授权范围和已连接 PC；
- [x] 不可用工具不会进入模型工具列表；
- [x] Action Selector 四种结果具有确定性回放；
- [x] 错误按类别处理，重试有界且不会重复未知副作用；
- [x] Direct 与 Advanced 都有可验证结果回执；
- [x] 只有明确纠正、长期偏好和验证结果能进入对应记忆层；
- [x] 不新增用户侧运行时、常驻服务或强制配置；
- [x] 五个日常场景与故障注入通过代码回放；
- [ ] PC/Android 真机、体积与资源质量门通过。

只有以上完成门通过，才根据真实长线任务失败数据决定是否进入 Phase 6 Dynamic Task Graph。
