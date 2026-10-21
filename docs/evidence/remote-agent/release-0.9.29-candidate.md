# 0.9.29 同源双端候选

## 冻结范围

- 在 0.9.28 浏览基线之上加入 RH-02 原生 DSH 文字执行子集，详见 [实现证据](RH-02-native-execution.md)。
- 主应用、Windows 安装器、Android 均为 0.9.29；Android versionCode 9029，候选下限 9028。
- 不改动 X1T / Online Agent 线上服务，不更新官网稳定入口，不安装用户设备。

## 本地预检

- `cargo test --offline --lib -- --test-threads=1`：356 passed / 0 failed / 1 ignored；131 项现有编译告警，不宣称零告警。
- `pnpm --config.verify-deps-before-run=false test`：15 files / 84 passed。
- `node --test scripts/version-contract.test.mjs`：5 passed。
- `python scripts/test_promote_candidate_release.py`：2 passed（离线断点续传、摘要拒绝和冻结版本读取）。
- `node scripts/export-remote-agent-preview.mjs`：生成自包含离线预览，非本地客户端发布打包。
- `BOB_UI_OFFLINE=1` 运行 `scripts/test-remote-agent-ui.mjs`：390×844 中文与 1280×900 英文实际 Vue 交互通过；零页面异常、零 HTTP 请求。
- 原 file:// 测试夹具改为说明页；完整预览全部为模拟数据，无真实任务与认证。

## GitHub 候选状态

- 来源：main 提交 `788ab610122cd46a6b35fb08962c54860ab500e7`，已推送。
- [Windows CI](https://github.com/bobbik1984/bob-agent/actions/runs/39640877045)：completed / success，应用、独立安装器及候选 Artifact 已生成。
- [Android CI](https://github.com/bobbik1984/bob-agent/actions/runs/39640877016)：completed / success，版本门禁、APK 编译、签名校验及上传全部成功。
- 双端 Artifact 均包含 0.9.29 与上述完整 SHA，workflow_run.head_sha 一致；没有因文档补记而替换本次构建来源。
- 本批按授权生成 GitHub Actions 候选安装包，不另行晋级正式 Release 或覆盖官网稳定下载入口。0.9.28 候选 Release 独立保留。

### 已核验 Windows 归档

- Artifact ID `12364754183`，名称 `bob-desktop-windows-v0.9.29-788ab610122cd46a6b35fb08962c54860ab500e7`；71280264 字节。
- 完整下载与 GitHub Artifact SHA-256 一致：`46582aaf9897864a89bb2fecf0c397990aa85a2bcba7d3e72f60bc860645da26`。
- 本机归档：`dist-release/candidate-v0.9.29-788ab610122c/`。
- `bob-v0.9.29-installer.exe`：44349440 字节，SHA-256 `742a8c9cbc478bb26a424c0bd7bfb8aad12d5008c904d6d5c6250a7f49eb52ef`。
- `bob-v0.9.29-portable.zip`：34112745 字节，SHA-256 `c0ad065fe229ac74d275a6c48695f49cb5b30fc046af17ec3916ed40e77d5627`。

## 未验收边界

### 已核验 Android 归档

- Artifact ID `12366192644`，名称 `bob-mobile-apk-v0.9.29-788ab610122cd46a6b35fb08962c54860ab500e7`；34736999 字节。
- 完整下载与 GitHub Artifact SHA-256 一致：`b3350d79b74afc25e74957eec8db03537fb9b03180323c39c4fd092c278c1dfc`。
- `bob-v0.9.29-signed.apk`：94290274 字节，SHA-256 `849887762ce95520d168545aabb943ba724cc0bcadabbb04de01b6611e8d15c5`。
- 与 Windows 存于同一候选归档目录，并附 `SHA256SUMS.txt`。签名及对齐检查在 GitHub Android Runner 执行；本机未安装或启动此 APK。

### 真机与真实端点

CI 成功不等于 PC/Android 安装启动通过，也不等于 X1T 认证、实际模型调用、审批或取消真机通过。文件产物、新建会话、自动重连和跨重启恢复不在本批范围；旧 A4/A5 门禁不变。
