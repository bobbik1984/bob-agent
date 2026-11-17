# todo — 需求池（仅保留未完成与待补尾缺口）

> **记录准则**（遵循工作区《项目开发与文档统一标准》§3.7）：
> 1. **已完成项** 一律归档至 [docs/archive_done.md](docs/archive_done.md)，不在本文件堆积战报；
> 2. **已获得的能力与验收标准** 统一登记于 [docs/FEATURES.md](docs/FEATURES.md)；
> 3. **当前排期施工步骤** 唯一维护于 [docs/CURRENT_STAGE.md](docs/CURRENT_STAGE.md)；本文件仅保留**尚未解决的需求、缺陷与部分完成项（`[/]`）的剩余缺口**。

---

## 一、`WP-E1` 线上实景闭环与桌面 / 手机用户终验

- [/] **`WP-E1.1` 常驻节点无头浏览器真实办公站点接管验收**：在常驻节点（含 `fonts-noto-cjk` 与 `0700` `--user-data-dir`）选取 3～5 个真实办公站点（测试账号），跑通自动浏览 → 手机 390px `<VirtualBrowserModal />` 接管 → 重启 Cookie 保活 → Routine 复跑并落盘证据。
- [/] **`WP-E1.4` 对话现场生成小组件真实商业 LLM 调优**：用真实商业模型完成 ≥2 个白领实用小组件“生成 → 无闪烁首帧 → 3 分钟内微调 → 保留及超时/重启回退”全流程实测录屏/日志。
- [ ] **主助理连续性与长期记忆用户终验（`WP-E1` / `WP-E1-M`）**：桌面/手机浏览器视觉与真机终验；后续演进补充独立离线副本记忆元数据同步、来源/作用域专门编辑 UI、FTS5 逐行增量索引、跨项目语义检索与长期 Worker 逐子问题结果自动对账（见 [CURRENT_STAGE §1–§2.1](docs/CURRENT_STAGE.md)）。
- [/] **Android 新版覆盖安装与真机终验（等待用户连机通知）**：合并覆盖 `d740979` / `07a7ce9` / `c7555da` / `e6da7e7` 及后续正式项目/记忆改动发包后的 Pixel 真机复核——状态栏安全区、手势返回栈、水滴与手机键盘高度缩放、三天日历触控拖动、已完成待办按年折叠、零对话项目/测试归档跨端同步、真实 BYOK 断网/重开（`RV-11`）、真实多节点流式联调，以及连接卡片/按消息定位的文件预览（详见 [CURRENT_STAGE §2.1](docs/CURRENT_STAGE.md)）。

---

## 二、`BP4` 设备入口、多 Harness 监管与策略扩展矩阵

- [/] **`BP4-06` 外部独立 CLI 实时旁观与多工具全事件故障矩阵**：补齐独立 `Claude` / `AGY` / `Codex TUI` 外部进程实时订阅（须确认官方持久接口，禁提取凭据/刮屏）、`DSH` 待问答交互全链路、`Codex` 缺轮次 `unknown` 权威核对与整机掉电故障恢复矩阵（详见 [CURRENT_STAGE §2.2](docs/CURRENT_STAGE.md)）。
- [/] **`BP4-07-TP4` 工具启停策略扩展验证与单节点工具限制**：真实审批等待中途停用工具、长任务取消确认、离线节点断网恢复同步与移动端设置开关；跟踪处理 `VPS3` AGY `1.2.16` 兼容/`DSH` 服务接线/`VPS2` Pi 模型配置与远端无效目录错误穿透。
- [ ] **`X1T` Claude 官方 `statusLine` `rate_limits` 真实持续显示**：等待并验收真实 `rate_limits` 回调写入持续额度菜单。
- [/] **`BP4-01/02/03` 跨设备工作区交接与成果渲染补全**：真实跨节点文件/dirty 素材交接与跨实例关联同步；`Codex`/`Claude`/`AGY` 原会话文件链接映射与隔离网页/复杂 PDF/Office（Office 依赖暂缓）实际渲染及不可变 `ArtifactVersion` 历史快照。
- [/] **`BP4-04/05` 主对话任务草案卡前端挂载**：在主对话 UI 补齐 Observe/Ticket 草案确认卡与低置信度（`< 0.90`）复核提示交互。
- [ ] **主助理自然语言代建会话与设备元数据增强**：主助理自由文本直建指定设备工具会话（与侧栏表单复用同一校验）；设备标识补齐可靠友好名、硬件类型与最后观测时间，区分连接 Home 与执行节点。

