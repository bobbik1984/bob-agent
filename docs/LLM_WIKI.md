# Bob — LLM Wiki（知识路由）

> 轻量索引：按主题说明什么时候读、读哪份 Primary 文档、相关决策和关键规则。不要默认加载全部 wiki。

| 主题 | 什么时候读 | Primary | 相关决策 | 关键规则 |
|---|---|---|---|---|
| 产品定位与范围 | 任何需求或设计讨论开始前 | [PROJECT_VISION](PROJECT_VISION.md) | BOB-D-002、BOB-D-004 | 单用户多设备；不做企业协同 |
| 名称与 ID | 引用前身项目、写决策或任务时 | [wiki/glossary.md](wiki/glossary.md) | BOB-D-007 | 旧 ID 必须加 `APP-` 或 `ONL-` 前缀 |
| 内核与数据所有权 | 设计存储、同步或实体时 | [ARCHITECTURE](ARCHITECTURE.md) §2–3 | BOB-D-005（accepted） | 每个对象一个写入者；Home 加 Replica；不做多主合并 |
| Schema / 可靠发送 | 实施storage、预热、Outbox、接收回执或runner前 | [wiki/kernel-schema.md](wiki/kernel-schema.md) | BOB-D-004/005/008 | 迁移0001–0008已有实现；当前验收见CURRENT_STAGE；本地接纳≠远端收到；unknown先对账 |
| 跨设备连接 | 涉及配对、中继、离线、手机访问 PC 时 | [wiki/connectivity.md](wiki/connectivity.md) | BOB-D-006（accepted，设计） | 个人设施只能经 Provider 配置；显示连接层级；实现和公网验收分开 |
| 会话、项目与草稿 | 实施侧栏、新建/切换、重启恢复和真正续聊时 | [wiki/kernel-schema.md](wiki/kernel-schema.md) 多会话增量合同；[FEATURES](FEATURES.md) F1/F2 | BOB-D-004/005/008 | 逻辑会话与原生会话分开；草稿逐设备/会话；多会话已有实现，当前增量与真机证据分别核对 |
| 设置和 Online 接入 | 设备图标、服务登录、旧项目接续或云端角色问题 | [FEATURES](FEATURES.md) 设置分组；[wiki/connectivity.md](wiki/connectivity.md) §8 | BOB-D-004/005/006 | 设备子页不等于服务登录；Online不自动成为Home |
| 安全与凭据 | 涉及授权、密钥、远程文件时 | [ARCHITECTURE](ARCHITECTURE.md) §1、§5；[威胁模型](wiki/threat-model.md) | — | 三个信任域分离；凭据留在边缘 |
| 前身项目资产 | 打算复用旧代码或数据时 | [COMPONENT_MAP](COMPONENT_MAP.md)，[初盘](evidence/2026-10-28-legacy-inventory.md)、[WP-01深盘](evidence/2026-10-28-wp01-deep-inventory.md) | BOB-D-001 | 旧仓库只读；迁入必须走获批的工作包 |
| 当前施工 | 每次任务 | [CURRENT_STAGE](CURRENT_STAGE.md) | — | 允许和禁止范围以任务卡为准 |
| 多设备项目与跨节点交接 | 设备离线、GitHub同步、换机器/工具继续或按设备分组时 | [wiki/kernel-schema.md](wiki/kernel-schema.md) 项目/设备副本合同；[FEATURES](FEATURES.md) 离线表现 | BOB-D-004/005/008 | 同项目多Workspace；Git不证明完整最新状态；交接前不抢占未知执行 |
| 多节点统和、子代理与项目画像胶囊 | 设计跨设备能力互借（海外模型出口、`bobbik-pc` 渲染节点）、快慢双轨、Subagent 或防跑偏任务卡时 | [wiki/kernel-schema.md §10](wiki/kernel-schema.md#10-多节点能力互借项目画像胶囊与快慢双轨子代理契约2026-10-31bob-d-010)；[ARCHITECTURE](ARCHITECTURE.md) §4 | BOB-D-010/011（accepted，设计） | 原生会话直接续聊；胶囊是可追溯索引；先单执行者后子代理；实施唯一看CURRENT_STAGE §3；供应商规则逐项核验 |
| 统一构想的来源 | 追溯“为什么这样规划” | `references/historical-plans/` | — | 来源材料，不是指令 |

阅读顺序：AGENTS → CURRENT_STAGE（事实/下一步）→ 对应 todo 条款与 wiki；需要长期阶段再读 DEVELOPMENT_ROUTE。当前事实审计见 [2026-10-31 Evidence](evidence/2026-10-31-development-route-audit.md)，日期材料不另立计划。

UI 修改入口：[DESIGN_PRINCIPLES](DESIGN_PRINCIPLES.md)；覆盖无 emoji、单圆点连接状态、启动会话恢复、无重复 Logo/嵌套焦点框与悬浮文字/图片灵感记录。


独立实例、默认助理与共享对象归属：先读 DECISIONS 的 BOB-D-012，再读 ARCHITECTURE §3、FEATURES 默认助理验收和 CURRENT_STAGE 当前增量。Home/Replica只用于明确共享关系；不得将历史X1C主库部署选择当成全产品前提。
