# todo — 需求池

> 尚未进入当前施工步序的需求、风险与待判断事项。当前已排期的详细执行步骤见 [docs/CURRENT_STAGE.md](docs/CURRENT_STAGE.md)。

## 待用户裁决

- [x] BOB-D-006设计已获用户确认；真实安全/跨网验证仍待做
- [ ] BOB-D-007 文档权威位置：是否需要在 Google Drive 放只读镜像供跨端 Bot 读取

## 核心待办队列（按里程碑）

### BP0 → BP1（架构与规格定案）
- [x] **WP-01**：静态深度盘点完成，见 [Evidence](docs/evidence/2026-10-28-wp01-deep-inventory.md)；旧测试/现场数据库/全历史secret扫描未运行
- [x] **WP-02**：Schema/预热Outbox设计草案完成，见 [kernel-schema](docs/wiki/kernel-schema.md)；25项内存DDL约束通过；后续已有内核运行实现，见BP2当前证据
- [x] **WP-03**：连接配置与威胁模型设计获批；不代表真实安全测试通过，BOB-D-007镜像选项仍待定

### BP2 → BP3（1.0.1 骨架与个人工作环境 MVP）
- [/] **BP2-01**：搭建 `1.0.1` Rust Workspace（`crates/bob-kernel`）+ Tauri v2（Desktop & Android）+ Vue 3 工程骨架
- [/] **BP2-02**：实现 `BOB-D-008` 懒式休眠、输入框 `@focus`/`@input` 静默预热、单飞 `inFlightWakeupPromise`、本地 `intent_outbox` 先落盘必达与 `not_dispatched` / `unknown_send` 分离对账
- [ ] **BP3-01**：打通 X1C Home 主库 ↔ 手机 Tauri Android Replica 的会话、捕获、待办与远程文件只读浏览（F4）
- [ ] **BP3-02**：通过进程边界接入 Harness Runner（DSH / AGY / Codex / Generic API），在统一 UI 中跑通 Task → Execution → Artifact 预览与 Review 闭环

当前证据：内核59项、实际TLS套接字11项、原生12项、Node17项及前端构建通过。Home授权/加密接收与回执查询、Windows DPAPI已实现；测试证书不代表首次配对。真实彩虹回复正常完成；第二轮真实调用超时且清理确认；两轮都没有停止请求，手动停止仍待验收。见[当前施工](docs/CURRENT_STAGE.md)和[shell证据](docs/evidence/2026-10-29-bp2-shell.md)。

- [/] BP2/BP3：本机只读执行与固定Stop入口；继续完成停止按钮真实操作、窄屏与可用性验收。
- [/] Home认证基础：持久设备/会话授权、TLS接收及回执查询完成本机验证；继续首次配对、App投递/回复同步和Android Keystore/真机，不以CLI登录或测试证书代替配对。

## 前身项目需要修正（只记录，不在本仓库动手；需要时到对应项目执行）

- [ ] 旧 Online 统一提案 §2 中“worktree 注册项可 prune”的表述应改为“禁止 prune，其中有 2707 项未提交改动（已备份）”
- [ ] 旧 Online ONL-D-005 仍写 VPS3 为 Core，实际生产在 Hostinger，决策已过期
- [ ] 旧 App `docs/contracts/bob_cloud_agent_interchange.yaml` 应标为 superseded

## 开源前提（BP6，日常开发中就要避免返工）

- [ ] 选定许可证并补齐 LICENSE；审计依赖许可证
- [ ] 去个人化：域名、IP、节点名全部改为外部 Provider 配置注入
- [ ] 包名与商标：旧 Online 的 `antigravity-nexus` 不能沿用
- [ ] LLM Key 统一改用系统安全存储（Windows DPAPI / Android Keystore）
- [ ] 复用并修订旧 App 的 `OPEN_SOURCE_WORKFLOW.md`
- [ ] Git 历史 secrets 扫描结论与微信网关合规评估

## 产品缺口（统一审查 G14）

- [ ] 没有服务器时手机如何收到推送和提醒（Android 后台限制）
- [ ] BYOK 费用可见性与预算上限
- [ ] 两套 UI 的 i18n 键对齐（严禁硬编码中文字符串与硬编码颜色）
- [ ] 旧 App 0.9.x 已安装用户的数据迁移（`bob.db` → `1.0.1` Home 库）
- [ ] 遥测和崩溃上报的隐私策略
- [ ] AGY / Codex 订阅在开源场景下的使用许可
