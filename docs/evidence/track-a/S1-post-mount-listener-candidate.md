# S1 候选：原生工具确认监听后移（2026-10-01）

## 改动范围

在 v0.9.15 施工线中，将 `tool:confirm_required` 监听从 `tauri-bridge.js` 模块顶层移到 `app.mount('#app')` 返回后的显式初始化调用。监听处理器及允许/拒绝 IPC 语义不变；注册失败会向调用者抛错并在入口记录错误，不再产生未处理的顶层注册 Promise。未改配对、同步、数据库、密钥、模型或文件审批代码。

这只减少一个挂载前副作用，**不证明**它是 v0.9.21 卡 Logo 的唯一原因。Android 真实首屏结果见 [同包升级实测](S1-inplace-upgrade-preflight.md)。

## 本地验证

- `node --check src/tauri-bridge.js`、`node --check src/main.js`、`node --check src/startup/deferred-tool-confirm.js`：退出码 0。
- 定向 Vitest：`src/startup/deferred-tool-confirm.test.js` 2 项通过，覆盖调用前不注册、拒绝决策传回 Rust、注册失败冒泡。
- 全部前端 Vitest：11 个文件、64 项通过、0 失败。新工作树未安装依赖；复用与 v0.9.15 相同锁文件对应的原仓库 `node_modules` 目录连接，用已存在的 Vitest 运行，未修改依赖锁文件。
- `git diff --check`：退出码 0。

## 真机补充与未验证

0.9.22 签名候选已由 CI 构建，在 Pixel 8 保留数据覆盖安装后，首次启动和两次强制停止后的冷启动均显示实际对话界面，旧会话可见。未配对、无网、PC 离线等状态矩阵仍未执行，亦未检查真实高风险工具事件在 Android 与 Windows 上的到达时序。S1 只完成真机启动的第一项证据，不是全门禁通过；A4 状态不变。
