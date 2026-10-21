# RH-02：DSH 原生文字执行子集

状态：本地实现与契约测试通过，纳入 0.9.29 候选范围；构建状态以 [0.9.29 交付证据](release-0.9.29-candidate.md) 为准。未连接 X1T、未进行 PC/Android 安装验收。

## 基线与边界

- 用户授权先冻结 0.9.28，再实现发送、实时回复、审批、取消；无需真实端点作为开发前置。
- 冻结提交：`1f23bb58f57bfb4a65082c51ad76db8f9e3541bb`。本报告描述其后的未发布工作树，不描述 0.9.28 安装包。
- 上游协议：`deepseek-ai/deepseek-harness@687f282e8625c6b8d11ff58211a90524be2e4317` (`dsh-v0.2.1-rc.2`)。核对 session-controller、gateway stream-protocol、user-approval/user-questions、agent 身份、AssistantStreamAccumulator。
- Bob 保持独立客户端，不经过 cloud-agent，不使用 Bob-to-Bob 配对记录，不上传本地模型密钥。

## 实际调用链

`RemoteAgentPanel → remote-agent-state → tauri-bridge → remote_agent watch/live/action → DSH HTTP / Mux`

| 行为 | 生产路径与事实判据 |
|---|---|
| 交互准入 | `/connect interactions=answer`；同 Cookie 的 Mux；`$events` ready 的 clientId 绑定当前连接 |
| 发送 | `session/prompt`；稳定 requestId、queue、文本；accepted 不标完成；同 ID 同内容不重复发出，冲突 ID 拒绝 |
| 回复 | `session/follow assistantStream=true`；snapshot 后持续消费，seq 去重，attempt/index/revision 校验；持久消息到达后移除临时增量 |
| 审批 | `$events` waterfall → 归一化 approval 卡 → `$events/result` 的 clientId/eventId/outcome；只允许本次/拒绝；不自动许可 |
| 普通问题 | 归一化 questions 卡；严格匹配问题 ID 与选项，支持自由文本；计时/continued 问题明确不支持 |
| 取消 | `session/cancel accepted` → cancel_requested；匹配当前 turn 的 aborted 才为 stopped；15秒后未确认保持可见且继续监听 |
| 断线/失败 | 固定脱敏错误码；未知结果禁止自动重发；旧 watch/过期交互不能操作；手动重连获取新基线 |
| 界面 | 现有 Work 懒加载面板、侧栏、主题和双语；新增输入/审批，推理默认折叠；远端文本不交给 HTML/本地文件打开器 |

DSH 审批是运行端自身的一次性权限决策，不是 Bob 文件写前 Diff 暂存保障。取消保留 DSH inbox，不保证终止已启动外部进程。关闭面板仅断开，不声称停止执行。

## 本地验证矩阵

- Rust `cargo test --offline --lib remote_agent`：13 passed / 0 failed；覆盖真实本机 Axum HTTP + WebSocket 与生产 Dsh trait 调用，另有投影状态单测。
- Rust 全量 `cargo test --offline --lib -- --test-threads=1`：356 passed / 0 failed / 1 ignored；既有编译告警保留，不将其描述为零告警。
- 前端 `pnpm --config.verify-deps-before-run=false test`：15 files / 84 passed；包含 12 项 Remote Agent 控制器测试。
- `node scripts/test-remote-agent-ui.mjs`：390×844 中文、1280×900 英文；实际 Vue 页面发送、流文本、折叠推理、审批、取消待确认、按钮状态及断开；无横向溢出，无远端 HTML 执行。
- UI 使用模拟 Bridge；Rust 网络使用本机协议夹具，均不等于已登录 X1T 或实际运行大模型。未新增依赖，未运行本地发布构建。

重点负向断言：错令牌/重定向/响应 ID、失效 watch、重复输入 ID、未观察到运行时取消、错误审批值、重复答复、跨会话审批、撤回后答复、未知提交不重发、流缺口、断线失效、取消超时不假停止、切会话不忘未决输入、快照恢复与 surface 替换。

## 尚未交付的能力

新建会话、计时问题 attachWait、continued 回答、文件上传/下载、Blender 图片、Work 持久关联、跨 App 重启请求账本、自动重连、完整历史分页、第二适配器和目标设备验收。capabilities.artifacts 保持 false；不把这一批标成 RH-02/RH-03 全量通过。

浏览器临时截图：`bob-remote-agent-ui/mobile.png`、`desktop.png`，仅含夹具数据。原测试页依赖 Vite，不适合 file:// 直接交付。现有 `scripts/export-remote-agent-preview.mjs` 将同一 Vue 组件及模拟 Bridge 打成单一 HTML，输出到 `dist-release/previews/remote-agent-preview.html`；已在 file:// 下以两种尺寸验证发送、审批、取消待确认、断开，页面异常为零、HTTP 请求为零。原测试页直接打开时只展示说明与完整预览入口，不再执行无法解析的裸导入。没有安装工具或访问真实执行端。
