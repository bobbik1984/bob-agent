# Bob-Mobile 跨端联动蓝图 (Cross-Platform Blueprint)

> **文档版本**: v2.0 — 2026-07-03
> **前置依赖**: 本文档基于 bob-agent v0.4.4 (Tauri V2 + Vue 3 + Rust) 的现有架构，并且已在 Android (ARM64) 平台上验证底层引擎（Rust + SQLite）100% 跑通。
> **关联文档**: [ARCHITECTURE.md](./ARCHITECTURE.md) (PC 端架构) | [todo.md](../todo.md) (目标 22-23)
> **目标读者**: 所有参与 Bob 生态开发的多智能体与人类开发者

---

## 一、产品定位与核心哲学

### 1.1 两个完全不同的产品形态

Bob 的跨端战略**不是**把电脑版缩小到手机上，而是两个定位截然不同、但底层深度互通的产品：

| 维度 | Bob-Desktop (PC) | Bob-Mobile (手机) |
|------|-------------------|-------------------|
| **角色隐喻** | 🧠 大脑 (Brain) | 👁️ 触手 + 便携式离线节点 |
| **核心动作** | 深度思考、图谱编织、工作流闭环执行 | 极速捕获、碎片输入、触控交互、便携式离线推理 |
| **AI 策略** | 云端 API + Tool Calling + Goal Mode | **端侧 llama.cpp 推理 (Gemma 4B 等)** + 云端 API 降级 |
| **数据存储** | 完整 SQLite，100% 本地 | 轻量 SQLite，使用 Tauri `app_data_dir()` 沙盒隔离 |
| **交互范式** | 键鼠为主，宽屏多栏 | 触屏为主，底部导航条 (Bottom Navigation)，极简汉堡菜单，语音优先 |
| **常驻后台** | 是 (系统托盘) | 否 (用完即走，或者通过 Sidecar 驻留) |

> 手机端的核心愿景不仅是 PC 的延伸，更是一个通过 `ggml-org/llama.cpp` 驱动本地轻量级开源大模型的**离线便携式节点**。实现真正的断网可用、数据绝对隐私。

### 1.2 手机端核心场景

1. **📎 稍后阅读 / 文章抓取 (Share Extension)** — 分享给 Bob，手机调 API 直接生成结构化知识点存本地，回到 PC 后图谱互联
2. **🎙️ 闪念胶囊 / 灵感捕获 (Voice-first)** — 大麦克风按钮，一键录音转文字自动排版
3. **📋 意图待办 (Natural Language Todo)** — 手机可独立管理日程，同步 PC 后补充提醒/执行

### 1.3 跨端安全架构

扫码绑定互通流程 (Onboarding)：
- 移动端抛弃“选工作区”等重型流程，将微信接入步骤替换为“扫码绑定 PC”向导。
- **扫码库**：采用 Tauri 原生 `@tauri-apps/plugin-barcode-scanner` 插件，调用 iOS/Android 系统级极速扫码 API，不占用 WebView 性能。
- PC 生成专属二维码 (包含 Ed25519 公钥 + 局域网/信令连接信息)。
- 手机扫码，自动生成如 `Bob-Mobile-X1` 的唯一设备标签，同时生成自己的 Ed25519 密钥对。
- **PC 端二次确认**：手机扫码后，PC 端会弹出全局确认弹窗（“一台名为 XXX 的手机正在请求绑定”）。只有在 PC 上点击“允许”后，双方才交换公钥建立信任。
- PC 通过安全通道全量下发运行时配置 (API Keys / System Prompt / 模型偏好 / 知识库状态 / config_version) 实现首发同步。此后任意一端修改设置，全自动双向同步更新。

安全通道技术细节：
- **Ed25519**: 身份认证（证明你是谁），不是加密
- **X25519 ECDH**: 从 Ed25519 密钥对推导共享密钥
- **AES-GCM**: 用共享密钥做对称加密传输 (已有 aes-gcm = 0.10 依赖)

### 1.4 数据同步边界 (Master-Edge 模式)

PC 作为重型计算主节点 (Master)，手机作为轻量遥控边缘节点 (Edge Node)：

| 分类 | 内容 | 同步方向 |
|------|------|:--------:|
| ✅ 手机同步 (镜像) | 知识库、日程任务定义、笔记、模型配置、技能列表、Bob 用户记忆、Bob 人设 (`bob.md`) | PC → 手机 |
| 🚫 不同步 | 本地大模型文件 (数 GB)，手机使用独立的轻量级模型 | — |
| ⚡ 执行权隔离 | 手机同步显示 PC 定时任务但不执行；手机可向 PC 下发新任务 | 手机 → PC |

---

## 二、Tauri V2 统一架构

当前 Cargo.toml 使用 tauri 2.0.0-rc.17。必须升级到 2.0 正式版。

Tauri V2 原生支持 Android/iOS：
- 前端 Vue 3 通过系统 WebView 渲染
- Rust 编译为 ARM 动态库
- 不需要 Java/Kotlin/Swift 代码

