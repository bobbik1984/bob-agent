# S0 手机→PC 重建基线记录（2026-10-01）

## 已核实

- 当前施工目录是独立 Git 工作树 `bob-agent-mobile-pc-rebuild`，分支 `mobile-pc-rebuild` 从 `a896f53`（v0.9.15）建立。设计文档提交为 `7b2f1cc`，未拣选 v0.9.16～v0.9.21 的业务代码。
- 原诊断工作树仍在 `bob-agent`，诊断分支最近提交在建立工作树时为 `46e0885`；未重置、清理或覆盖它。
- 本地存在标称 v0.9.15 的签名 APK：`dist-release/bob-v0.9.15-signed.apk`；大小 92,832,098 字节，SHA-256 为 `87FE83D200C577462B708B07A473CD2560D17CFF5B2BC63A110CAE7947FE5A1F`。本机未找到 `apksigner`、Java 或 `keytool` 命令，签名证书指纹和 APK 与提交的精确构建来源尚未核验。
- 当前 USB 可见的手机是 `24069RA21C`，其已装 `bob.agent` 为 `0.9.5-h`。它不是先前出现 v0.9.21 白底 Logo 的目标设备；本次没有覆盖安装、启动测试或读取应用数据。

## v0.9.15 启动静态审计

- `src/main.js` 静态导入 `App.vue`、i18n 和 `tauri-bridge.js` 后挂载根组件；Bridge 是模块求值的前置依赖。
- `src/tauri-bridge.js` 顶层在 Tauri 环境下动态导入多项 Tauri API，并立即注册 `tool:confirm_required` 监听。该注册不应被直接认定为 v0.9.21 故障根因；这里只标记为启动依赖。
- `src/App.vue` 的 `onMounted` 中存在串行配置/会话 IPC，随后读取 `pairing_payload` 并异步触发同步。代码包含一秒后移除原生 Splash 的兜底。静态代码不能证明界面已可交互，也不能证明这些 IPC 在真机上有界返回。
- `src-tauri/src/lib.rs` 在 `run()` 内初始化数据库，并在 Tauri `setup()` 中启动 Relay 后台监听。Rust 进程启动成功不等于 WebView 首屏成功。

## 尚未验收与下一门槛

用户此前报告 v0.9.15 在其手机上正常启动，这是历史使用事实；同一目标手机、当前系统环境下的 v0.9.15 独立复测尚未执行。不能因此宣称 S0 真机复现或 S1 启动验收通过。

下一步先完成启动入口与协同依赖的最小任务卡及自动化测试接缝。需要目标手机参与时，先核对设备与安装包签名，选择不覆盖现有数据的验证方式；不得为了回退旧 APK 而卸载或清除当前应用。
