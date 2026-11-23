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
- 成品归档路径：`dist-release/bob-v1.0.1-1c36b57-signed.apk`（相对项目根目录）。

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
- `apps/bob-app/src/kernel-bridge.mjs`: `128d9dbf60a42f25aa4a839ae36ae67c523b8cb1f16a5308dca20661c54d3163`
- `apps/bob-app/src/send-controller.mjs`: `993675e0323dfb12cc45596108a489a61e2eb902e67fecf1b6d347921f1b6194`
- `apps/bob-app/tests/ui-browser.py`: `4151b2e310f819330553f76bbd630ffe0529949b32df6c4929d01515e9472db6`

## 5. F1 直接模型调用（BYOK）、已配对手机离线直连汇合与 F3 个人助理内核增量证据（2026-10-30）

### 5.1 交付范围
- **旧能力逐项对照与 F3 个人助理内核**：`docs/COMPONENT_MAP.md` 第 6 节逐项对照表、`crates/bob-kernel/migrations/0006.sql`、`crates/bob-kernel/src/personal_ops.rs`（`captures`、`memories` 显式生命周期 `proposed -> active -> superseded -> forgotten`、`notes`、`schedule_items`、`decisions`）。
- **F1 无需配对直接模型调用（BYOK）与三轴解耦**：`docs/wiki/kernel-schema.md` 第 9 节、`crates/bob-kernel/src/direct_provider.rs`、系统安全存储凭据槽位（Windows DPAPI `device_vault.rs` / Android Keystore AES-256-GCM `android_vault.rs` + `DeviceVaultPlugin.kt`）、原生三协议适配器 `apps/bob-app/src-tauri/src/direct_llm.rs`（`openai_compatible`、`anthropic_messages`、`gemini_generate_content`）、前端设置子页「直接模型 API」与执行路径切换器（`App.vue` + `kernel-bridge.mjs`）。
- **已配对 Replica 在 Home 离线时直连调用与重连幂等汇入**：`replica_direct_sync_outbox`、`Store::complete_replica_direct_execution`、`Store::receive_replica_direct_turn`、双向 TLS 控制消息 `HomeRequest::ReplicaDirectTurn`（`crates/bob-transport/src/control.rs` + `home_host.rs` + `home_link.rs`）。

### 5.2 增量自动化验证结果

| 检查项 | 命令 | 结果 | 说明 |
|---|---|---|---|
| `bob-kernel` 全量单测与集成测试 | `cargo test` (`crates/bob-kernel`) | **81 passed, 0 failed** | 新增 `direct_provider_and_personal_ops` 4 大集成测试（含 Replica 离线直连→重连幂等汇入 Home 主库、已出网 `unknown_send` 禁止本地重复执行、F3 全生命周期） |
| `bob-transport` 双向 TLS 1.3 测试 | `cargo test` (`crates/bob-transport`) | **24 passed, 0 failed** | 含 `HomeRequest::ReplicaDirectTurn` SHA-256 校验与 `Store::receive_replica_direct_turn` 集成 |
| `bob-app` 原生 IPC / Vault / Loopback 测试 | `cargo test --lib --features native-tests` (`apps/bob-app/src-tauri`) | **31 passed, 0 failed** | 新增 Windows DPAPI 凭据槽位读写清除回环 + 本地 HTTP Loopback OpenAI/Anthropic/Gemini 三协议端到端执行验证 |
| 前端控制器与 IPC 映射单测 | `node --test tests/*.test.mjs` (`apps/bob-app`) | **33 passed, 0 failed** | 新增 `providerGet`/`providerSave`/`providerTest`/`routeGet`/`routeSet` 桥接验证 |
| Android 扫码插件暂存检查 | `python -m unittest discover -s tests -p test_android_scanner_stage.py` | **4 passed, 0 failed** | 验证 Android 插件暂存 |
| 前端生产构建 | `pnpm run build` (`apps/bob-app`) | **95 modules, built in 1.76s** | 零硬编码色彩、中英双语键完整对齐 |
| Playwright 5 视口浏览器交互 | `python tests/ui-browser.py` (`apps/bob-app`) | **5/5 视口通过** | 覆盖设置首页→「直接模型 API」子页→保存配置→返回设置首页→「设备与连接」子页→多会话切换 |

### 5.3 新增与更新源文件 SHA256 校验值
- `crates/bob-kernel/migrations/0006.sql`: `caf2369f3f35d5450de0b03f84261177f370b75f02948de970ec32b6d2af9947`
- `crates/bob-kernel/src/direct_provider.rs`: `f98ac49c425bf8274dccd4acaf3ac6bd84fbca0e12ad2e88d9d32421a8be28e9`
- `crates/bob-kernel/src/personal_ops.rs`: `a2cda28518c35c3b62048917d9f0d7e4845ca6fa5704366b97dead0a9eb0acc8`
- `crates/bob-kernel/tests/direct_provider_and_personal_ops.rs`: `7c49cd625494df0c4fd5537dbebcd3c2e102733d3f114db60949eb9f1b447012`
- `crates/bob-transport/src/control.rs`: `6988bc4fecfdf6ea4415df8cf9528384985284985240c014a6f9b1badfd0c56baca4bb`
- `apps/bob-app/src-tauri/src/direct_llm.rs`: `6938078e7b55f2b0879099c33f31e53a1287803bf42ef9de7cdf14e8812c13f3`
- `apps/bob-app/src-tauri/src/device_vault.rs`: `ae0d6e6789d5c11b198bcc20c7c6ed6c846bece036015cc75b2a98a798a20745`
- `apps/bob-app/src-tauri/src/android_vault.rs`: `36ec39b6c151372bd7e2bf02d441aff76d839d45f25872ecf6bfbda2a206dfd0`
- `apps/bob-app/src-tauri/android/DeviceVaultPlugin.kt`: `3fac521f0c480e251b1b5307f60d36e0f1fd88e7e3b33cd6ce99945e9d09517c`
- `apps/bob-app/src-tauri/src/lib.rs`: `c57c7f0bb50c816351ab5d769c792f336adb2a072e3fe85c6f3ae6c7b3da8840`
- `apps/bob-app/src/App.vue`: `2e57b0542ac0414f89ce786e2ff944ea07a43e88e811544ff387f1165e3ccb29`

### 5.4 GitHub Actions Android 固定签名 APK 溯源（F1 + F3 增量候选）
- Workflow Run：[`40998475431`](https://github.com/bobbik1984/bob-agent/actions/runs/40998475431)（`completed / success`）
- 源码 Commit：`15a8dc2fd15884c0d0348233c03e70aaff9cd27d`
- Artifact ID：`12952985322`（`bob-android-dev-15a8dc2fd15884c0d0348233c03e70aaff9cd27d`）
- Archive ZIP SHA256：`09b601a0724c561bcd7a75ada178780fe42c317c05aa742db68a7b13890d1941`
- Signed APK SHA256（`app-universal-debug.apk`）：`be0bb242765a3d88f76462fe11b6828a7fd845dad5565f9fded34cd2237575d7`
- 成品归档路径：`dist-release/bob-latest.apk` 与 `dist-release/bob-v1.0.1-15a8dc2-signed.apk`（相对项目根目录，附 `dist-release/SHA256SUMS.txt`）
