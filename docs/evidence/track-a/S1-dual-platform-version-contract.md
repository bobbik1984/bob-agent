# 双端统一版本与候选构建门禁：本地验证（2026-10-02）

## 已实现

- 根 `package.json` 的 `version` 是唯一手工维护的产品版本。`scripts/version-contract.mjs sync` 只同步主应用、安装器及两个 package-lock/Cargo.lock 的根包版本元数据；`check` 只读校验；`candidate` 额外要求 Android versionCode 高于已保留恢复包 9023。
- 当前主应用与 PC 安装器的 package/Tauri/Cargo 版本已对齐为 0.9.24。原安装器 0.9.7 与两个 package-lock 根元数据 0.9.3 的偏差已消除。这个元数据修复**不是**新的 PC 安装包，也不表示已完成桌面端启动验收。
- Windows 与 Android 工作流都接受 `mobile-pc-rebuild` 施工分支，在同一提交上先执行版本门禁，成功后上传带版本与完整 SHA 的候选 Artifact。两个工作流均不再包含直接写 GitHub Release 的步骤，也不在候选阶段生成官网固定名 APK；公开下载入口保持未触碰。
- 0.9.24 的 `check` 与 `candidate` 均通过：Android versionCode 为 9024，高于恢复包下限 9023。尚未将本次提交的双端云端构建视为成功，也未完成双端安装验收。

## 可复现验证

- `node --test scripts/version-contract.test.mjs`：5 项通过，覆盖一致性、安装器漂移、非法版本、Android 候选下限及两个工作流不得自动发布。
- `node scripts/version-contract.mjs check`：`version=0.9.24 androidVersionCode=9024 verified=0`。
- `node scripts/version-contract.mjs candidate`：`version=0.9.24 androidVersionCode=9024 verified=0`。
- Python YAML BaseLoader 解析 `android.yml` 与 `windows.yml`：均成功。
- 前端 Vitest 直接调用现有程序：13 文件、68 项通过。Rust 库测试：331 项通过，1 项忽略。未安装或改动依赖。
- `git diff --check`、两个新增 `.mjs` 的 `node --check`：通过。

## 未完成与边界

尚未执行同一提交的 Windows/Android 云端双构建、桌面端安装与启动、手机→PC 配对/只读指令验收，也未实现正式晋级时核对双端产物并共同发布的独立流程。任何一端的本地绿色测试均不能替代这些证据；当前公开 Release 与官网固定入口不变。
