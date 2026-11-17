# Bob — LLM Wiki（知识路由）

> 轻量索引：按主题说明什么时候读、读哪份 Primary 文档、相关决策和关键规则。不要默认加载全部 wiki。

| 主题 | 什么时候读 | Primary | 相关决策 | 关键规则 |
|---|---|---|---|---|
| 产品定位与范围 | 任何需求或设计讨论开始前 | [PROJECT_VISION](PROJECT_VISION.md) | BOB-D-002、BOB-D-004、BOB-D-015 | 单用户多设备；L1/L2/L3 按需展开；`bob-agent` 与 `bob-bot` 分立 |
| 名称与 ID | 引用前身项目、写决策或任务时 | [wiki/glossary.md](wiki/glossary.md) | BOB-D-007 | 旧 ID 必须加 `APP-` 或 `ONL-` 前缀 |
| 内核与数据所有权 | 设计存储、同步或实体时 | [ARCHITECTURE](ARCHITECTURE.md) §2–3 | BOB-D-005 / BOB-D-012 | 每个对象一个写入者；实例独立，共享按来源版本同步；不做多主合并 |
| Schema / 可靠发送 | 实施 storage、预热、Outbox、接收回执或 runner 前 | [wiki/kernel-schema.md](wiki/kernel-schema.md) | BOB-D-004/005/008 | 迁移 `0001`–`0014`；本地接纳≠远端收到；`unknown` 先对账 |
| 跨设备连接 | 涉及配对、中继、离线、手机访问 PC 时 | [wiki/connectivity.md](wiki/connectivity.md) | BOB-D-006（accepted） | 个人设施不进源码；显示连接层级；实现与公网/真机验收分开 |
| 会话、项目与草稿 | 实施侧栏、新建/切换、重启恢复和续聊时 | [wiki/kernel-schema.md](wiki/kernel-schema.md)；[FEATURES](FEATURES.md) F1/F2 | BOB-D-004/005/008 | 逻辑会话与原生会话分开；草稿逐设备/会话；收件箱仅列正式项目 |
| 正式项目 SSOT、侧栏降噪与 L1/L2 vs L3 门禁分层 | 修改项目目录同步、测试折叠、`/opt` 排除或 `full_auto` 权限时 | [ARCHITECTURE](ARCHITECTURE.md)；[PROJECT_VISION](PROJECT_VISION.md) | BOB-D-015 / BOB-D-017 | 仅同步 `project_registry.yaml` 正式开发项目（`bob_project: false` 排除运行态）；`full_auto` 仅限 L3 节点编程会话且硬拦截灾难命令，绝不放宽 L1/L2 审批门 |
| 设置与 Online 接入 | 设备图标、服务登录、旧项目接续或云端角色问题 | [FEATURES](FEATURES.md)；[wiki/connectivity.md](wiki/connectivity.md) §8 | BOB-D-004/005/006 | 设备子页不等于服务登录；Online 不自动成为 Home |
| 安全与凭据 | 涉及授权、密钥、远程文件时 | [ARCHITECTURE](ARCHITECTURE.md) §1、§5；[威胁模型](wiki/threat-model.md) | — | 三个信任域分离；凭据留在边缘；快照不返明文 |
| 前身项目资产 | 打算复用旧代码或数据时 | [COMPONENT_MAP](COMPONENT_MAP.md)、[初盘](evidence/2026-10-28-legacy-inventory.md)、[WP-01 深盘](evidence/2026-10-28-wp01-deep-inventory.md) | BOB-D-001 | 旧仓库只读；迁入必须走获批工作包 |
| 当前施工 | 每次任务必读（L0） | [CURRENT_STAGE](CURRENT_STAGE.md) | — | 允许与禁止范围、已验与未验边界以任务合同为准 |
| 多设备项目与跨节点交接 | 设备离线、GitHub 同步、换机器/工具继续或按设备分组时 | [wiki/kernel-schema.md](wiki/kernel-schema.md)；[FEATURES](FEATURES.md) | BOB-D-004/005/008 | 同项目多 Workspace；Git 不证明最新完整状态；交接前不抢占未知执行 |
| 节点工具认证维护与多 Harness 适配 | 编写或调试 DSH / AGY / Codex / Claude / Pi / Hermes / OpenClaw 探测与接续时 | [wiki/harness-auth.md](wiki/harness-auth.md)；[ARCHITECTURE](ARCHITECTURE.md) | BOB-D-010/011/013 | 统一五动作；原生持久接口优先；禁止凭据提取与终端刮屏 |
| 多节点互借、子代理与胶囊 | 设计能力互借、快慢双轨、Subagent 或任务卡胶囊时 | [wiki/kernel-schema.md §10](wiki/kernel-schema.md#10-多节点能力互借项目画像胶囊与快慢双轨子代理契约2026-10-31bob-d-010)；[ARCHITECTURE](ARCHITECTURE.md) §4 | BOB-D-010/011 | 原生会话直接续聊；胶囊是可追溯索引；先单执行者后有界子代理 |
| 去 `X1C` 与可插拔多参谋长总线 | 调整跨节点态势、`/api/bot/v1/mcp`、`chief_orders` 或外部助理权限时 | [ARCHITECTURE](ARCHITECTURE.md)；[DECISIONS](DECISIONS.md) `BOB-D-017` | BOB-D-017 | `X1C` 为对等终端；外部参谋长只可读战报与提交 `pending_approval` 指令，严禁越权批准或执行 |
| Jarvis 主助理、小组件沙箱与云浏览器接管 | 修改水滴球/小组件主屏、Tool Bus、功能包试用、Routine 或虚拟浏览器窗口时 | [ARCHITECTURE](ARCHITECTURE.md)、[COMPONENT_MAP](COMPONENT_MAP.md) §1.1 | BOB-D-015 | 零 Node 依赖；无硬编码颜色与装饰 emoji；密码/验证码零入模；`WP-J1`～`WP-J6` 已落盘 |
| 长期记忆、阶段摘要与多问题覆盖 | 修改 `assistant_context`、遗忘边界或问题覆盖追踪时 | [ARCHITECTURE](ARCHITECTURE.md)、[2026-11-17 证据](evidence/2026-11-17-assistant-memory.md) | BOB-D-015 / `WP-E1-M` | 记忆有来源与作用域，遗忘切断旧上下文且拒绝过期审批；记忆不授予执行权限 |
| 白领生产力、三路径向导与生态扩展 | 承接 `WP-E1` 剩余门禁及 `WP-E2`～`WP-E4` 时 | [DEVELOPMENT_ROUTE](DEVELOPMENT_ROUTE.md)、[CURRENT_STAGE](CURRENT_STAGE.md) §2–§3、[ARCHITECTURE](ARCHITECTURE.md) | BOB-D-016 | 保持 L1 极简体验与零 Node 铁律；外部 MCP 写操作默认 `SideEffect` |
| UI 规范与交互约束 | 修改导航、输入框、水滴球、日历或收集箱前 | [DESIGN_PRINCIPLES](DESIGN_PRINCIPLES.md) | BOB-D-014/015 | 无装饰 emoji；CSS Variables 主题；i18n 双语对称 |
