# BOB-D-016（WP-E1：WP-E1.1～WP-E1.4）系统性进化第一波次本地全栈验证回执

- **验证日期**：2026-11-14
- **关联决策**：`BOB-D-016` 第一波次 `WP-E1`（`WP-E1.1` 容器/浏览器 CJK 与容错加固、`WP-E1.2` Clerk/Worker 快慢双模分流、`WP-E1.3` 3 步图形化开箱向导与一次性配对码、`WP-E1.4` 小组件生成提示词与沙箱调优）
- **运行环境**：Linux 本地离线验证环境（外置构建缓存、Rust 1.89 `--offline`、本地真实无头 Chromium/Edge CDP 实测、Node.js `--test`、Vite 6 生产构建；未修改或重启线上常驻生产实例、未打包 Android APK、未向商业服务商发起付费调用）

---

## 1. 子工作包交付物与代码落点核对（`WP-E1.1`～`WP-E1.4`）

| 任务卡 | 核心落盘模块（仓库相对路径） | 关键实现与验证结果 |
|---|---|---|
| **`WP-E1.1`** 常驻节点无头浏览器实装（CJK 字体 + 隔离卷）与接管恢复 | `deploy/Dockerfile`、`scripts/deploy-vps-oneclick.sh`、`crates/bob-browser/src/{launcher.rs, cdp.rs, screencast.rs, lib.rs}`、`crates/bob-node/src/{browser_ws.rs, lib.rs}`、`apps/bob-app/src/components/VirtualBrowserModal.vue` | **通过**：容器运行时与一键装机脚本补齐 `fonts-noto-cjk` / `fonts-noto-color-emoji` / `fonts-liberation2` 字体回退（零 Node.js 运行时依赖）；`secure_and_prepare_profile_dir` 强制 `--user-data-dir` `0700` 权限并清理残留 `SingletonLock` / `SingletonCookie` / `SingletonSocket`；`ChromiumManager` 检测进程异常退出自动重启恢复；`validate_ws_origin` 覆盖 HTTPS/WSS 反代同源校验并阻断协议降级与跨站劫持；支持 `ImeSetComposition` / `ImeCommit` 中文输入法组合与 390px 视口 `buttons: 1` 平滑滑块拖拽；新增 `app_browser_takeover_save_routine` 将人工接管操作脱敏导出为 `RoutineStore` 可复跑任务；在本地真实无头浏览器跑通 CJK 渲染、滑块、IME、崩溃重启与持久化 Cookie 恢复 |
| **`WP-E1.2`** 置顶 Jarvis `Resident Clerk + Async Worker` 快慢双模分流 | `crates/bob-runtime/src/{model_presets.rs, worker_ledger.rs, native_harness.rs, tool_bus.rs, runtime_rpc.rs}`、`crates/bob-node/src/jarvis.rs`、`apps/bob-app/src/composables/{useJarvisHome.mjs, useChatSession.mjs}`、`apps/bob-app/src/components/PrimaryChatPane.vue` | **通过**：新增 `model_presets.rs` 严格从 `model_providers.json` 提取 10 家主流服务商推荐 `clerk_model` + `maker_model` 配对（零虚构模型 ID，支持单 API Key 一键双模绑定与自定义覆盖）；`/api/jarvis/v1/stream` 拆分为秒级首字响应的 `Resident Clerk`（轻量个人事务工具 + `dispatch_background_task`）与后台独立线程 `Maker Worker`（经共享 `SharedAppAccessor` 按工具粒度短锁访问 `LocalApp`，长流式推理期间零阻塞）；落地 `WorkerTaskStore` 持久化 `StepLedger`、断线重连增量同步（`app_jarvis_worker_tasks` / `app_jarvis_worker_cancel`）、以及对齐 `SUPER_AGENT_NOTES.md` 的 **Thread 订阅直通与追问事件批处理**（活跃 Worker 会话中途追问直通入队并在下一轮迭代前合并注入，零额外 Clerk 调用） |
| **`WP-E1.3`** 首次访问 3 步图形化开箱向导与一次性配对码鉴权 | `crates/bob-node/src/{onboarding.rs, node_access.rs, lib.rs, main.rs}`、`apps/bob-app/src/{http-bridge.mjs, App.vue}`、`apps/bob-app/src/components/OnboardingWizard.vue`、`apps/bob-app/src/locales/{zh-CN.json, en-US.json}` | **通过**：未初始化实例启动时由 CSPRNG 生成一次性 6 位初始配对码（30 分钟有效、常量时间比对、连续 3 次错误触发指数退避 `429 PAIRING_RATE_LIMITED`、已认领实例防抢注拦截 `403 ONBOARDING_ALREADY_CLAIMED`）；既有已配置实例自动识别 `needsOnboarding: false` 零打扰；只读 `/api/node/v1/onboarding/status` 仅暴露布尔状态与预设服务商卡片，零凭据/系统信息回传；前端落地响应式 `<OnboardingWizard />` 三步引导（Step 1 配对码与访问口令 → Step 2 服务商卡片 + `/models` 实连探测 + Clerk/Maker 自动配对 → Step 3 可选助理命名与风格写入 `SOUL.md` 或跳过直达 Jarvis 水滴主屏） |
| **`WP-E1.4`** 对话现场生成与迭代桌面小组件沙箱调优 | `crates/bob-runtime/src/{features.rs, native_harness.rs}`、`apps/bob-app/src/components/FeatureWidgetFrame.vue`、`apps/bob-app/src/composables/useJarvisHome.mjs` | **通过**：固化 `WIDGET_SANDBOX_PROMPT_SPEC` 与 `validate_widget_sandbox_html` 静态门禁（拦截外链 `<script src>` / `<link href>` / `@import` CDN 与硬编码 `#hex` / `rgb()` / `hsl()` 色值，强制使用宿主透传 `--bg-*`、`--text-*`、`--accent-primary` 与 `bob.call` / `bob.save` `<= 8KB` 契约）；`FeatureWidgetFrame.vue` 将 `srcdoc` 计算与可变 `cached_state` / `storage` 解耦消除重挂载白屏闪烁，并捕获沙箱运行时 `window.onerror` / `unhandledrejection` 触发 `app_feature_report_error`（试用期组件自动回退并展示恢复条）；完成 `daily_focus_matrix` 与 `quote_margin_calc` 两个实用白领小组件从创建、3 分钟试用迭代、保留/回退到 5 版本历史演进的完整闭环验证 |

