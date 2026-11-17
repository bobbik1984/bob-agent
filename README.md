# bob-agent (v1.0.1)

**One Bob, many devices. Work keeps going. You own it.**

Bob 是一个属于个人用户的 Jarvis 级统一个人助理（PC 桌面端、Android 客户端、iOS/Web PWA 与可选 Linux 常驻节点共用同一纯 Rust 内核）：日常零噪音陪伴、待办日程与长期记忆持续衔接、云端浏览器与本机协作可接管、模型与专业开发工具按需展开。

> **当前形态（`1.0.1` 持续演进中）**：核心工作区已落地纯 Rust 内核与 Jarvis 主助理全栈能力（`WP-J1`～`WP-J6`、`WP-E1` 本地底座与长期记忆、`BOB-D-017` 多参谋长总线）；原 `0.9.x` 单机版代码已归档至 `legacy/v0.9` 分支。独立快速交互原型实验室 `bob-bot`（另行立项维护）与本生产主干严格分库运行，验证成熟的能力统一归并至本仓库工程化落地。

## 权威文档入口

- 为什么做、为谁、核心原则：[docs/PROJECT_VISION.md](docs/PROJECT_VISION.md)
- 现在在做什么、基线与待验门禁：[docs/CURRENT_STAGE.md](docs/CURRENT_STAGE.md)
- 系统边界与安全门禁：[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 长期阶段路线（`BP0`–`BP7` / `WP-J` / `WP-E`）：[docs/DEVELOPMENT_ROUTE.md](docs/DEVELOPMENT_ROUTE.md)
- 产品功能与验收标准：[docs/FEATURES.md](docs/FEATURES.md)
- 使用与接入说明：[docs/USER_GUIDE.md](docs/USER_GUIDE.md)
- Agent 工作宪法：[AGENTS.md](AGENTS.md)

## 仓库结构

```text
AGENTS.md            Agent 工作宪法（JIT 路由、门禁与验证命令）
todo.md              活跃需求与待补验证缺口池
progress.yaml        机器可读的阶段与任务状态投影
crates/
  bob-kernel/        核心状态机、SQLite 迁移、记忆/日程/Outbox/项目目录与 FTS5
  bob-transport/     双向 TLS 传输协议、帧规范与证书指纹校验
  bob-runtime/       BobNativeHarness 流式主脑、Tool Bus、Routine、做梦、多 Harness 适配
  bob-browser/       纯 Rust CDP 持久浏览器引擎与安全接管拦截
  bob-node/          tokio + axum 无界面常驻节点、SSE/WS 网关、Web Push 与受控 MCP
apps/
  bob-app/           统一 Vue 3 UI（Desktop Tauri / Android Tauri / Web & iOS PWA）
deploy/              容器与反代参考模板（Dockerfile / compose / Caddyfile.example）
scripts/             自动化部署、运维 bootstrap、一致性校验与迁移辅助脚本
docs/                权威设计文档、wiki 主题页与脱敏验证证据（evidence/）
references/          历史来源规划材料（只读参考，非当前指令）
```

许可证：待定（`BP6` 开源通用化前确定）。