---

## 三、目录结构规划

```
bob-agent/
├── src/                                # 前端 (Vue 3)
│   ├── shared/                         # 跨端共享层 (composables/utils/components)
│   ├── desktop/                        # PC 专属 (DesktopShell + views + components)
│   ├── mobile/                         # 手机专属 (MobileShell + views + components)
│   ├── tauri-bridge.js                 # IPC 适配层
│   └── router.js                       # 平台检测 + 动态路由
├── src-tauri/src/
│   ├── core/                           # 跨端共享核心 (llm/db/config/calendar/sync_protocol)
│   ├── desktop/                        # PC 专属 (sidecar/dream/kg/wechat/web_drop/tunnel...)
│   └── mobile/                         # 手机专属 (qr_scanner/voice/share_receiver/lan_sync)
```

原则：
1. Shell + View 分层，共享组件沉淀到 shared/
2. Rust #[cfg()] 条件编译控制平台差异
3. tauri-bridge.js 统一接口，内部分叉

---

## 四、UI/UX 差异化

| 维度 | Desktop | Mobile |
|------|---------|--------|
| 布局 | 侧栏 + 主内容区 (三栏) | 沉浸式状态栏 + 底部导航 (Bottom Nav) + 对话双层级回退视图 |
| 输入 | 键盘 + 快捷键 | 语音 + 触屏 + 系统分享 |
| 知识图谱 | Vis.js 力导向图 | 不呈现 |
| Tool Calling | 完整过程可视化 | 隐藏细节，只展示结果 |
| 设置 | 完整多 Tab | 极简：模型 + 同步状态 |

防暴设计：禁止 Goal Mode、禁止文件写入工具、语音确认缓冲、流量感知提醒

### 4.1 移动端专属 UX 技术规范 (M2 核心要求)

为了确保 Bob-Mobile 达到顶级原生 App 的体感，开发时必须严格遵守以下技术规范：

1. **全面沉浸式与安全区自适应 (SafeArea)**
   - **原理**: 绝不硬编码顶部留白。Tauri 需配置为全屏模式（沉浸状态栏）。
   - **实现**: 在 Vue 根组件或布局容器的 CSS 中，必须使用 `padding-top: env(safe-area-inset-top);` 和 `padding-bottom: env(safe-area-inset-bottom);`。这样系统会自动避开刘海屏、摄像头开孔以及底部的“小白条”，做到内容充实且不遮挡系统关键信息。
2. **强制锁定竖屏 (Portrait Lock)**
   - **原理**: AI 效率工具（大量阅读和聊天流）在横屏下高度极度受限且易被输入法完全遮挡。
   - **实现**: 必须在 `src-tauri/gen/android/app/src/main/AndroidManifest.xml` (或通过 tauri 移动端配置) 中将 `android:screenOrientation` 写死为 `portrait`。
3. **原生返回手势劫持 (Edge-Swipe Back)**
   - **原理**: 安卓设备的边缘侧滑或物理返回键，默认会触发 WebView 的历史回退。如果栈底被击穿，App 会意外退出或切入后台。
   - **实现**: 必须在 Vue Router 全局守卫或顶层组件挂载时监听物理返回事件。逻辑链：若有弹窗/侧边栏打开 → 优先关闭它；若在聊天二级界面 → 执行 `router.back()` 回到主界面；只有在主界面栈底 → 提示“再按一次退出”或允许退后台。
4. **全局悬浮唤醒球 (Floating Action Button - FAB)**
   - **原理**: 手机端难以精准点击边角固定图标，改用原生 App 常见的“全局拖拽悬浮球”进行碎片输入。
   - **实现**: 在 Vue 层全局挂载一个具有 `z-index: 9999` 的半透明 Bob 图标。使用 VueUse 的 `useDraggable` 实现全屏物理拖拽，并在松手时实现“自动贴边吸附”动画。点击该悬浮球，即可一键唤醒语音听写或闪念胶囊输入法。

---

## 五、跨端通信架构（复用现有基建）

### 5.1 现有基建可直接复用

| 已有模块 | 位置 | Mobile 复用方式 |
|----------|------|----------------|
| **bob-relay** | VPS1 (Node.js) | 直接复用为设备信令红娘 |
| **coturn** | VPS1 | 直接复用为 STUN/TURN (自有服务器) |
| **web_drop.rs** | bob-agent | 复用 WebRTC 引擎改造为同步通道 |
| **http_api.rs** | bob-agent | 局域网直连 REST API 入口 |
| **微信/TG Bot** | wechat/ + telegram.rs | 跨网推送唤醒 + 信令降级 |

### 5.2 四级渐进式连接策略

| 优先级 | 连接方式 | 速度 | 经过 VPS? |
|:---:|----------|------|:---------:|
| 1 | 局域网直连 (http_api.rs) | 全速 | 否 |
| 2 | WebRTC P2P 打洞 (web_drop 引擎) | 全速 | 仅信令 |
| 3 | TURN 中继 (coturn) | 受限 | 是 |
| 4 | 微信/TG 推送等待 | 人工触发 | 否 |