---

## 三、个人事务、收集箱（`BOB-D-014`）与多参谋长外部消费端

- [/] **单一收集箱与每日幕僚长分拣 Step 4 闭环（`BOB-D-014`）**：落地每日定时分拣触发、确认后自动写入常驻/PC 项目 `todo.md`（离线 PC 显示 `Waiting for PC`）、知识笔记「展开这个想法」一键进对话，以及旧 Dashboard `#inbox` 重定向；待确认 4 项默认值（`21:00` 合并未处理 / V1 100% 人工确认 / 先文字图片后语音 / 旧入口保留 30 天）。
- [/] **外部 Bot（`grok_bot` / `meta_muse`）消费端全闭环验收（`BOB-D-017`）**：消费端真实接入验证语义待办修改及 `bob_briefing`/`bob_order_submit`/`bob_orders` 派单回执、配对手机端读回，以及后续按需评估账号管理 UI 与待办完成/删除接口（严禁开放节点执行）。
- [ ] **跨端体验补齐项（`RV-13` / `RV-14` / `RV-15` / `RV-19` / `RV-21` / `XP-02`）**：闪念项目选择搜索框、`capture_media` 原图跨 Home 同步、Windows Toast / Android 到期提醒通知、`conversations`/`messages` FTS5 全文检索与对话内拍照传附件、`App.vue` 宽屏持续模块化，以及用纯 Rust `rustls` 客户端替换桌面端 `curl` 子进程。

---

## 四、`BOB-D-016` 后续进化波次（`WP-E2` → `WP-E4`）

- [ ] **`[WP-E2]` 白领办公文档深度处理与高频 IM 触达**：`BobNativeHarness` 内置纯 Rust 解析与生成 **`.pdf` / `.xlsx` / `.docx` / `.csv`** 并直通 `V1→V2` 批注审阅；落地标准化 `ChannelAdapter`（飞书 / Telegram / 官方微信桥增强）。
- [ ] **`[WP-E3]` 三路径零门槛安装体系与桌面扫码直连**：CI 发布含 Chromium 与中文字体的多架构镜像与静态二进制，升级 90 秒单行交互引导脚本与 1Panel / CasaOS / NAS 模板；完善桌面端托盘首次启动 3 步向导与对手机扫码直连。
- [ ] **`[WP-E4]` MCP 连接器客户端与 `QuickJS` 插件扩展体系**：`bob-runtime` 实现纯 Rust MCP Streamable HTTP + OAuth 2.1 客户端（写操作默认 `SideEffect`）；将 `FeatureStore` 升级至内嵌 `rquickjs`（`16MB` / `500ms` 沙箱）并以「能力目录」统一呈现，保持零 Node.js 必经依赖。

---

## 五、开源通用化（`BP6`）与前身项目待办备忘

- [ ] **`BP6` 开源发行与去个人化收尾**：选定 `LICENSE`、依赖许可证审计与 Git 历史 secrets 扫描；彻底核查去个人化配置注入（`BOB-D-007`）、跨设备凭据迁移（若在 `BP6` 立项）威胁模型，以及 `0.9.x`（`bob.db`）旧库迁移、无服务器 Android 后台提醒边界与费用可见性面板；明确是否需要 Google Drive 只读文档镜像。
- [ ] **前身项目文档清理备忘（回原仓库执行，不在本仓库直接修改）**：
  - 旧 Online 统一提案 §2 中“worktree 注册项可 prune”改为“禁止 prune（含 2707 项已备份未提交改动）”；
  - 旧 Online `ONL-D-005` 仍写 VPS3 为 Core，需标明已过期；
  - 旧 App `docs/contracts/bob_cloud_agent_interchange.yaml` 标为 `superseded`。
