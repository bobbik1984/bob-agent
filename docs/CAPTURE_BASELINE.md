# Capture 基线与回放矩阵

> 状态：阶段 0 基线；自动测试和真实设备结果应持续补充。
> 目的：记录输入从哪里进入、实际写到哪里、是否同步以及失败是否可恢复。

## 已确认的当前事实

| 入口 | 修改前实际路径 | 已确认问题 | 本轮目标路径 |
|---|---|---|---|
| 快捷笔记 | `QuickNoteOverlay → notebook_append_daily` | 只有 Markdown 结果，没有统一接收状态和幂等记录 | `capture_quick_note → capture_journal → daily note` |
| 聊天 `/memo` | `useChat → notebook_append_daily` | 与快捷笔记相似，但入口身份丢失 | `capture_quick_note(chat_memo) → journal → daily note` |
| Android 分享 | `App.vue → mobile_outbox(create_note)` | PC Outbox 白名单不接受 `create_note`，成功提示不能证明落库 | 本机 Journal 与笔记提交成功后清缓存，再通过数据库/笔记同步传播 |
| 聊天收藏文章 | Link Harvester / `browse_page` / 文件写入 | 内容提取后写入 `wiki/raw/article` 或 `wiki/sources` | 文件写入成功后登记 committed Capture，并保留来源 URL 与文件引用 |
| 日程/待办 | `add_calendar_event → events` | 工具契约区分两者，简单解析和时间默认仍需回放 | 阶段 2 进入统一分类与纠正流程 |

## 自动回放矩阵

| ID | 场景 | 输入 | 期望 | 状态 |
|---|---|---|---|---|
| C-001 | 空 Capture | 只有空白文本 | 拒绝且不落库 | Rust 单测已覆盖 |
| C-002 | 换行规范化 | CRLF 与 LF 等价文本 | 相同内容哈希与幂等键 | Rust 单测已覆盖 |
| C-003 | 重复提交 | 相同内容提交两次 | 返回首条记录，不新增第二条 | Rust 单测已覆盖 |
| C-004 | 快捷笔记 | 一段普通灵感 | Journal 为 committed，daily note 有一条记录 | 待集成验证 |
| C-005 | Android 文本分享 | 系统 Share 文本 | 成功提交后才清缓存，连接后可同步 | 待真实设备验证 |
| C-006 | Android 图片分享 | 系统分享图片 | 原子归档成功后才删缓存；失败保留原缓存 | 命名/日志单测已覆盖，待真实设备验证 |
| C-007 | 跨端重复 | 手机与 PC 输入相同内容 | 以 idempotency_key 归并 | Rust 单测已覆盖 |
| C-008 | 跨端状态更新 | 一端 received、另一端 committed | 较新状态归并且不重复 | Rust 单测已覆盖 |
| C-009 | 启动恢复 | 速记在 received/committing 时退出 | 重启后幂等续写；最多 5 次并退避 | Rust 状态与诊断单测已覆盖；真实退出待设备回放 |
| C-010 | 终态乱序 | committed 后收到较新的 failed | 不允许终态倒退或丢失派生引用 | Rust 单测已覆盖 |
| C-011 | 稳定网页三入口 | 同一 canonical URL 经文章收藏、快捷笔记、Android 文本分享 | 来源 URL/状态等价；Knowledge 与 Seed 差异可解释；跨端可归并 | 本地夹具与 Rust 纵切片测试已覆盖，待真机回放 |

## 真实设备矩阵

| 网络/状态 | PC | Android | 预期证据 |
|---|---|---|---|
| 同一 Wi-Fi | 在线 | 在线 | LAN 路径、双方同一 sync_id、结果落库 |
| 不同网络 | 在线 | 在线 | Relay 四段回执、双方同一 trace_id |
| PC 离线 | 离线 | 分享内容 | 手机本地 committed，原始输入保留，稍后补同步 |
| Relay 不可达 | 在线 | 在线 | LAN 可用则降级；否则准确显示 Relay 阶段失败 |
| 回程丢失 | 在线 | 在线 | 不虚报完成，重复请求保持幂等 |
| 应用提交中退出 | 重启 | 任一端 | Journal 保留非终态项，可恢复或明确失败 |

## 发布体积基线

