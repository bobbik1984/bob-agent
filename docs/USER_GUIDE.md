# Bob Agent 用户手册 / User Guide

> **Version v0.4 (Tauri v2)**
>
> 零设置、懂你的、以结果为单位工作的个人执行系统。

---

## 目录 / Table of Contents

1. [快速上手 / Getting Started](#1-快速上手--getting-started)
2. [核心对话 / Smart Chat](#2-核心对话--smart-chat)
3. [目标模式 / Goal Mode](#3-目标模式--goal-mode)
4. [模型管理 / Model Hub](#4-模型管理--model-hub)
5. [MCP 服务器 / MCP Servers](#5-mcp-服务器--mcp-servers)
6. [知识库与知识图谱 / Knowledge Base & Graph](#6-知识库与知识图谱--knowledge-base--graph)
7. [日程与定时任务 / Calendar & Scheduling](#7-日程与定时任务--calendar--scheduling)
8. [微信互联 / WeChat Integration](#8-微信互联--wechat-integration)
9. [Web Drop / 文件极传](#9-web-drop--文件极传)
10. [连接器 / Connectors](#10-连接器--connectors)
11. [记忆与自进化 / Memory & Evolution](#11-记忆与自进化--memory--evolution)
12. [设置详解 / Settings Reference](#12-设置详解--settings-reference)
13. [快捷键 / Keyboard Shortcuts](#13-快捷键--keyboard-shortcuts)
14. [故障排查 / Troubleshooting](#14-故障排查--troubleshooting)
15. [数据与隐私 / Data & Privacy](#15-数据与隐私--data--privacy)

---

## 1. 快速上手 / Getting Started

### 系统要求 / System Requirements

- **操作系统**：Windows 10 / 11（64-bit）
- **API Key**：至少配置一个大模型供应商的 API Key（如 DeepSeek、OpenAI、通义千问等）

> 💡 如果暂时没有付费 Key，可以使用 ModelScope（魔搭社区）的免费 API Key 开始体验。

### 开发者运行 / Developer Setup

```bash
# 需要: Node.js 18+, Rust toolchain, VS C++ Build Tools
npm install
npm run dev:tauri
```

### 首次配置 / First-Time Setup

1. 首次启动后会自动弹出**设置向导（SetupWizard）**，引导你完成基础配置。
2. 进入 **设置 → 模型面板 → API 密钥管理**，填入你的 API Key。
3. 返回对话界面，开始和 Bob 聊天！

> 💡 **懒人模式**：你也可以直接在对话框中告诉 Bob：  
> *"帮我配好这个 Key: sk-xxx"*  
> Bob 会通过 **Outbox 系统**（AI 声明式配置管道）自主完成 API Key 的写入，无需手动去设置面板操作。

---

## 2. 核心对话 / Smart Chat

### 基础对话

- 在输入框中输入内容，按 **Enter** 发送消息。
- 按 **Shift + Enter** 可以在输入框内换行。
- Bob 回复支持 **Markdown 渲染**和**代码语法高亮**，阅读技术内容体验极佳。
- 每次对话底部会显示**费用指示器**（¥），让你实时了解 Token 消费。

### 今日概览 / Today Layer

- 新对话的空白首屏会显示一张紧凑 Today 卡片：一个当前焦点、最多两个需关注项，以及其余内容数量。
- 点击卡片或顶部的 Today 图标，会打开同一个详情层；日程、待办、项目、Goal、待确认选择、最近会话和 Dream 摘要按优先级汇总。
- 详情层不会替代聊天页面；手机端在弹层内部滚动，关闭后回到原来的输入位置。
- 悬浮速记底部也有 Today 入口。切换时未提交的速记草稿会被保留，再次打开速记可继续输入。
- Today 的日常汇总使用本地规则和 SQLite，不需要额外模型调用；断网时仍能展示本机已有状态。若某个来源暂时不可用，会显示“部分可用”，其他可靠内容不受影响。
- PC 和手机分别记录已读。只有内容 revision 发生变化时，该条目才会重新计入“有更新”。

### 📸 图片分析 / Vision

Bob 支持多模态视觉分析，你可以通过以下方式发送图片：

- **Ctrl + V**：直接粘贴屏幕截图或剪贴板中的图片。
- **拖拽**：将图片文件拖入对话窗口。
- **截屏**：Bob 可以通过工具截取当前桌面画面并进行分析。

发送后，Bob 会使用 Vision 能力识别和分析图片内容。

### 🤖 Agent 模式 / Agent Mode

开启 Agent 模式后，Bob 可以自主调用一系列 Rust 原生工具来完成复杂任务。以下是当前全部可用工具：

| 工具 | 功能说明 |
|------|----------|
| `read_file` | 读取本地文件内容 |
| `write_file` | 写入/创建本地文件 |
| `append_file` | 向已有文件追加内容 |
| `list_dir` | 浏览目录结构 |
| `web_search` | 搜索互联网（需配置 Tavily API Key） |
| `fetch_url` | 抓取网页内容并提取纯文本 |
| `brain_search` | 检索本地知识库（Wiki 目录） |
| `add_calendar_event` | 向日程表添加事件或待办 |
| `system_time` | 获取当前系统时间和日期 |
| `get_weather` | 查询城市实时天气 |
| `list_skills` | 浏览可用的认知技能列表 |
| `read_skill` | 读取特定认知技能的内容 |
| `send_wechat_file` | 向微信联系人发送文件 |
| `share_file` | 通过 Web Drop 生成文件分享链接 |

> **Restatement 机制**：在较长的多步骤工具调用链中，Bob 会自动进行"目标重述"——在每一轮工具调用后重新回顾原始目标，防止大模型在长链路中丢失焦点或偏离方向。这让复杂任务的完成率大幅提升。

---

## 3. 目标模式 / Goal Mode

### 什么是 Goal Mode

默认 Auto 已能把需要跨时间、阶段推进或中断恢复的请求转成持续目标。你不需要理解 Goal、DAG 或模型角色：只要用普通语言说明想要的结果，Bob 会选择直接回答、当前会话深入处理，或创建一个可恢复的 Advanced Goal。

### 如何使用

保持 **Auto** 即可。若 Bob 创建了持续目标，聊天回复旁和“工作”页面会显示当前状态、下一步、风险、模型/工具调用数、恢复点和证据状态。

### 运行机制

1. Bob 保存目标、范围、限制、预算、风险和完成证据。
2. 明确且低风险的工作自动执行一个有界切片；重要选择或高风险动作显示 2–4 个明确选项。
3. 工具回执或用户验收通过后才会显示“已验证完成”；证据不足时显示等待、阻塞或失败原因。
4. 应用重启后，Bob 只会自动恢复安全、可确认没有未知副作用的 R0/R1 工作。

### 适用场景

- 需要多步骤协作的复杂任务（例如：调研 + 整理 + 撰写完整报告）
- 对输出质量有较高要求、不希望一问一答浅尝辄止的场景
- 完成条件较清楚、允许有限重试的工作

> 当前限制：应用关闭期间不会后台执行；尚无 Dynamic DAG、多 Agent 或完整跨端 R3 批准。手动 **Goal Mode** 仍是旧 Maker–Checker 实验入口，不具备新 Runtime 的持久语义。

---

## 4. 模型管理 / Model Hub

### 自动发现 / Auto-Discovery

Bob 的后端会读取模型供应商注册表。当你填入任一 OpenAI 兼容供应商的 API Key（如 DeepSeek、通义千问、火山引擎、ModelScope 等）后，系统会自动向供应商的 `/models` 端点发送探测请求，**自动发现并录入最新上线的模型**。

- 自动过滤掉 Embedding、TTS、语音等非聊天模型，保持列表整洁。
- 新发现的模型默认隐藏，由你自行勾选可见，避免列表过载。
- 支持手动点击**刷新**按钮重新扫描。

### 主 / 副模型角色 / Main & Clerk Model

| 角色 | 用途 | 推荐模型 |
|------|------|----------|
| **Main Model**（主模型） | 核心对话、复杂推理、工具调用决策 | deepseek-chat、glm-4-plus 等强推理模型 |
| **Clerk Model**（副模型） | 后台脏活：晨报整理、会话压缩、记忆提取、Goal Mode 评估 | ModelScope 免费模型、极低成本模型 |

通过主副模型分工，可以最大限度节省 Token 预算。

### 添加自定义模型 / Custom Models

如果你使用的供应商不在预置列表中，可以在 **设置 → 模型面板** 中手动添加自定义模型：
- 填写 Model ID、显示名称、API Base URL 和 API Key 即可。
- 自定义模型与自动发现的模型统一管理。

### 离线引擎 / Offline Engine (Sidecar)

Bob 内置了 `llama-server` 进程管理器，支持加载本地 GGUF 模型文件：
- 在无网络或机密环境下提供 **100% 本地离线推理**。
- 通过 **设置 → 模型面板 → 离线引擎** 管理，选择本地模型文件即可启动。

---

## 5. MCP 服务器 / MCP Servers

### 什么是 MCP

**MCP（Model Context Protocol）** 是一种开放协议，允许外部工具服务与 AI 模型进行标准化交互。Bob 内置了 MCP 客户端，可以启动和管理第三方 MCP Server，将它们的工具无缝集成到 Bob 的工具集中。

### 如何配置

1. 进入 **设置 → 工作空间 → MCP 服务器**。
2. 以 JSON 格式添加 MCP 服务器配置：

```json
{
  "server_name": {
    "command": "npx",
    "args": ["-y", "@example/mcp-server"],
    "env": {
      "API_KEY": "your-key-here"
    }
  }
}
```

| 字段 | 说明 |
|------|------|
| `command` | 启动命令（支持 `npx`、`npm`、`python`、二进制可执行文件） |
| `args` | 命令参数数组 |
| `env` | 可选的环境变量 |

### 工具合流

- 启动时，Bob 会通过 `tools/list` 自动获取 MCP 服务暴露的所有工具。
- 所有 MCP 工具自动加前缀（格式：`mcp_{server_name}_{tool_name}`），避免命名冲突。
- MCP 工具与 Bob 的 14 个原生工具**无缝合流**，供大模型统一调用。
- 在 Windows 上会自动适配 `.cmd` 后缀以保证兼容性。

> ⚠️ 首次启动基于 `npx` 的 MCP 服务时，可能需要等待 60 秒左右用于拉取依赖包。

---

## 6. 知识库与知识图谱 / Knowledge Base & Graph

### 📁 知识库 / Knowledge Base

#### 建立知识库

1. 在 **设置 → 工作空间** 中设置 Wiki 知识库目录。
2. **拖拽文件夹**到 Bob 的对话窗口，Bob 会自动扫描并索引。

#### 支持格式

- `.md`（Markdown）
- `.txt`（纯文本）
- `.pdf`（PDF 文档）
- `.docx`（Word 文档）
- `.xlsx`（Excel 表格）

#### 使用检索

在对话中自然地提出问题，Agent 模式下 Bob 会自动调用 `brain_search` 工具检索知识库内容。你也可以直接说"搜索一下我知识库里关于 xxx 的内容"。

### 🕸️ 知识图谱 / Knowledge Graph

Bob 在本地 SQLite 数据库中维护了一套**实体-关系知识图谱**，将你的知识以节点和边的形式结构化存储。

#### 如何访问

点击左侧导航栏的**知识图谱**图标，进入交互式图谱画布。

#### 核心功能

- **可视化力导向图**：所有节点和关系以动态物理仿真图呈现（基于 Vis.js），可拖拽、缩放。
- **BFS 局部子图探索**：以某一关键词为种子，向外进行 2-3 跳的广度优先搜索，在毫秒内渲染局部子图。非常适合"顺藤摸瓜"式的知识探索。
- **节点合并 / 别名消歧**：支持将同一实体的不同别名（例如 "DS" 和 "DeepSeek"）合并至主节点，自动重定向所有关联边，保证知识的单一真相源。

---

## 7. 日程与定时任务 / Calendar & Scheduling

### 📅 日程管理 / Calendar

#### 自然语言创建事件

在对话中用自然语言告诉 Bob，例如：

- *"明天下午 3 点开产品评审会"*
- *"帮我记录一小时后去剪头发"*
- *"周五上午 10 点和客户电话会议"*

Bob 会调用 `add_calendar_event` 工具，将日程写入本地数据库。

#### WeekTimeline 周时间轴

点击左侧导航栏的**日程**图标，可以看到完整的周视图时间轴：
- 所有事件以时间块形式展示在时间轴上。
- 支持**拖拽调整**事件的开始和结束时间。

#### TodoList 待办列表

除了时间事件外，Bob 还支持管理待办事项（Todo），在日程面板中统一展示。

### ⏰ Cron 定时任务 / Cron Jobs

Bob 内置了 Cron 定时任务引擎，可以定期自动执行 AI 任务：

| 操作 | 说明 |
|------|------|
| **添加定时任务** | 在 **设置 → 日常例程** 中创建，填写标题、Cron 表达式和执行提示词 |
| **启用 / 禁用** | 通过开关切换任务的激活状态 |
| **删除任务** | 移除不再需要的定时任务 |

例如，你可以设置一个每天早上 8 点运行的任务：*"检查今天的日程并生成晨间简报"*。

---

## 8. 微信互联 / WeChat Integration

### 如何连接

1. 进入 **设置 → 连接 → 微信**。
2. 扫描弹出的**登录二维码**，完成微信账户绑定。
3. 绑定成功后，你可以在手机微信端直接和 Bob 对话。

### 微信端指令 / Commands

在微信中与 Bob 对话时，可以使用以下指令：

| 指令 | 功能 |
|------|------|
| `/sessions` | 列出最近 5 次会话（带有"刚刚"、"15分钟前"等友好时间戳），回复序号即可切换会话 |
| `/new` | 开启一个新对话 |
| `/status` | 查看当前绑定的会话 ID |
| `/help` | 获取指令帮助 |

### URL 自动抓取

当你在微信里发送一个链接或卡片消息时，Bob 会在后台**自动抓取网页内容**（提取前 2000 字符），将其作为上下文喂给大模型分析——无需你手动复制粘贴网页内容。

### 文件传输

Bob 支持向微信联系人发送文件。在 Agent 模式下，Bob 可以调用 `send_wechat_file` 工具，自动完成文件加密上传和发送。

---

## 9. Web Drop / 文件极传

### 什么是 Web Drop

Web Drop 是 Bob 内置的**端到端加密文件分享**功能，让你可以极速地将文件从电脑传到手机（或任何有浏览器的设备）。

### 如何使用

1. 在 Agent 模式下，告诉 Bob：*"帮我分享这个文件"*，或者 Bob 在执行任务时会自动调用 `share_file` 工具。
2. Bob 会生成一个**分享链接**。
3. 在手机浏览器中打开这个链接，即可直接接收文件。

### 安全机制 / E2EE

- Bob 在本地内存中生成一个 Room ID 和 AES-128 GCM 加密密钥。
- 分享链接以 URL Hash Fragment 格式拼接（`#room_id.key`）。
- 由于浏览器不会将 `#` 后的内容发送给服务器，因此**中继服务器只能看到密文，无法解密文件内容**。
- 传输优先走 WebRTC 点对点直连，如遇 NAT 限制则自动降级为加密中继。

简单来说：**你的文件只有你和接收方能看到，任何中间人（包括服务器）都无法读取。**

---

## 10. 连接器 / Connectors

Bob 支持与多种外部服务集成，通过 **设置 → 连接** 面板进行配置：

| 连接器 | 功能 | 配置方式 |
|--------|------|----------|
| **Google Calendar** | 同步 Google 日历事件 | OAuth 授权登录 |
| **Gmail** | 读取和发送邮件 | OAuth 授权登录 |
| **飞书 / Lark** | 接入飞书工作台 | 填写凭据 |
| **Telegram Bot** | 通过 Telegram 远程控制 Bob | 填写 Bot Token |
| **Discord Bot** | 通过 Discord 远程控制 Bob | 填写 Bot Token |

连接成功后，Bob 可以在对话中调用相应的服务能力（如查看日历、发送邮件等）。

---

## 11. 记忆与自进化 / Memory & Evolution

### 三层记忆模型

Bob 拥有一个精心设计的三层记忆系统，让他真正"记住"你：

| 层级 | 名称 | 作用 |
|------|------|------|
| **Soul 层** | `SOUL.md` | Bob 的核心人设、沟通风格、你的硬性偏好。作为 System Prompt 直接注入每次对话。 |
| **Session 层** | 短期热记忆 | 当前会话及最近几轮对话上下文。7 天后自动冷迁移到长期存储。 |
| **Wiki 层** | 长期冷知识 | 本地 Wiki 知识沉淀，通过全文检索和图谱关联召回。 |

### 静默学习

每轮对话结束时，Bob 会在后台（由 Clerk 模型执行）自动分析对话内容，提取**持久性事实**，包括：

- **用户习惯**（user）：你的喜好偏好、工作风格
- **项目知识**（project）：技术选型、架构决策
- **纠偏记录**（feedback）：你对 AI 错误的纠正——**确保同样的错误不会犯第二次**
- **常用参考**（reference）：高频代码片段、常用命令或 URL

提取到的事实会自动保存为 Markdown 文件，并实时写入 SQLite 全文索引，使 `brain_search` 在下一轮对话中就能检索到。

### 🌙 做梦引擎 / Dream Engine

每隔 24 小时，当应用被唤醒时，Bob 会在后台静默"做梦"（Compaction）：

- 清理过期/临时事实
- 合并内容相似的冗余记忆
- **SOUL 自谐调**：将积累的纠偏记录和用户习惯重新写回 `SOUL.md`，实现人设的自我修正与进化

你会在下一次打开 Bob 时看到一份**晨间简报**（Dream Report），了解 Bob 在"睡梦中"做了哪些记忆整理工作。

---

## 12. 设置详解 / Settings Reference

### 模型面板 / Model Panel

- **API 密钥管理**：添加/修改各供应商的 API Key
- **Model Hub**：浏览自动发现的模型，切换可见性
- **角色指派**：设置 Main Model 和 Clerk Model
- **自定义模型**：手动添加不在预置列表中的模型
- **离线引擎**：管理本地 llama-server 和 GGUF 模型文件

### 工作空间 / Workspace

- **知识库目录**：设置 Wiki 知识库的本地路径
- **关注的文件夹**：添加/移除要跟踪的文件夹
- **MCP 服务器**：配置 Model Context Protocol 服务器（JSON 格式）

### 连接 / Connections

- **微信**：扫码登录，建立微信通信桥
- **Telegram**：填写 Bot Token，启用远程控制
- **Discord**：填写 Bot Token，启用远程控制
- **Google**：OAuth 授权，接入 Google Calendar 和 Gmail
- **飞书 / Lark**：填写凭据，接入飞书

### 日常例程 / Daily Routine

- **Cron 定时任务**：添加、删除、启用/禁用定时自动执行的 AI 任务

### 外观 / Appearance

- **主题模式**：Dark / Light / System — 平滑 CSS 过渡
- **强调色**：从预设色板中选择个性化主题色
- **语言**：简体中文 / English — 全双语 UI

### 关于 / About

- **打开数据目录**：查看本地数据库、记忆和知识库文件
- **打开日志目录**：访问错误日志和工具调用记录
- **版本信息**：当前 Bob Agent 版本号
- **恢复出厂设置**：清除所有数据（⚠️ 谨慎使用！）
- **Doctor 健康检查**：一键自检 API Key 连通性、数据库状态、沙箱权限等核心指标

---

## 13. 快捷键 / Keyboard Shortcuts

| 快捷键 | 功能 |
|--------|------|
| `Ctrl + Shift + B` | **全局唤醒** — 在任意应用中按下，立即唤起 Bob 窗口 |
| `Ctrl + V` | 粘贴图片到对话（截图或剪贴板中的图片） |
| `Enter` | 发送消息 |
| `Shift + Enter` | 输入框内换行 |
| **文件拖拽** | 将文件/文件夹拖入对话窗口，自动解析或索引 |

---

## 14. 故障排查 / Troubleshooting

### Bob 没有回复 / Bob Not Responding

1. **检查 API Key**：进入 设置 → 模型面板 → API 密钥管理，确认至少配置了一个供应商的 Key。
2. **检查模型选择**：确认底部状态栏已选中一个可用模型。
3. **查看日志**：通过 设置 → 关于 → 打开日志目录，检查错误日志。

### 🩺 Doctor 自检 / Health Check

在 **设置 → 关于** 中，点击 **健康检查（Health Check）**，Bob 会自动检测：

- API Key 连通性
- 本地 SQLite 数据库读写权限
- Tauri 后端沙箱路径白名单
- 本地 llama-server 进程状态
- 其他核心部件指标

### 🔧 一键自愈 / Auto-Fix

当健康检查发现问题时，Bob 提供**一键自愈修复**功能：
- 自动回滚安全备份
- 重构损坏的配置文件
- 重新初始化存储
- 全程不破坏用户核心数据

### 日程面板为空

- 确保在对话中使用 Agent 模式让 Bob 添加日程。
- Bob 会调用 `add_calendar_event` 工具，成功后刷新日程面板即可看到。

### 应用打开了两个窗口

- 已通过 `tauri-plugin-single-instance` 修复。
- 如果仍有残留进程，在任务管理器中关闭后重启即可。

---

## 15. 数据与隐私 / Data & Privacy

### 本地优先 / Local-First

- **所有数据保持在本地** — 无遥测、无云同步、无后台上传。
- 对话历史存储在本地 SQLite 数据库中。
- 记忆文件为纯 Markdown，位于 `data/memory/` 和 `data/wiki/` 目录。

### API Key 存储

- API Key 存储在本地 `config.json` 中。
- 加密存储功能已在规划中。

### 审计日志 / Tool Audit Log

- 每次 Agent 调用工具的操作记录保存在 `logs/tools.log`，可随时审查。

### 恢复出厂设置 / Factory Reset

- 随时可通过 **设置 → 关于 → 恢复出厂设置** 删除全部数据，回到初始状态。

---

> **Bob Agent — 少一点设置，多一点真正完成。**

### 💡 进阶技巧：如何让 Bob 读取巨型 Excel 报表？
遇到几十万字的财务台账？不要慌！
1. 直接将您的 `.xlsx` 或 `.csv` 报表拖入 Bob 的对话框。
2. 提出自然语言要求，例如：“帮我分析一下这张表里 LV 品牌的销售额”。
3. Bob 会自动通过底层的原生分析器（不用 Python！）提取关键数据列返回给您，不仅速度快，而且再也不用担心“读取不全”的报错了！
