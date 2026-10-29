# Bob — Development Route

> 长期阶段与退出门（Exit Gate）。当前施工只看 [CURRENT_STAGE](CURRENT_STAGE.md)。阶段编号 `BP#` 不复用前身项目的 Phase 编号。步骤不代表工期承诺，测试与回退贯穿每个阶段。

## 阶段总览

```mermaid
flowchart LR
    BP0["BP0 真相与保护"] --> BP1["BP1 架构定案"]
    BP1 --> BP2["BP2 骨架与契约"]
    BP2 --> BP3["BP3 个人环境 MVP"]
    BP3 --> BP4["BP4 Work 与 Harness"]
    BP4 --> BP5["BP5 迁移与旧系统退役"]
    BP3 --> BP6["BP6 开源通用化"]
    BP5 --> BP6
    BP6 --> BP7["BP7 iOS 与扩展"]
```

| 阶段 | 目标 | 退出门 | 对应来源提案步骤 |
|---|---|---|---|
| **BP0 真相与保护** | 知道前身项目有什么、谁在写、哪些能恢复；资产先保护起来 | 旧仓库的 refs、未提交改动、搬移的 worktree 都有经过验证的保护副本；盘点有时间戳并列出 unknown | A0、A、B |
| **BP1 架构定案** | 内核拓扑、数据所有权、连接方案、威胁模型、组件去留 | BOB-D-005、BOB-D-006 被接受；同名实体语义对齐表完成；威胁模型通过审阅；组件去留表获批 | C、D、E |
| **BP2 骨架与契约** | 新仓库可以构建；内核 crate、Tauri 外壳、版本化契约、测试框架就位 | 桌面和 Android 能启动空壳；契约有 schema 和一致性测试；缓存隔离合规；CI 能跑 | F 的前置 |
| **BP3 个人环境 MVP** | 作者每天能用：PC 和手机上对话与捕获，事项连续，手机让 PC 处理文件 | F1、F2、F4 的验收在作者的真实设备上通过（真机、公网分别记录）；断线与重复发送场景通过 | F |
| **BP4 Work 与 Harness** | 事项晋升为 Work；Execution 和成果版本；接入 DSH、AGY、Codex；常开节点 | F5、F6 验收；每个 Harness 的能力矩阵有证据；同一事项跨 Harness 续接 | G、H（部分） |
| **BP5 迁移与旧系统退役** | 旧 App 0.9.x 和旧 Online 的数据与用户迁入；旧路径逐项退役 | 数据迁移可以读回、可以回退；旧版本只读可查；退役有用户确认 | G |
| **BP6 开源通用化** | 别人能装、能用、能升级 | LICENSE 与依赖许可证审计；去个人化；默认连接 L1+L2+L4；安装器；升级保留数据；安全报告渠道 | I、J |
| **BP7 iOS 与扩展** | iOS 评估；第二种实现验证窄 Provider 契约 | 由届时的真实需求决定 | I |

## 依赖说明

- BP3 可以先用作者的自建设施实现连接，但必须走 Connection Provider；BP6 只是增加默认 Provider，不改内核。
- BP4 期间，旧 Online 继续在生产运行；新内核的能力逐项验收后，才进入 BP5 的数据迁移。
- 旧项目里进行中的工作（ONL-MWNP-01、ONL-R06-D、APP-RH 系列、ONL-B4 HOLD）仍按旧项目合同推进或暂停，**不计入本项目进度**；需要并入的结论通过本项目的工作包显式引用。

## 来源

- `references/historical-plans/2026-10-28_BOB_UNIFIED_DEVELOPMENT_PLAN_PROPOSED.md`（用户提出的统一构想，A–J 步骤）
- `references/historical-plans/2026-10-28_unification_review.md`（遗漏审查与选项）
- 用户 2026-10-28 的确认：新目录、目标用户、Tauri Android、个人环境优先
