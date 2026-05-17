# 里程碑 10: 认知补全 — 架构考古报告

> 以下所有设计决策均**来自已有文档和过往对话**，不是重新构思。

---

## 📚 信息来源清单

| 来源 | 文件 | 关键内容 |
|------|------|---------|
| ① | [agents_electron.md D-008](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/docs/agents_electron.md#L285-L321) | 三层记忆引擎完整规格 |
| ② | [AGENTS.md D-005](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/AGENTS.md#L214-L216) | "Tier 1 灵魂 → Tier 2 短期 → Tier 3 长期" 确认 |
| ③ | Conv 42b23e6b step 9194 | wiki/memory 双存储目录结构讨论 |
| ④ | [ARCHITECTURE.md](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/docs/ARCHITECTURE.md) | data/ 目录结构、IPC 通道设计 |
| ⑤ | [capabilities_registry_design.md](file:///C:/Users/xm_bo/.gemini/antigravity/brain/42b23e6b-622b-44d5-87a4-2efebe0e73a8/capabilities_registry_design.md) | 外部能力声明式注册 + System Prompt 注入 |
| ⑥ | [weather/SKILL.md](file:///D:/OneDrive/Learning/Code/Gemini/Assistant/common/knowledge/skills/weather/SKILL.md) | 天气技能规格：Open-Meteo (免费) + wttr.in + 和风天气 |
| ⑦ | [REQUIREMENTS.md](file:///d:/OneDrive/Learning/Code/Gemini/bob-agent/docs/REQUIREMENTS.md) | 产品需求：三大支柱 + 用户画像 |
| ⑧ | [bob_agent_sync_strategy.md](file:///C:/Users/xm_bo/.gemini/antigravity/brain/42b23e6b-622b-44d5-87a4-2efebe0e73a8/bob_agent_sync_strategy.md) | bob-agent vs TodoList 同步策略 |

---

## 一、三层记忆引擎 (D-008 原文)

> 来源: agents_electron.md L285-L321

### 完整规格表

| 层级 | 存储位置 | 注入方式 | 触发时机 |
|------|---------|---------|---------|
| **Tier 1: 灵魂** | `data/memory/SOUL.md` | 每次全文注入 System Prompt | 每次对话开始 |
| **Tier 2: 短期记忆** | `data/memory/sessions/<id>.md` (mtime ≤7天) | 自动压缩注入 System Prompt | 每次对话开始 |
| **Tier 3: 长期记忆** | `data/wiki/sessions/<id>.md` + `data/wiki/projects/` + `data/wiki/clippings/` | 通过 `brain_search` 工具检索 | Bob 主动调用 |

### Session 总结生命周期（D-008 原文）

1. 用户切换/新建对话时，后台静默调用**廉价 LLM** 压缩旧对话为 ≤100 字总结
2. 总结写入 `memory/sessions/<conv-id>.md`（热记忆）
3. 超过 **7 天**未被访问的总结自动迁移到 `wiki/sessions/`（冷记忆）
4. 用户删除对话时，**级联删除** `memory/sessions/<id>.md` 和 `wiki/sessions/<id>.md`

### Session .md 格式（D-008 原文）

```markdown
---
conversation_id: <UUID>
title: <对话标题>
created: <YYYY-MM-DD>
---
<压缩总结内容>
```

### 安全网（D-008 原文）

- 第一层：每条消息实时写入 SQLite（原始数据永不丢失）
- 第二层：切换对话时后台生成压缩总结
- 第三层：启动时**补偿扫描**（处理崩溃/强关导致的未总结对话）

---

## 二、wiki/memory 双存储目录结构

> 来源: Conv 42b23e6b step 9194 + agents_electron.md L109-L117

### 对话中确认的目录布局

```
bob-agent/
├── data/                        # Bob 的"脑"，持久化目录
│   ├── wiki/                    # 客观知识池 — "Bob 知道什么"
│   │   ├── projects/            # 项目/主题的结构化摘要
│   │   │   └── 万象龙华.md
│   │   │   └── quant_lab.md
│   │   ├── clippings/           # AKP 收割的外部知识剪报
│   │   └── sessions/            # >7天沉淀的旧对话总结
│   │
│   └── memory/                  # 主观记忆 — "Bob 记得什么"
│       ├── SOUL.md              # Tier 1: 静态人格偏好
│       ├── preferences.json     # 用户偏好（通勤、饮食习惯等）
│       ├── patterns.json        # 行为模式（"用户一通常早晨问天气"）
│       └── sessions/            # Tier 2: ≤7天的对话压缩总结
│           └── <conv-id>.md
```

> [!IMPORTANT]
> **关键决策（Conv 42b23e6b）**: `data/` 放在 bob-agent 项目目录下（`d:\OneDrive\Learning\Code\Gemini\bob-agent\data\`），**不是 %APPDATA%**。这样：
> - 跟着 Syncthing/OneDrive 同步 → 换电脑不丢知识
> - 是用户数据的一部分，不是程序的一部分
> - `.gitignore` 排除，不进版本控制

---

## 三、天气技能分析

> 来源: weather/SKILL.md

### 依赖情况

| 数据源 | 依赖 | 费用 | 可 Rust 原生化？ |
|-------|------|------|----------------|
| **Open-Meteo** (首选) | 纯 HTTP GET | ✅ 完全免费，无需 Key | ✅ reqwest 直接调用 |
| wttr.in (备用) | 纯 HTTP GET | 免费 | 🟡 国内不稳定 |
| 和风天气 (备用) | 需 API Key | 免费额度 1000次/天 | ✅ 可选 |

### 原始技能的外部依赖

weather 技能依赖 `scripts/fetch_weather.sh`（shell 脚本）和 `scripts/geocode.sh`。这些是为 OpenClaw VPS 节点设计的，Bob 无法直接执行。

### 原生化路径

Open-Meteo 只需要两个 HTTP GET 请求：

```
1. 城市名 → 坐标
   GET https://geocoding-api.open-meteo.com/v1/search?name={city}&count=1&language=zh

2. 坐标 → 天气
   GET https://api.open-meteo.com/v1/forecast
       ?latitude=X&longitude=Y
       &current=temperature_2m,weather_code,wind_speed_10m
       &daily=temperature_2m_max,temperature_2m_min
       &timezone=Asia/Shanghai
```

reqwest 已经在 Bob 的 Cargo.toml 中，**零额外依赖**。

---

## 四、能力注册与 System Prompt 注入

> 来源: capabilities_registry_design.md

### 核心设计模式

capabilities_registry 的关键思想可以直接移植到 Bob：

```
System Prompt 中注入 [CAPABILITIES] 段：

[CAPABILITIES]
✅ 文件读取 (read_file)
✅ 目录浏览 (list_dir)
✅ 网页抓取 (fetch_url)
✅ 互联网搜索 (web_search: Tavily+TinyFish)
✅ 天气查询 (get_weather: Open-Meteo)
✅ 系统时间 (system_time)
✅ 知识库写入 (write_file: wiki/ 沙箱)
✅ 知识库搜索 (brain_search: wiki/ 全文检索)
✅ 技能库 (49 个外部技能可加载)
❌ 离线推理 (本地模型未配置)
❌ 浏览器自动化 (未集成)
```

**Bob 应该"知道自己能做什么和不能做什么"**。

---

## 五、当前状态 vs D-008 设计的差距

| D-008 要求 | 当前 Tauri 实现 | 差距 |
|-----------|----------------|------|
| SOUL.md 全文注入 System Prompt | ❌ 不存在 | 需创建模板 + llm.rs 注入 |
| sessions/ 热记忆 (≤7天) | 🟡 dream.rs V1 写 JSON | 需改为 .md 格式 + frontmatter |
| 廉价 LLM 压缩总结 | ❌ V1 是纯文本截取 | 需调 clerkModel 异步压缩 |
| 7 天自动迁移 wiki/sessions/ | ❌ 不存在 | 需后台定时任务 |
| wiki/projects/ 知识库写入 | ❌ 没有 write_file 工具 | **Phase 1 关键** |
| brain_search 工具检索 | ❌ 不存在 | 需新增工具 |
| 级联删除 | 🟡 todo.md T-631-mem 已定义 | Electron 版已做，Tauri 版待实现 |
| 启动补偿扫描 | 🟡 todo.md T-629 已定义 | Electron 版已做，Tauri 版待实现 |
| system_time 工具 | ❌ 不存在 | 纯 Rust 10 行 |
| weather 工具 | ❌ 技能存在但依赖 shell | 需原生化 (Open-Meteo HTTP) |
| data/ 在项目目录 | ❌ 当前 get_data_dir() → %APPDATA% | **需修改路径** |

---

## 六、实施建议

### Phase 1: 立即可做（工具补全）
1. `system_time` — 复用 outbox.rs 的时间逻辑
2. `write_file` — 沙箱限定 `data/wiki/` + `workspaceDir/`
3. `get_weather` — Open-Meteo 两步 HTTP GET
4. `brain_search` — 扫描 `data/wiki/` 全文关键词匹配

### Phase 2: 记忆引擎升级
1. 创建 `data/memory/SOUL.md` 初始模板
2. llm.rs 注入 SOUL.md + sessions/ 热记忆
3. dream.rs V1 → V2: .md 格式 + LLM 压缩
4. 7 天冷热迁移定时任务

### Phase 3: 数据目录迁移
1. `get_data_dir()` 从 `%APPDATA%/bob-agent/` 改为 bob-agent 项目目录下的 `data/`

> [!WARNING]
> Phase 3 影响较大：config.json、SQLite 数据库、logs 等都会跟着移动。
> 建议先只把 **wiki/** 和 **memory/** 放到项目目录，其余保持 %APPDATA%。
> 即：`get_wiki_dir()` 单独指向 `bob-agent/data/wiki/`，可通过 config.json 中的 `wikiDir` 配置。