| 产物 | 基线字节数 | 采集方式 | 状态 |
|---|---:|---|---|
| PC 主程序 | 尚未采集 | Release 构建后读取文件字节数 | 待执行 |
| PC 绿色包 | 尚未采集 | `scripts/release.bat` 产物 | 待执行 |
| PC 安装程序 | 尚未采集 | `scripts/release.bat` 产物 | 待执行 |
| Android APK/AAB | 尚未采集 | GitHub Actions 或本机构建产物 | 待执行 |

本轮 Capture 实现只复用 Rust、SQLite、Vue 和现有同步系统，不新增运行时或第三方依赖。

## 2026-08-09 自动验证结果

- `cargo check`：通过。
- `cargo test --lib`：57 项通过，0 项失败；其中 Capture 契约、恢复、图片、活动和三入口纵切片相关 13 项。
- `npm test`：6 项通过，0 项失败。Relay 使用独立 package 与测试命令，不再被根项目误收集。
- `npm run build`：通过；仅保留既有的大 chunk 提示。
- `git diff --check`：通过。

## 原始资料与图片边界

- PC 文件和文档只保存原始绝对路径引用，不复制用户资料。
- Android `content://` 授权通常是临时的，现有 ShareActivity 也只把图片复制到缓存。因此图片成功接收后归档到 `notes/assets/captures/images/YYYY/MM/`。
- 文件名采用 `<capture_id>--<hash8>--<原始文件名>`，Journal 同时保存内容哈希、受管相对路径和来源设备；同名分享不会覆盖。
- 图片复制使用 `.part` 临时文件、字节数校验和原子重命名；完成前不清理分享缓存。
- 当前图片标记为 `local_only`，不会通过只含 SQLite 数据的同步流向其他设备虚报“图片已同步”。图片二进制跨端传输属于阶段 3。
- `capture_events` 保存语义事件代码与参数，每个来源设备只保留最近 50 条；展示时按当前 UI 语言翻译，不固化中英文文本。

## 恢复边界

- 快捷笔记、聊天 `/memo` 与 Android 文本分享属于可安全重放入口；每日笔记写入隐藏 `capture_id` 标识，进程在文件写入后退出也不会重复追加。
- 每次失败记录 `retry_count`、`next_retry_at`、错误阶段与消息；退避为 5 秒、30 秒、2 分钟、10 分钟，之后上限 30 分钟，自动恢复最多 5 次。
- 未知或未来入口不自动决定知识、日历或待办落点，只保留为 pending 并通过诊断接口报告。
- `capture_diagnostics` 返回待处理总数、失败总数和最近 20 条失败；`capture_retry` 提供显式重试。

## 2026-08-10 离线优先智能分流

- `capture_ingest` 始终先写入本地 Journal，再执行本地分流；网络和模型不是捕获成功的前置条件。
- “明天”“后天”“N 天后”“下周一”“8 月 20 日”和常见中文时刻由组合式本地解析器处理，不调用 Clerk。
- 明确 Todo/Event 经日期、时间和 schema 校验后，以 Capture ID 生成确定性事件 ID；重复提交不会创建第二条记录。
- 复杂或模糊输入进入 `pending_enrichment`。后台在启动 8 秒后开始检查，此后每 60 秒处理到期队列。
- Clerk 只返回结构化候选；候选先写入 `capture_enrichment`。工具提交失败时保留 `validated` 结果，后续只重试提交阶段。
- 网络或 Clerk 暂不可用采用有上限的退避并持续等待恢复，不耗尽后丢弃；持续非法模型输出最多 5 次后进入 `permanently_failed`，原始内容仍保留。
- Todo/Event 已形成确定性数据库提交闭环；QuickNote/Note/Source 已接入权威 Markdown 提交器。Note 以内容哈希稳定去重，Source 以规范化 URL/原文件稳定去重并保存原始引用。
- 项目笔记只接受稳定 `project_` ID；用户只说项目名称时，本地索引必须唯一匹配现有 Project 对象。未匹配或重名进入 `needs_clarification`，不会让 Clerk 猜 ID，也不会作为网络错误反复重试。
- 仅有 URL 的 Source 先以 `pending_extraction` 可靠保存；正文提取、Knowledge Point 蒸馏和实体关系属于下一条管线，不影响收藏成功。
