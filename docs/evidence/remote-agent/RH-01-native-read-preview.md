# RH-00/RH-01 通用接入与 DSH 浏览子集

日期：2026-10-21。基线 `main@3876a46`、版本 0.9.27。用户批准实施最小通用接入框架并以 X1T DSH 为首接；本报告仅描述当前本地浏览子集，不能作为 RH-02 指令执行或 X1T 现场验收证据。

## 变更范围

- Rust `remote_agent` 模块：品牌中立契约/句柄，DSH rc.2 HTTP + Mux 实现；Cookie 内存隔离、无初始化网络工作、认证失败/跳转/断线错误收口。
- Work 按需加载 `RemoteAgentPanel`，Bridge 独占 IPC；手机可收起项目列表，近期用户/助手文字快照、手动刷新。既有本机与配对 PC 流程不改写。
- 能力诚实降级：send/approvals/artifacts 均为 false。当前没有执行任务、导入项目、下载文件、或服务器令牌撤销。
- 原始 token/Cookie 不落盘、不同句柄不混用；网络错误只返回固定错误码。远端文本不执行 HTML、不打开本地路径。

## 上游核对与限制

固定 `deepseek-ai/deepseek-harness@dsh-v0.2.1-rc.2` 的 browser-auth、RPC/Mux、workspace-controller、session-controller、core/session/types、llm/message 和 session-title/types 源码。

- `/connect` 为 302 + Cookie，`interactions=none`；严格核对 Mux 目的地址，原生 HTTP 与 WS 使用同一 Cookie。
- `session/list` 发送 `client-request` 封套及命名 `_request` 参数；响应必须同 rpcId 且 `result.ok=true`。
- `workspace/follow` 基线、`session/follow` 快照；标题是 `projections.values.title` 字符串，助手文字在 `data.message.content`，不能按用户消息的 `data.content` 读取。
- `session/follow` 可能激活宿主已有 Agent；只读指 Bob 没有 prompt/变更入口，不是 owner 凭据受服务端只读约束。
- 非文本、工具、注入上下文、附件及其他过程记录不展示。当前快照不是完整历史折叠；不自动分页/重连，不声称在线/任务已完成。

## 本地可复现证据

| 验证 | 结果 | 边界 |
|---|---|---|
| `cargo test --offline --lib remote_agent` | 4 passed，0 failed | 编译实际 Bob 库与 Tauri 命令；本机 Axum HTTP + WebSocket 夹具，不是 X1T |
| `pnpm --config.verify-deps-before-run=false run test` | 15 files / 79 passed | 含 7 项新增连接、清理、迟到历史、能力限制与错误脱敏测试；2026-10-22 版本升至 0.9.28 后复跑通过 |
| `node scripts/test-remote-agent-ui.mjs` | mobile + desktop passed | 使用现有 bundled Playwright + Edge；390×844 中文、1280×900 英文；模拟 Bridge 数据，不是已登录远端 |

Rust 用例覆盖 URL/明文限制、秘密不进入 ConnectionInfo、跨端点跳转拒绝、401、断线、错 rpcId、真实本地 Cookie/Mux/HTTP 读链、错误会话拒绝、用户与助手文本投影、关闭后请求拒绝。没有使用未授权真实 token。

前端测试覆盖零启动调用、可替换适配器数据、关闭面板后迟到连接释放、切换会话不串记录、断线明确错误、断开后的迟到历史忽略。浏览器覆盖实际 Vue 组件编译与交互、两个宽度无横向溢出、HTML 只呈现为文字、断开后清空 token/URL 查询。截图在本机临时目录 `bob-remote-agent-ui/`，包含模拟数据，不入库。

首次普通 `pnpm test` 被 pnpm 的自动依赖检查阻断（缓存权限错误）；随后显式关闭自动依赖安装检查，仅复用现有依赖执行。没有修改依赖清单/lockfile或安装工具。Cargo 使用既有外置缓存，在获准的测试执行上下文运行；131 条现有库 warning 未在本任务批量整改。

浏览器复现：先用现有依赖启动仅监听 127.0.0.1 的 Vite 开发服务；将 `BOB_BROWSER_MODULE` 指向已安装的 Playwright 入口（不要求安装新包），执行脚本。`BOB_UI_TEST_URL` 默认本机 5178；脚本拦截非本机 HTTP 请求。测试 HTML 不属于生产打包入口。

## 未完成与后续门槛

RH-00/RH-01 **部分实现/本地验证**。未验证实际 X1T 版本、认证、反代、既有网页重连根因；没有公网端到端、Android 原生网络/生命周期、候选安装测试。没有调用 provider、Blender、Godot 或任何真实执行命令。

下一步按规格第 8 节：不以真实端点凭据为本地开发前提；接入 `$events` ready、问答/审批、prompt 幂等与真实任务状态后再开放发送。2026-10-22 用户授权先冻结 0.9.28 并进行 GitHub 双端候选构建，构建结果单独记录，不由本报告推断；A4/A5 历史门禁不因本轮测试自动通过。

Online Agent 仅只读查阅本地 current-stage 指向的运行器源码，未验证其登录后线上行为，也未修改其服务、数据或 X1T 现场配置。
