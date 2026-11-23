# 术语表与命名空间（A0）

> 稳定知识页。用来解决前身项目之间名称和 ID 冲突的问题（统一审查 G7）。有新术语时在此追加，不在别处另立定义。

## 1 项目与组件名称

| 名称 | 含义 | 不要和它混淆的 |
|---|---|---|
| **Bob（统一项目）** | 本仓库建设的统一个人助理 | — |
| **旧 App / legacy-app** | `<workspace_root>/bob-agent`，Tauri 版 Bob 0.9.x | 本仓库目录同样叫 `bob-agent`，口头和文档里一律称“统一项目” |
| **旧 Online / legacy-online** | `<workspace_root>/cloud-agent`，曾用名 Nexus、Bob Online、online-agent，包名 `antigravity-nexus`；生产实例部署在 Hostinger | 统一项目里的常开节点 `bob-node` |
| **bob-kernel**（架构已接受，BOB-D-005；尚未实现） | 统一项目唯一的领域内核 | 旧 App 的 Work Core（`APP`），旧 Online 的 Bob Core（`ONL`） |
| **bob-node**（已接受的部署形态；尚未实现） | 内核的无界面部署形态，跑在常开设备上 | 旧 Online 服务器 |
| **Home（主权威）** | 某个用户数据的主权威内核实例 | 设备副本（Replica） |
| **Replica（副本）** | 设备上的只读缓存加意图队列（outbox） | 第二份可写的真相 |

## 2 运行身份（沿用前身项目已被接受的区分）

- **Host**：执行主机。**Harness**：运行 Agent 的方式（DSH、AGY、Codex、Generic API 等）。**Workspace**：某台主机上的工作目录或边界。**Provider**：能力来源（模型、文件、连接等）。**Profile**：可选择的运行配置。**Project** 不等于以上任何一项。
- 模型字段区分 **Requested / Resolved / Observed**；没有实际观测时保持 `unknown`。
- 三个信任域相互独立：**设备配对**、**操作授权**、**Provider 认证**。

## 3 ID 命名空间

| 对象 | 统一项目 | 引用旧 App | 引用旧 Online |
|---|---|---|---|
| 决策 | `BOB-D-001` | `APP-D-008` | `ONL-D-001`、`ONL-D-WEB-01` |
| 长期阶段 | `BP0`…`BP7` | `APP-Phase-5.5` | `ONL-Phase-4` |
| 工作包 | `WP-00`… | `APP-RH-02` | `ONL-MWNP-01`、`ONL-R06-D`、`ONL-B4` |
| 功能 | `F1`… | — | — |
| 统一审查发现 | `G1`…`G14` | — | — |
| 架构候选决策 | `AD-1`…`AD-7`（已转为 BOB-D-005 至 BOB-D-007，其余尚未转换的留在 todo） | — | — |

提案中的步骤 A–J 只是来源材料里的依赖表达，已映射到 `BP0`–`BP7`（见 DEVELOPMENT_ROUTE），不再单独使用。
