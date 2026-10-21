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

待本次提交推送后记录固定 SHA、两条 Actions run 与 Artifact。此文档不提前声明云端成功。

## 未验收边界

CI 成功不等于 PC/Android 安装启动通过，也不等于 X1T 认证、实际模型调用、审批或取消真机通过。文件产物、新建会话、自动重连和跨重启恢复不在本批范围；旧 A4/A5 门禁不变。
