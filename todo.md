# Bob 当前开发清单

> 产品方向：`docs/PRODUCT_VISION.md`
>
> 阶段顺序与完成门槛：`docs/BOB_EVOLUTION_ROADMAP.md`
>
> 现状回放与已知缺口：`docs/CAPTURE_BASELINE.md`
> 2026-08-09 前的完整历史清单：`docs/archive_todo_pre_capture_20260809.md`

本文件只保存当前阶段及紧邻下一阶段的可执行任务，不再混入每日开发日志、已完成版本说明或长期设想。

## 当前主线：阶段 0–1，可靠 Capture 基座

### R0-01 现状回放集（P0，进行中）

- [x] 盘点聊天 `/memo`、快捷笔记、Android 系统分享的实际入口。
- [x] 记录 Android 分享原 `create_note` Outbox 操作无法通过 PC 白名单的断点。
- [ ] 为文章收藏、Source/Knowledge 生成和网页提取建立自动回放样例。
- [ ] 为 Todo/Event 自然语言分类建立中英文回放样例。
- [ ] 在真实 PC 与 Android 上执行 `docs/CAPTURE_BASELINE.md` 场景矩阵。

**验收**：每条核心路径均有输入、预期落点、实际落点、失败状态和复现步骤。

### R0-02 发布与同步基线（P0）

- [x] Relay V2 契约、客户端诊断和自动故障注入测试已有基础。
- [ ] 使用同一 `trace_id/sync_id` 完成 PC、Relay、Android 三方真实日志对账。
- [ ] 记录 PC 主程序、绿色包、安装器、Android APK/AAB 当前字节数。
- [ ] 验证 LAN、跨网 Relay、对端离线、回程丢失和应用重启。
- [ ] 确认生产 Relay 的版本、进程守护和重启策略。

**验收**：连接与同步失败能定位到具体阶段，客户端产物有可比较体积基线。

### R0-03 文档收口（P0，已完成）

- [x] 建立 `docs/PRODUCT_VISION.md`。
- [x] 建立 `docs/BOB_EVOLUTION_ROADMAP.md`。
- [x] 将旧 `todo.md` 完整归档，重建当前执行清单。
- [x] 将当前主线从 Goal Runtime 调整为 Capture 基座。
- [x] 完成第一轮代码验证后更新 `README.md`、`docs/ARCHITECTURE.md`、`docs/FEATURES.md` 与 `LLM_WIKI.md`。

**验收**：愿景、路线、真实架构、代码导航、任务和进度各有唯一职责。

### R1-01 CaptureEnvelope 契约（P0，已完成）

- [x] 定义版本化 Envelope：入口、来源设备、原始内容、来源 URL/文件、显式意图、哈希、幂等键、隐私/同步范围、状态与错误阶段。
- [x] 新增 SQLite `capture_journal` 与状态索引。
- [x] 增加 `capture_ingest`、`capture_quick_note`、`capture_list` Bridge API。
- [x] 增加等价内容规范化、幂等和空载荷 Rust 单测。
- [x] 通过 Rust 编译、单测和前端构建验证。

**验收**：重复输入不重复落库；失败保留阶段和原始输入；不增加用户侧运行时。

### R1-02 入口适配（P0，已完成）

- [x] 快捷笔记改由 Capture Journal 可靠提交。
- [x] 聊天 `/memo` 改由 Capture Journal 可靠提交。
- [x] Android 分享先本地提交成功再清理原始缓存，移除无效 `create_note` 伪 Outbox。
- [x] Capture Journal 纳入 SQLite 增量同步。
- [x] 将 Link Harvester 的文章写入路径和 `save_to_notes` 接入 Source/Knowledge Capture 流程。
- [x] PC 普通文件保持原路径引用；Android 图片从临时缓存原子归档到可追溯受管目录，不再生成占位文本。
- [x] 为各 Capture 入口加入语义化用户活动日志，每设备仅保留最近 50 条并按当前 UI 语言渲染。

**验收**：同一内容从不同入口进入时具有一致的来源、幂等与处理状态。

**边界**：Android 当前原生 ShareActivity 只接收文本与图片；普通文档分享及图片二进制跨端传输留到阶段 3，不以元数据同步冒充文件同步。

### R1-03 Capture Journal 恢复能力（P1，已完成）

- [x] 增加失败 Capture 的手动重试命令、最多 5 次自动恢复和有上限退避策略。
- [x] 应用启动时安全恢复速记类 `received/extracting/classifying/committing` 中断项；未知入口保留诊断，不猜测落点。
- [x] 增加待处理数量和最近 20 条失败查询，但不污染普通同步日志。
- [x] 为跨端乱序、重复、状态推进和终态不可倒退补充测试。

**验收**：模型、网络或应用中断不会丢失输入，也不会重复产生派生对象。

### R1-04 三入口纵切片（P1，进行中）

- [x] 建立本地稳定网页与期望契约夹具，不依赖公网内容漂移。
- [x] 自动回放聊天收藏、快捷笔记和 Android 文本分享三入口。
- [x] 对比 canonical URL、Knowledge/Seed 显式意图、committed 状态、活动日志和跨数据库归并结果。
- [x] 修复快捷笔记与 Android 文本分享只在正文保留 URL、Journal `source_url` 为空的差异。
- [ ] 在真实 PC 与 Android 上执行三入口回放，记录双方日志与同一同步结果。

**验收**：三入口得到等价的来源与状态；允许因显式意图产生不同派生对象，但差异必须可解释。

## 下一阶段：知识与行动分流

阶段 1 完成后才展开：

- [x] 确认统一信息与知识生命周期设计，明确 Markdown 真相源与 SQLite 可重建加速层。
- [x] 建立知识对象 schema、稳定 ID、关系词表、兼容解析和安全写入测试。
- [x] 建立只读知识审计器与固定迁移 fixtures；真实 AppData dry-run 不修改文件。
- [ ] 根据 dry-run 结果完善旧 `wiki/learned` 分类与同标题候选判断。
- [ ] 建立可从 Markdown 重建的 SQLite 对象和关系索引。
- [x] 实现 Todo/Event 离线优先分流状态机：先持久化 Capture，联网后由 Clerk 延迟补充语义。
- [x] 实现组合式本地时间快车道与统一置信度协议，避免穷举完整句子。
- [x] 将 Clerk 限定为结构化解析器；所有日历/待办写入必须经过本地校验、幂等提交和真实回执。
- [x] 增加断网退避、非法模型输出、日期校验和重复创建防护测试。
- [x] 将 Note/Source/QuickNote 接入权威 Markdown 提交管线：稳定 ID、来源去重、原始引用、项目唯一匹配和真实写入回执。
- [ ] 接入 Source 正文提取、Knowledge Point 蒸馏与来源到知识点的可追溯关系。
- 结构化区分 Source、Knowledge、Seed、Todo、Event、Routine、Goal；
- 一次 Capture 可事务性地产生多个派生对象；
- 修复 Todo 被午夜时间伪装成 Event 的语义问题；
- 低置信度分类使用低干扰确认；
- 用户纠正进入最高优先级 correction memory。

## 暂缓

在 Capture、分流和跨端一致完成前暂缓：多 Agent、完整 Goal DAG UI、iOS、独立 Web UI、手机大型本地模型和新增通讯渠道。

## 完成规则

任务只有在代码、测试、用户可理解错误、跨端影响、依赖/体积和权威文档同时通过时才能标记完成。规划中的能力不得写成现有功能。
