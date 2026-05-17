# Bob Agent User Guide / 用户手册

> Version 1.0.0-alpha (Tauri)

## 1. Getting Started / 快速上手

### Prerequisites / 系统要求

- Windows 10/11 (64-bit)
- 至少一个大模型供应商的 API Key（如 DeepSeek、OpenAI）

### For Developers / 开发者运行

```bash
# 需要: Node.js 18+, Rust toolchain, VS C++ Build Tools
npm install
npm run dev:tauri
```

### First-Time Setup / 首次配置

1. 启动后会出现设置向导（SetupWizard）
2. 进入 **设置 → API 密钥管理**，填入 API Key
3. 返回对话界面，开始聊天！

> 💡 也可以直接在对话中告诉 Bob："帮我配好这个 Key: sk-xxx"，他会通过 Outbox 系统自主完成配置。

## 2. Core Features / 核心功能

### 💬 Smart Chat / 智能对话

- 输入框输入后按 Enter 发送
- 支持 Markdown 渲染、代码高亮、文件引用
- 底部费用指示器显示每次对话的预估消费（¥）

### 🤖 Agent Mode / Agent 模式

Bob 拥有 12 个 Rust 原生工具，可以自主调用：

| 工具 | 功能 |
|------|------|
| `read_file` / `write_file` / `list_dir` | 读取、写入、浏览本地文件 |
| `append_file` | 向文件追加内容 |
| `web_search` | 搜索互联网（需要 Tavily API Key） |
| `fetch_url` | 抓取网页内容并提取纯文本 |
| `brain_search` | 检索知识库 (wiki/ 目录) |
| `add_calendar_event` | 向日程表添加事件或待办 |
| `system_time` | 获取当前系统时间和日期 |
| `get_weather` | 查询城市实时天气 |
| `list_skills` / `read_skill` | 浏览和读取认知技能 |

### 📸 Image Analysis / 图片分析

- **Ctrl+V** 粘贴截图或图片
- **拖拽** 图片文件到对话窗口
- Bob 会使用 Vision 能力分析图片内容

### 📅 Calendar / 日程管理

- 在对话中说 "明天下午3点开产品评审会" 或 "帮我记录一小时后去剪头发"
- Bob 会调用 `add_calendar_event` 工具，将日程写入数据库
- 点击左侧导航栏的 **日程** 查看周时间轴和待办列表
- 时间轴支持**拖拽调整**事件时间

### 📁 Knowledge Base / 知识库

1. 在设置面板的 **Bob 工作空间** 中设置知识库目录
2. **拖拽文件夹**到对话窗口，Bob 会自动索引
3. 在对话中说 "搜索我的知识库" 即可检索已索引内容
4. 支持 .md / .txt / .pdf / .docx / .xlsx 等格式

## 3. Settings / 设置详解

### Model Hub / 模型中心

- Bob 自动发现 40+ 模型（来自已配置的 API Key）
- 指派 **Main Model**（主对话）和 **Clerk Model**（后台任务）
- 支持 DeepSeek、OpenAI、Qwen、Doubao、GLM、Kimi、MiniMax、ModelScope(免费) 等

### Bob 工作空间

- **知识库目录**：设置 wiki 知识库的本地路径
- **关注的文件夹**：添加/移除要跟踪的文件夹
- **MCP 服务器**：配置 Model Context Protocol 服务器

### Theme / 主题

- **Dark / Light / System** — 平滑 CSS 过渡
- **Accent Color** — 从预设色板选择

### Language / 语言

- 简体中文 / English — 全双语 UI

### About / 关于

- **打开数据目录**：查看本地数据库、记忆和知识库文件
- **打开日志目录**：访问错误日志
- **清除所有数据**：恢复出厂设置（谨慎使用！）

## 4. Keyboard Shortcuts / 快捷键

| Shortcut | Action |
|----------|--------|
| `Ctrl+Shift+B` | **全局唤醒** — 在任意应用中按下，立即唤起 Bob 窗口 |
| `Ctrl+V` | 粘贴图片到对话 |
| `Enter` | 发送消息 |
| `Shift+Enter` | 输入框换行 |

## 5. Troubleshooting / 故障排查

### Bob 没有回复

1. 检查 设置 → API 密钥管理 — 是否至少配置了一个供应商？
2. 检查底部状态栏 — 是否选中了一个模型？
3. 打开日志目录（设置 → 关于）查看错误日志

### 日程面板为空

- 确保在对话中使用 Agent 模式让 Bob 添加日程
- Bob 会调用 `add_calendar_event` 工具，成功后刷新日程面板即可看到

### 应用打开了两个窗口

- 已通过 `tauri-plugin-single-instance` 修复
- 如果仍有残留进程，在任务管理器中关闭后重启

## 6. Data & Privacy / 数据与隐私

- **所有数据保持在本地** — 无遥测、无云同步
- API Key 存储在本地 config.json 中（加密存储计划中）
- 对话历史存储在本地 SQLite 数据库
- 记忆文件为纯 Markdown，位于 `data/memory/` 和 `data/wiki/`
- **工具调用审计日志** — 每次 Agent 调用工具的操作记录保存在 `logs/tools.log`
- 随时可通过 设置 → 清除所有数据 删除全部数据
