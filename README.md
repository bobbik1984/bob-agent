# bob-agent — AI 桌面私人秘书

> 给不喜欢折腾的朋友用的 Windows AI 助手

一个开箱即用的桌面应用，融合**智能对话**、**图片识别**、**日程管理**和**文件分析**四大能力。

## 核心功能

| 功能 | 描述 |
|------|------|
| 💬 **智能对话** | 多模型聊天（DeepSeek/OpenAI/Ollama），流式输出 + Markdown 渲染 |
| 📸 **图片识别** | 截图/粘贴/拖拽图片，DeepSeek V4 Vision 分析内容 |
| 📅 **智能收件箱** | 自然语言录入日程，截图自动提取事件，同步 Microsoft 365 日历 |
| 📁 **文件分析** | 拖拽本地文件（.docx/.xlsx/.pdf/.csv/.md），AI 深度分析 |

### 杀手级场景

```
同事微信群发了会议通知截图
  → Ctrl+V 粘贴进 bob-agent
  → Vision 识别："5月15日 14:00 产品评审会 xx大厦"
  → 弹出确认卡片，一键写入日历
  → 到时间自动桌面通知提醒
```

## 技术栈

| 组件 | 技术 |
|------|------|
| 桌面框架 | Electron |
| 前端 | Vue 3 + Vite |
| LLM | OpenAI SDK (兼容 DeepSeek/OpenAI/Ollama) |
| 数据库 | SQLite (better-sqlite3) |
| 日历 | Microsoft Graph API (@azure/msal-node) |
| 文件解析 | mammoth (docx) + xlsx + pdf-parse |

## 快速开始

```bash
# 安装依赖
npm install

# 开发模式
npm run dev

# 打包 Windows 安装包
npm run pack
```

## 项目文档

- [AGENTS.md](AGENTS.md) — AI 编码代理行为准则
- [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md) — 产品需求文档
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — 技术架构详解
- [docs/DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md) — 开发计划
- [docs/TODOLIST_INTEGRATION.md](docs/TODOLIST_INTEGRATION.md) — TodoList 集成指南

## 生态定位

```
bob-agent (Windows 桌面)     ←→     TodoList (VPS1 微服务)
├── 桌面对话 + Vision                 ├── Telegram Bot 入口
├── 截图 → 日程                       ├── Discord Bot 入口
├── 文件拖拽分析                      ├── 每日 Briefing 推送
└── 全局快捷键                        └── 事件前提醒
         ↕ 同步点：Microsoft 365 Calendar ↕
```
