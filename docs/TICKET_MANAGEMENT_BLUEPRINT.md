# Bob Wallet: 票据与凭证管理系统设计蓝图

> **目标**: 让 Bob 成为一个不仅能“记事”，更能“装东西”的智能助理，完美替代 Google Wallet / Apple Wallet 的基础票据管理功能。
> **定位**: 将票据（门票、机票、电影票、会员卡）作为**知识图谱中的一等公民**，并与**日程表 (Calendar)** 深度双向联动。

## 1. 核心架构设计

我们废弃了最初“将票据作为独立笔记”的妥协方案，转而将票据（Ticket）定义为知识图谱（Knowledge Graph）中拥有确定结构语义的实体节点。

### 1.1 数据结构 (SQLite - `kg_nodes` / `events`)

在 `kg.rs` 中，新增节点类型 `ticket`：
- **type**: `"ticket"` (与 `concept`, `entity`, `document` 并列)
- **metadata (JSON)**: 
  ```json
  {
    "category": "flight" | "movie" | "exhibition" | "membership",
    "venue": "深圳国际会展中心",
    "start_time": "2026-09-09T09:00:00",
    "end_time": "2026-09-11T16:00:00",
    "status": "upcoming" | "used" | "expired",
    "qr_image_path": "assets/tickets/qr_cioe_2026.png",
    "barcode_data": "5832604123", 
    "seat_info": "12号馆 C1入口"
  }
  ```
在 `calendar.rs` 中，`events` 表追加关联字段：
- **`linked_ticket_id`**: 指向知识图谱中 `ticket` 节点的 UUID。

### 1.2 语义网状关系 (Edges)

新建的 Ticket 节点将自动在图谱中建立关联：
- `[scheduled_on]` → Calendar Event (触发日历 UI 显示门票图标)
- `[held_at]` → Location Entity (如：深圳国际会展中心)
- `[belongs_to]` → Event Concept (如：CIOE 中国国际光电博览会)

---

## 2. 交互与用户流 (User Journey)

### 2.1 录入：视觉大模型 + 自动编排

1. **拖入/粘贴截图**：用户将一张门票、登机牌或包含二维码的展会确认单扔给 Bob。
2. **多模态提取 (LLM Vision)**：
   - 提取票务核心信息（时间、地点、座位、单号）。
   - 裁剪并提取二维码/条形码区域（保存到本地文件系统）。
3. **调用复合工具 `create_ticket`**：
   - LLM 调用该工具，传入提取好的 JSON 数据。
   - Rust 后端原子化执行三件事：
     1. 创建 `ticket` 图谱节点。
     2. 创建 `calendar` 日程事件，并写入 `linked_ticket_id`。
     3. 建立 `[scheduled_on]` 和 `[held_at]` 关系边。

### 2.2 展示：PC 端的票夹中心

1. **日历视图融合**：在 `WeekTimeline.vue` 中，如果检测到事件包含 `linked_ticket_id`，则在其下方显示醒目的 `[🎫 查看入场凭证]` 按钮。点击从侧边滑出票据详情面板。
2. **知识图谱"票夹"视图**：在 `KnowledgeGraphView.vue` 顶部增加 `[🎫 票夹]` 过滤 Chip。按时间线排列：
   - **即将到来 (Upcoming)**：高亮显示，直接展出二维码缩略图。
   - **已过期/已使用 (Expired)**：自动灰化，折叠沉底。

### 2.3 移动端：当天的“刷卡”体验

手机端 Bob 作为最佳载体，提供极简的检票体验：
- 距离日程开始前 24 小时，手机端日历置顶显示该票据卡片。
- **一键高亮**：点击卡片中的二维码，自动将屏幕亮度调至最高，全屏居中显示二维码/条形码，并锁住屏幕旋转，方便扫码入场。

---

## 3. 开发实施计划 (Implementation Plan)

### Phase 1: 数据层与后端 API (Rust)
- [ ] 扩展 `events` 表的 Schema，增加 `linked_ticket_id`，并提供迁移脚本（Migration）。
- [ ] 扩展 `kg.rs` 的插入逻辑，支持 `ticket` 类型的专用元数据验证。
- [ ] 在 `tools.rs` 中新增 `create_ticket` 工具暴露给 LLM，处理自动建图谱、建日程的级联逻辑。

### Phase 2: 前端组件与日历适配 (Vue)
- [ ] 开发通用组件 `TicketCard.vue`，支持根据 `category` (机票、电影、展会) 渲染不同的微件（如：登机口、座位号、展馆号）。
- [ ] 改造 `WeekTimeline.vue`，在日历卡片中挂载 `TicketCard` 的缩略形态。
- [ ] 在 `KnowledgeGraphView.vue` 中增加票夹视图过滤器。

### Phase 3: 多模态与移动端体验 (Vision & Mobile)
- [ ] 调试大模型的 System Prompt，指导其如何从包含二维码的截图中准确提取 `create_ticket` 所需的结构化参数。
- [ ] 移动端专属适配：实现二维码点击放大、强制屏幕高亮（可能需要 Tauri 插件调取系统亮度 API）。
- [ ] 增加会员卡 (Membership) 支持：用户可手动输入条形码号码生成通用会员卡。
