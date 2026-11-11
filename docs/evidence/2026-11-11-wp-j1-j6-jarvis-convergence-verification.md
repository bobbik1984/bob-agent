# BOB-D-015（WP-J1～WP-J6）Jarvis 统一主助理与桌面小组件全栈收敛本地验证回执

- **验证日期**：2026-11-11
- **关联决策**：`BOB-D-015`（`WP-J1`～`WP-J6`，`T01`～`T12` 12 步正交拆解）
- **运行环境**：Windows 本地离线验证环境（Rust 1.89 `--offline`、Node.js `--test`、Vite 6 生产构建；未修改线上实例、未向公网服务商发起付费调用）

---

## 1. 工作包交付物与代码落点核对（`WP-J1`～`WP-J6`）

| 工作包 | 任务卡 | 核心落盘模块 | 验证结果 |
|---|---|---|---|
| **WP-J1** 纯 Rust `BobNativeHarness` + SSE 网关 + 安全护栏 | `T01`, `T02`, `T04` | `crates/bob-runtime/src/{budget.rs, net_guard.rs, tool_tracker.rs, tool_bus.rs, native_harness.rs}`、`crates/bob-node/src/jarvis.rs` (`/api/jarvis/v1/stream`) | 通过（多服务商 OpenAI/Anthropic/Gemini SSE 解析、分级 `Action Gates`、月度 `$10` 上限 80% 预警/100% 拦截、SSRF 公网守卫、死循环熔断器、挂起恢复） |
| **WP-J2** Siri 流体彩色球 + 手机桌面小组件网格 + L1/L2/L3 降噪 | `T03`, `T05` | `apps/bob-app/src/components/{JarvisOrb.vue, ActionCard.vue, JarvisWidgetGrid.vue}`、`composables/useJarvisHome.mjs`、`PrimaryChatPane.vue`、`App.vue` | 通过（四态流体球、结构化/审批/接管卡片、主屏小组件增删排序、无外部节点时零极客术语 L1 首屏、移除主对话终端字符旋转棒） |
| **WP-J3** 无 Node 依赖功能包沙箱与 3 分钟试用自动回退 | `T06`, `T07` | `crates/bob-runtime/src/features.rs`、`crates/bob-node/src/lib.rs` (`/api/f/:pkg/:route`)、`apps/bob-app/src/components/{FeatureWidgetFrame.vue, FeatureTrialBanner.vue}` | 通过（零 Node 子进程依赖、8KB 存储配额、公网代理守卫、`<iframe sandbox="allow-scripts">` 无凭据隔离、持久化 3 分钟试用与最近 5 版回退） |
| **WP-J4** 统一时间调度、Routine 待批草稿、成长回路与多通道送达 | `T08`, `T09` | `crates/bob-runtime/src/{routines.rs, growth.rs}`、`crates/bob-kernel/src/kb_search.rs`、`crates/bob-node/src/push_channels.rs`、`apps/bob-app/public/{manifest.webmanifest, sw.js}` | 通过（`ScheduleItem` 联动、启用前强制 1 次试运行、2h 过期标 `missed`、无人值守 `Draft` 7 天保留与免模型重放、五阶段夜间做梦 `≤3` 条可恢复修剪、结构化晨报、四档主动关心与 `22:00–08:00` 免打扰、SQLite FTS5 全文检索、VAPID P-256 ES256 Web Push 与 `/v1/chat` 微信桥兼容） |
| **WP-J5** 纯 Rust `bob-browser` CDP 引擎与手机虚拟窗口接管 | `T10`, `T11` | `crates/bob-browser/src/*`、`crates/bob-node/src/browser_ws.rs` (`/api/ws/browser-takeover`)、`apps/bob-app/src/components/{VirtualBrowserModal.vue, SiteSessionVault.vue}` | 通过（带编号 AXTree 快照穿透 iframe 与开放 Shadow DOM、遮挡过滤、密码/验证码零入模硬拦截、CSWSH `Origin` 防劫持校验、手机/桌面视口切换、中文输入法与滑块拖动坐标穿透、禁止导出原始 Cookie SQLite 库） |
| **WP-J6** PC 受控本机工具、`V1→V2` 批注对照审阅、Tauri IPC 对齐与一键部署模板 | `T12` | `crates/bob-runtime/src/{host_tools.rs, runtime_rpc.rs}`、`apps/bob-app/src-tauri/src/{runtime_commands.rs, lib.rs}`、`apps/bob-app/src/components/ArtifactRevisionReview.vue`、`deploy/*`、`scripts/deploy-vps-oneclick.sh` | 通过（获准工作区边界与路径穿越/敏感文件拦截、`host_write_file` `SideEffect` 审批、`host_run_command` `Irreversible` 硬门禁、PC 离线 `waiting_for_pc` 队列对账、行级/框选批注携带 `parentVersionId` 发起 `V1→V2` 并排对照、脱敏容器部署脚本） |

