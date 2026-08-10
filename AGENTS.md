# Local Agent Guide: bob-agent

> 适用于在本仓库工作的所有编码代理。先读本文件，再按任务类型 JIT 加载相关文档。

## 1. [The Desk] 当前开发焦点

- **产品定位**：Bob 是让复杂工作不断线的 Personal Work Orchestration Layer；不是 Codex/Claude Code 的外壳。
- **当前主线**：`v0.8.1` Phase 0–1，建立 Persistent Work Core、Project State 与 Decision 一等对象。
- **推进顺序**：Work Core → 现有入口关联 → Decision/Change → Complexity Router → Advanced Project Loop → Dynamic Graph → Runtime Adapter。
- **当前真实边界**：Auto 尚不会升级到 Goal；Goal 尚无持久 DAG 和重启恢复；Dream 以事实整理为主，尚未形成结果驱动的用户模型。
- **核心约束**：PC 安装版/绿色版保持零外部运行时，Android 不增加用户侧依赖；客户端体积增长必须解释和验证。
- **工作区状态提醒**：开始修改前先检查 `git status`，不得覆盖用户未提交的代码或临时文件。

## 2. 项目红线

1. **纯 Tauri**：系统能力、网络、文件和数据库逻辑放在 Rust；不得恢复 Electron 后端。
2. **Bridge 唯一入口**：Vue 组件继续调用 `window.electronAPI.*`；Tauri `invoke` 只出现在 `src/tauri-bridge.js`。
3. **零依赖体验**：不得为了 Goal、DAG、Dream 或同步增加 Python/Node 用户运行时；Relay 的 Node 依赖只存在于服务器。
4. **Goal 不得虚假完成**：执行者不能单方面宣布 Done；证据不足使用 `unverified`，失败必须定位到节点或验证规则。
5. **Memory 边界**：SOUL 只保存稳定身份与交互原则；工具失败进入 procedural/diagnostic memory，不得污染 SOUL。
6. **权限不降级**：Goal/Auto/远程来源都不能绕过 R0–R3 Policy Engine；不可逆、外部影响和批量操作仍需确认。
7. **UI 一致性**：只用 Lucide 图标，禁用 Emoji；用户可见文本必须同步 `zh-CN.json` 与 `en-US.json`；颜色使用设计变量。
8. **隐私与凭证**：不得提交 `.env`、`data/` 或用户记忆，不得硬编码 API Key。
9. **稳健错误处理**：Tauri Command 返回可读 `Result`；生产路径禁止新增 `unwrap()`/`panic!()`。
10. **文档同步**：改变真实能力、数据契约或路线图时，同步更新对应 SSOT、`todo.md`、`progress.yaml`；不得把目标架构写成已完成功能。

## 3. JIT 路由

| 任务 | 修改前必须阅读 |
|---|---|
| 产品方向、功能边界 | `docs/PRODUCT_VISION.md`、`docs/BOB_EVOLUTION_ROADMAP.md` |
| 跨模块长期架构决定 | `docs/DECISIONS.md` |
| Work Continuity 完整阶段与质量门 | `docs/superpowers/plans/2026-08-10-work-continuity-evolution-plan.md` |
| Capture、分享、笔记、知识/行动分流 | `docs/CAPTURE_BASELINE.md`、`todo.md` 当前主线 |
| 可迁移知识对象、目录边界与迁移 | `docs/KNOWLEDGE_SCHEMA.md`、`docs/superpowers/specs/2026-08-10-unified-knowledge-lifecycle-design.md` |
| Project State、Goal、自动路由、DAG、Verifier、Dream/Memory | `docs/GOAL_RUNTIME.md`、`docs/DECISIONS.md`、`todo.md` 对应目标 |
| Rust 模块、IPC、数据库、状态机 | `docs/ARCHITECTURE.md`、`LLM_WIKI.md` |
| UI、布局、颜色、图标、i18n | `design_principles.md` |
| 跨端同步、Relay、移动端配对 | `docs/MOBILE_BLUEPRINT.md`、`todo.md` 目标 23/30 |
| 用户可见功能和操作说明 | `docs/FEATURES.md`、`docs/USER_GUIDE.md` |
| 发布、安装器、版本 | `scripts/release.bat`、`OPEN_SOURCE_WORKFLOW.md`、`CHANGELOG.md` |
| 历史 Electron 设计 | `docs/agents_electron.md`，仅用于考古，不得作为当前实现依据 |

文档职责：`PRODUCT_VISION.md` 定方向；`BOB_EVOLUTION_ROADMAP.md` 定阶段；`README.md` 讲产品入口和真实能力；`ARCHITECTURE.md` 讲当前系统；`LLM_WIKI.md` 做代码导航；`todo.md` 管当前可执行任务；`progress.yaml` 是看板源。

## 4. 本地验证循环

按改动风险选择最小充分验证，不要无意义生成大量产物：

```powershell
npm test
npm run build
Set-Location src-tauri; cargo check
```

- Rust/数据库/同步修改：运行相关 Rust 单测；跨端协议同时运行 Relay 故障注入测试。
- UI 修改：先用 `npm run dev` 的 `localhost:5173` 验证；需要原生能力时再运行 `npm run dev:tauri`。
- Goal/Dream 修改：必须覆盖状态机、暂停恢复、预算、权限、证据缺失、模型超时和进程重启。
- 发布只运行 `scripts\release.bat`，不得用默认 NSIS/MSI 代替项目 Bootstrapper。
- 发布前记录 PC 安装版、绿色版和 Android APK 的体积变化。

## 5. 完成定义

一项开发任务只有同时满足以下条件才算完成：

- 代码行为与任务验收条件一致；
- 相关测试或可复现检查通过；
- 用户可见失败有准确状态与可理解日志；
- 没有绕过权限、隐私、零依赖和体积约束；
- 权威文档反映真实状态，目标能力未被误写为已完成；
- `git diff` 中没有无关修改或用户文件损失。
