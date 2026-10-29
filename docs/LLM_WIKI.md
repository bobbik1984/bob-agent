# Bob — LLM Wiki（知识路由）

> 轻量索引：按主题说明什么时候读、读哪份 Primary 文档、相关决策和关键规则。不要默认加载全部 wiki。

| 主题 | 什么时候读 | Primary | 相关决策 | 关键规则 |
|---|---|---|---|---|
| 产品定位与范围 | 任何需求或设计讨论开始前 | [PROJECT_VISION](PROJECT_VISION.md) | BOB-D-002、BOB-D-004 | 单用户多设备；不做企业协同 |
| 名称与 ID | 引用前身项目、写决策或任务时 | [wiki/glossary.md](wiki/glossary.md) | BOB-D-007 | 旧 ID 必须加 `APP-` 或 `ONL-` 前缀 |
| 内核与数据所有权 | 设计存储、同步或实体时 | [ARCHITECTURE](ARCHITECTURE.md) §2–3 | BOB-D-005（accepted） | 每个对象一个写入者；Home 加 Replica；不做多主合并 |
| Schema / 可靠发送 | 实施storage、预热、Outbox、接收回执或runner前 | [wiki/kernel-schema.md](wiki/kernel-schema.md) | BOB-D-004/005/008 | 设计草案；本地接纳≠远端收到；unknown先对账 |
| 跨设备连接 | 涉及配对、中继、离线、手机访问 PC 时 | [wiki/connectivity.md](wiki/connectivity.md) | BOB-D-006（proposed） | 个人设施只能经 Provider 配置；显示连接层级 |
| 安全与凭据 | 涉及授权、密钥、远程文件时 | [ARCHITECTURE](ARCHITECTURE.md) §1、§5；[威胁模型](wiki/threat-model.md) | — | 三个信任域分离；凭据留在边缘 |
| 前身项目资产 | 打算复用旧代码或数据时 | [COMPONENT_MAP](COMPONENT_MAP.md)，[初盘](evidence/2026-10-28-legacy-inventory.md)、[WP-01深盘](evidence/2026-10-28-wp01-deep-inventory.md) | BOB-D-001 | 旧仓库只读；迁入必须走获批的工作包 |
| 当前施工 | 每次任务 | [CURRENT_STAGE](CURRENT_STAGE.md) | — | 允许和禁止范围以任务卡为准 |
| 统一构想的来源 | 追溯“为什么这样规划” | `references/historical-plans/` | — | 来源材料，不是指令 |