打洞原理（与 RustDesk/向日葵完全相同）：
1. 双方通过 coturn (STUN) 发现公网地址
2. 通过 bob-relay 交换 ICE Candidate
3. 双方同时向对方发 UDP 包，NAT 被骗开
4. P2P 直连建立，数据不经过任何服务器

### 5.3 同步流程 (PC 主导唤醒)

PC 启动后：
- 注册到 bob-relay，查询手机是否在线
- 情况 A (同一局域网): UDP 广播发现 → HTTP 直连同步
- 情况 B (不同网络 + App 在线): bob-relay 信令 → coturn 打洞 → WebRTC P2P
- 情况 C (App 被杀后台): 微信 Bot 推送唤醒 → 用户打开 App → 走情况 B

### 5.4 SyncPacket 数据模型

```rust
struct SyncPacket {
    id: String,
    created_at: i64,
    packet_type: SyncPacketType,  // ChatMessage | VoiceMemo | SharedArticle | TodoIntent
    payload: serde_json::Value,
    synced: bool,
}
```

### 5.5 config_version 配置同步

每次握手时 PC 携带 config_version。手机落后则自动拉取最新配置。

---

## 六、bob-relay 进化路径

1. **现状** (已完成): Web Drop WebSocket 信令
2. **下一步**: 增加设备注册协议 (register/query/notify)，半天工作量
3. **远景**: Rust 重写为独立产品级服务 (合并 STUN+信令+TURN)，有外部用户时再做

---

## 七、开发纪律

1. Bridge 双端清单: 改 tauri-bridge.js 必须同步检查两端
2. 双端编译检查: 改 core/ 后运行 cargo check --features desktop 和 mobile
3. 响应式 CSS: 共享组件必须用 CSS 变量适配两端
4. 平台标签: Rust 文件顶部标注 shared/desktop-only/mobile-only
5. Schema 兼容: 数据库变更走 Migration，新列必须有 DEFAULT

---

## 八、迁移路线图 (对应 todo.md 目标 22-23)

| 阶段 | 内容 | 类型 | 状态 |
|------|------|:----:|:----:|
| Phase 0 | Tauri 升级到 v2 stable | 改造 | ✅ 已完成 |
| Phase 1 | 解决 Android SQLite 沙盒隔离，Rust 引擎跑通 | 改造 | ✅ 已完成 (v0.4.4 达成) |
| Phase 2 | **手机端 MVP (UI 适配与裁剪)** | 重构 | 🏃 当前 Sprint |
| Phase 2.5 | **移动端体验优化 (图标/Onboarding/FAB Bug)** | Bug Fix | 🏃 当前 Sprint |
| Phase 3 | **端侧 LLM 框架 (llama.cpp 集成)** | 新建 | 🔜 规划中 |
| Phase 4a | bob-relay 增加设备注册 + 扫码配对 MVP | 改造 | 🏃 当前 Sprint |
| Phase 4b | web_drop → 持久同步通道 | 改造 | 🔜 规划中 |
| Phase 4c | SyncPacket 协议 + 认证 (Ed25519 + AES-GCM) | 新建 | 🔜 规划中 |

---

## 九、风险与深坑预警

| 坑位 | 问题 | 解法 |
|------|------|------|
| 文件系统沙盒 | 手机不能硬编码路径 | Tauri appDataDir() API |
| iOS 局域网权限 | mDNS 扫描被拦截 | PC 主导策略 |
| iOS ATS | 明文 HTTP 被阻断 | WebRTC DataChannel |
| 后台被杀 | iOS/Android 冻结后台 | 手机任务必须瞬态 |
| 虚拟键盘 | WebView 高度被挤压 | Visual Viewport API |
| DHCP 漂移 | 手机 IP 每天可能变 | UDP 心跳注册包 |
| 配置过期 | PC 换 API Key 手机不知道 | config_version 同步 |
| 不兼容 Crate | chromiumoxide/pdfium 等 | #[cfg(feature = desktop)] |
| iOS 物理限制 | Windows 无法编译 iOS | 先做 Android，iOS 用 CI/CD |

---

## 附录：竞品对比

| 维度 | Bob (PC+手机) | Obsidian/Roam | Notion AI | IMA/NotebookLM |
|------|:---:|:---:|:---:|:---:|
| 数据主权 | ★★★ | ★★★ | ★☆☆ | ★☆☆ |
| 知识输入 | ★★★ | ★☆☆ | ★☆☆ | ★★☆ |
| 知识互联 | ★★★ | ★★☆ | ★★☆ | ★☆☆ |
| 全天候记录 | ★★☆ | ★★☆ | ★★★ | ★★★ |
| 智能深度 | ★★★ | ★☆☆ | ★★☆ | ★★☆ |