---

## 2. 全栈四重自动化门禁实测结果

1. **Cargo 全工作区离线测试**：
   - 命令：`cargo test --offline --workspace`
   - 结果：**26 个非空测试套件共 386 项通过，0 失败，0 编译警告**（1 项既有内核子进程夹具显式 ignored）：
     - `bob-browser`：**31 项通过**（含本地真实无头浏览器 CJK 渲染、390px 滑块拖拽、IME 组合输入、进程崩溃自动重启与 `--user-data-dir` `0700` Cookie 持久化恢复）
     - `bob-kernel`：**9 项单测 + 100 项集成测试通过**（共 109 项）
     - `bob-runtime`：**166 项通过**（新增双模预设目录与连通性探测、持久 `StepLedger` 与追问批处理、小组件静态门禁与多版本试用自动回退等单测）
     - `bob-node`：**46 项通过**（新增 `WP-E1.1` 反代 `Origin` 矩阵与真实无头浏览器 WebSocket 接管转存 Routine 端到端、`WP-E1.2` `<1s` Clerk 快响应 + 异步 Maker Worker + Thread 直通合并追问 + 断线重连恢复端到端、`WP-E1.3` 3 步开箱配对码限流/防抢注/实连探测/`SOUL.md` 落盘端到端）
     - `bob-transport`：**7 项单测 + 17 项双向 TLS 1.3 集成测试通过**（共 24 项）
2. **Tauri 桌面/移动端宿主离线编译校验**：
   - 命令：`cargo check --offline --manifest-path apps/bob-app/src-tauri/Cargo.toml`
   - 结果：**0 错误通过**（新增 `app_model_presets_list`、`app_jarvis_worker_tasks`、`app_jarvis_worker_cancel`、`app_feature_report_error` 在 `runtime_rpc.rs` 与 `runtime_commands.rs` 100% 对齐）。
3. **Vue 3 前端全量单测与静态红线扫描**：
   - 命令：`node --test tests/*.test.mjs`（于 `apps/bob-app`）
   - 结果：**236 / 236 项全部通过，0 失败**（含 `onboarding-wizard.test.mjs`、`jarvis-dual-mode.test.mjs`、`feature-widgets.test.mjs`、`virtual-browser-ui.test.mjs`，零硬编码颜色扫描、零装饰性 emoji 扫描、`zh-CN.json` 与 `en-US.json` **1,151 / 1,151 叶子键 100% 对称校验**）。
4. **生产静态包构建校验**：
   - 命令：`pnpm build`（`vite build`）
   - 结果：**185 个模块转换成功，0 报错**（3.72s 完成构建）。

---

## 3. 安全脱敏与未验边界声明（诚实区分）

- **架构与脱敏红线核验**：
  1. `deploy/Dockerfile` 运行时阶段与 `crates/` 保持 **零 Node.js 运行时依赖**；
  2. 新增与修改的全部源码、测试与本文档经自动化扫描确认 **0 处个人私有域名（示例统一使用 `bob.example.com`）、0 处私有服务器/局域网/Tailscale IP、0 处宿主机绝对路径、0 处明文密钥**；
  3. 前端组件严格使用语义化 CSS Variables，**0 处硬编码色值、0 处装饰性 emoji**。
- **尚未执行的线上/真机范围（遵循重型操作边界，待用户授权后执行）**：
  1. 本轮按授权仅完成本地源码实现与全套离线/本地真实无头浏览器自动化验收，**未重启或变更线上常驻生产节点，未打包 Android APK**；
  2. 线上常驻容器挂载真实公网办公站点测试账号的跨天 Cookie 保活与真实商业大模型 API Key 下的在线交互录屏，留待用户授权部署候选/生产节点后补充线上实测回执。
