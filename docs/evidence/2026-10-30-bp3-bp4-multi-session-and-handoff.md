# BP3 / BP4 多会话目录、设置子页、Online 兼容与跨节点交接证据（2026-10-30）

## 1. 交付范围与代码基线

- 实现提交：`782ab0a` / `1c36b57`（分支 `1.0.1` 与 `main` 同步推送）
- 覆盖工作包：`BP3-01.a`、`BP3-01.b`、`BP3-01.c`、`BP3-01.d`（云端签名构建与下载校验）、`BP3-02.a`（内核契约与回归）、`BP4-01`、`BP4-02`、`BP4-03`

## 2. 自动化测试与构建验证矩阵

| 检查项 | 命令 | 结果 | 说明 |
|---|---|---|---|
| `bob-kernel` 全量单测与集成测试 | `cargo test` (`crates/bob-kernel`) | **77 passed, 0 failed** | 含基础回归 72 项 + `multi_session_and_handoff` 5 项 |
| `bob-transport` 双向 TLS 1.3 测试 | `cargo test` (`crates/bob-transport`) | **24 passed, 0 failed** | 7 项 bootstrap 单测 + 17 项双向 TLS 集成测试 |
| `bob-app` 原生 Tauri IPC 测试 | `cargo test --lib --features native-tests` (`apps/bob-app/src-tauri`) | **29 passed, 0 failed** | 含 `catalog_and_multi_session_ipc_commands_persist_and_switch_cleanly` |
| 前端控制器与 IPC 映射单测 | `node --test tests/*.test.mjs` (`apps/bob-app`) | **32 passed, 0 failed** | 覆盖 `catalog`/`saveViewState`/`savePreferences` 与 `loadDraft` CAS |
| Android 扫码插件暂存检查 | `python -m unittest discover -s tests -p test_android_scanner_stage.py` | **4 passed, 0 failed** | 验证 Android Scanner staging 注入 |
| 前端生产构建 | `pnpm run build` (`apps/bob-app`) | **95 modules, built in 1.52s** | 零硬编码色彩、中英双语键对齐 |
| Playwright 5 视口浏览器交互 | `python tests/ui-browser.py` (`apps/bob-app`) | **5/5 视口通过** | `390x844`、`1180x757`、`390x420` × `en-US`/`zh-CN` × `light`/`dark` |

## 3. GitHub Actions Android 固定签名 APK 溯源

- Workflow Run：[`40937679735`](https://github.com/bobbik1984/bob-agent/actions/runs/40937679735)（`completed / success`）
- 源码 Commit：`1c36b578ec7810814e84d14710983d2ff0674b98`
- Artifact ID：`12926235871`（`bob-android-dev-1c36b578ec7810814e84d14710983d2ff0674b98`）
- Archive ZIP SHA256：`0927a7ff7e30e361a17b4ea850b874e47110dd303583cce9b2134b3f64098b8a`
- Signed APK SHA256（`app-universal-debug.apk`）：`d57237f3b97b7eb6306e4482f2fad727b04e169e11832e02fe7579a28555f273`
- 16 KB ELF 对齐检查：`16 KB ELF PASS: lib/arm64-v8a/libbob_app_lib.so`
- 真机状态说明：核验时本机 `adb.exe devices -l` 未检测到 USB 连接中的 Pixel 8，未伪造真机安装记录；APK 已下载保存在 `D:\Code\Archive_Cache\bob-bp3-phone-candidate\app-universal-debug.apk`。

## 4. 核心源文件 SHA256 校验值

### Pixel增量复核（2026-10-30）

本地HEAD为7d2180a，APK实测SHA256与上述d57237f3一致；ADB检测到Pixel，install -r成功，未卸载/清数据；冷启动424ms。真实Android WebView读取仍有原连接测试用户消息及Bob回复。本次经可见界面新建Pixel verification A/B两条独立测试对话，仅输入未发送草稿；A/B切换后各自草稿正确，A经WebView刷新后仍恢复。首轮脚本在异步切换尚未完成时立即断言失败，稍后读回及等待加载完成后的复核均正确；不能据此认定草稿丢失，也尚未证明快速切换竞态均无缺陷。

设备状态图标实际直接进入Device & Connection子页，可见返回Settings入口和原配对保留。本次没有发送模型请求，没有验证新会话Home真实续聊、Online公网接口或跨节点交接；未做完整Android进程重启恢复，不能写作全部真机通过。测试A/B及草稿保留便于复核；一次性USB调试转发已移除。原始结果位于仓库外bob-multi-phone-review，含异步检查失败现场与后续设置页读回。源码hash清单如下。

- `crates/bob-kernel/migrations/0005.sql`: `0512bf02ac92524ccc6be2cf03e579d18f787705d9383f6d70ad54ba101f56ce`
- `crates/bob-kernel/src/catalog.rs`: `a24ca54d4a4821bbe48024ddc294318079a7fa7b55ed02b1afd2402d02b56893`
- `crates/bob-kernel/src/online_provider.rs`: `79e4b0fa7866bd44931925c26e2796072433226065c5b8149a0e2267ba482fec`
- `crates/bob-kernel/src/workspace_handoff.rs`: `f5d6f481970c2c30a5f97eb3cb7148e8595ab370abcd3bf6f36a85be2e79978d`
- `crates/bob-kernel/src/local_execution.rs`: `4fdb2f667229acf39cc54ad3213e27a98427d9341cf73aaa022b71e18604347c`
- `crates/bob-kernel/tests/multi_session_and_handoff.rs`: `3ba48817b85c31dca81ac88100d3bbef1fcba2e26cc08da41b5365be7bdb49ad`
- `apps/bob-app/src-tauri/src/lib.rs`: `bce88930680595725df94868c467190f58e9d861044fc66587d381737aa09107`
- `apps/bob-app/src-tauri/src/home_host.rs`: `f9dddb30dd00592104a519e72322001089fdec248a705c520506162e7346f99d`
- `apps/bob-app/src-tauri/src/tests.rs`: `0187d86e3614ef9b104fb68d4e4970144cadafc90002ea5c20649f492ca7d6d9`
- `apps/bob-app/src/App.vue`: `f3665f5de4f69551a1febf62a990d3735cf213d4158cf95141d80af398aa852c`
- `apps/bob-app/src/kernel-bridge.mjs`: `d10b3a5dce8eddba8d21217c05865f7967600de106eb49d532be9491f7b3cdd4`
- `apps/bob-app/src/send-controller.mjs`: `993675e0323dfb12cc45596108a489a61e2eb902e67fecf1b6d347921f1b6194`
- `apps/bob-app/tests/ui-browser.py`: `30461d98fe9a300cd57803cf05a46b9e5c57844f41f6c7002fb122a594185ccb`
