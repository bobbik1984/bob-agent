# bob-agent (v1.0.2)

**One Bob, many devices. Work keeps going. You own it.**

Bob 是一个按使用深度平滑生长的个人与工作流 Agent 系统（官网与手册：[bob.example.com](https://bob.example.com)）。

### 产品形态（横向能力倍增）：`0.1 · 1 · X`

`0.1`、`1`、`X` **不是前后淘汰的版本号**，而是同一个 Bob 体系通过不同模块组合产生的能力倍增：

| 形态 | 组合方式 | 核心价值 |
| :--- | :--- | :--- |
| **`0.1`** | [`bob-bot`](https://github.com/bobbik1984/bob-bot)（云端轻网关 + 浏览器 PWA） | **手机免安装、任意终端有浏览器即可直控**（无大厂 App 与移动端跨境网络依赖）；支持 iOS PWA Web Push / 潜在 Android APK 端云协同与 5 秒虚拟浏览器接管 2FA |
| **`1`** | **`bob-agent`（本仓库端侧客户端）** | 安装在手机（Android 1.0 APK）或电脑上**自主运行**；纯 Rust 单一内核（~38MB / ~50MB 常驻内存）、本地 SQLite 三层记忆与 OS 凭据保险箱 |
| **`X`** | `bob-online` + `bob-agent` + 多节点 | 7×24 云端主权中枢串联多台 PC / VPS / 移动节点，单一主窗口统筹调度、三档节点权限、连续同错熔断与外部 MCP 参谋长总线 |

### 真实三代演进（纵向 GitHub 历史）

- **Gen 1（2026.06）**：纯手机端个人助理原型起步（已归档至 `legacy/v0.9` 与历史归档仓）。
- **Gen 2（2026.07–10，`v0.6` → `v0.9.30`）**：扩展为手机 × 电脑双端联动（Tauri 桌面端、二维码配对、离线同步与本地图谱）。
- **Gen 3（2026.10–11，`1.0.x`）**：归档旧架构，重写为纯 Rust 统一内核（`bob-kernel / bob-runtime / bob-node / bob-browser`），并解耦出常驻中枢 `bob-online`、端侧自主 `bob-agent` 与轻量免装 `bob-bot`，形成 `0.1 / 1 / X` 自由组合体系。

> **当前施工基线（`1.0.2` 持续演进中）**：已完成纯 Rust 统一内核、Jarvis 主助理全栈能力（`WP-J1`～`WP-J6`）、本地底座与长期记忆（`WP-E1`）、多参谋长总线（`BOB-D-017`）与单一主窗口跨节点统筹闭环（`BOB-D-018`）；原 `0.9.x` 代码归档于 `legacy/v0.9` 分支。`bob-bot`（`0.1` 轻量化入口与快速交互实验仓）与本仓库严格分库并行。

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