---

## 2. 全栈四重自动化门禁实测结果

1. **Cargo 全工作区离线测试**：
   - 命令：`cargo test --offline --workspace`
   - 结果：**32 个测试套件共 359 项通过，0 失败**（1 项既有内核子进程夹具显式 ignored）：
     - `bob-browser`：27 项通过
     - `bob-kernel`：9 项单测 + 80 项集成测试通过
     - `bob-runtime`：158 项通过
     - `bob-node`：35 项通过（含真实 TCP `/api/jarvis/v1/stream` SSE 流式端到端、`/api/ws/browser-takeover` WebSocket 接管端到端、CSWSH 跨站 `Origin` 403 拦截、Routine 待批草稿生命周期、Web Push / 微信桥 `/v1/*` 端到端）
     - `bob-transport`：7 项单测 + 17 项双向 TLS 1.3 集成测试通过
2. **Tauri 桌面/移动端宿主离线编译校验**：
   - 命令：`cargo check --offline --manifest-path apps/bob-app/src-tauri/Cargo.toml`
   - 结果：**0 错误通过**（`bob-browser` 严格保持在节点侧，不污染 Android Tauri 编译图；35 项新增跨平台 RPC 在 `runtime_rpc.rs` 与 `runtime_commands.rs` 100% 对齐）。
3. **Vue 3 前端全量单测与静态规范扫描**：
   - 命令：`node --test tests/*.test.mjs`
   - 结果：**203 / 203 项全部通过，0 失败**（含零硬编码 `#hex`/`rgb()` 颜色扫描、零装饰性 emoji 扫描、`zh-CN.json` 与 `en-US.json` **1,060 / 1,060 键 100% 对称校验**）。
4. **生产静态包构建校验**：
   - 命令：`pnpm build`（`vite build`）
   - 结果：**179 个模块转换成功，0 报错**（3.30s 完成构建）。

---

## 3. 安全脱敏与未验边界声明（诚实区分）

- **去个人化与安全核验**：新增 17 个核心后端模块与 `deploy/`、`scripts/deploy-vps-oneclick.sh` 经自动化扫描确认 **0 处个人私有域名、0 处硬编码服务器 IP、0 处明文密钥**。
- **尚未执行的线上/真机范围（遵循重型操作防火墙，待用户明确授权后执行）**：
  1. 本轮所有验证均为本地离线单测、模拟 CDP/LLM 服务与本地真实 TCP/WebSocket 闭环，**尚未推送或重启线上生产节点**；
  2. **真实宿主 Chromium + 公网办公站点（含滑块/2FA）接管保活**、**iOS Safari 主屏幕真机 APNs 锁屏推送送达**、以及 **Android 真机覆盖安装与 Pixel 8 + PC 跨网实连** 仍须在获得部署与真机测试授权后逐项补齐真机回执，不将本地自动化全绿混同于生产真机终验。
