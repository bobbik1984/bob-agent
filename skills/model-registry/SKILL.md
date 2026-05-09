---
name: model-registry
description: 大模型统一信息源（Single Source of Truth）。查询所有已接入的 LLM 供应商名称、API Base URL、精确模型 API 字符串（model_id）、上下文窗口、输入/输出定价。任何项目在书写配置文件、生成连接脚本、或路由模型调用前，必须先查询此技能的 references/unified_model_registry.json 对齐标准信息，严禁自行猜测模型名称。当用户提到"模型价格"、"API 端点"、"模型列表"、"哪个模型便宜"、"添加新模型"、"更新模型信息"时触发。
version: 2.1.0
tags: [SystemOps]
related_skills: [cluster_ops, assignment]
---

# Model Registry (大模型统一字典)

## 目标

作为整个 Gemini 工程体系内**唯一**的大模型信息权威来源，终结以下历史遗留问题：
- 多个文件对同一个 Provider 使用不同名称（如 "Alibaba (Qwen)" vs "aliyun" vs "qwen"）。
- API Base URL 存在互相矛盾的版本（带不带 `/v1`、兼容模式 vs 原生模式）。
- 模型 API 字符串散落各处、拼写不统一，导致实际调用 404。

## 核心规则

1. **禁止猜测模型名**：无论你是生成 Python 连接脚本、修改 YAML、还是配置路由表，都必须先读取 `references/unified_model_registry.json`，从中取用精确的 `api_string` 字段。
2. **禁止猜测 Base URL**：每个 Provider 的 `api_base` 字段是唯一标准。如果需要 OpenAI 兼容模式，使用 `api_base_openai_compat` 字段（如果存在）。
3. **价格以 CNY（人民币）为基准单位**：统一使用"每 1M Token 人民币"作为计价标准。
4. **API 密钥不在此文件中**：密钥统一存放在 `D:\OneDrive\Learning\Code\Gemini\.env`。此技能的 `references/env_key_mapping.md` 记录了每个 Provider 对应的环境变量名称。
5. **不提供选型决策树**：本技能只提供价格和能力数据，由消费方工具根据自身需求自主决策。

## 消费者契约（谁必须读取此技能）

以下场景的 Agent 在执行前 **必须** 先读取 `references/unified_model_registry.json`：

| 场景 | 必须读取的字段 |
|------|---------------|
| 生成 API 调用代码 | `api_base`, `api_string`, `api_key_env` |
| 配置 YAML/JSON 中的模型字段 | `api_string`（禁止手写） |
| 比较模型价格 / 成本估算 | `input_price_cny_per_1m`, `output_price_cny_per_1m`, `tier` |
| 调用图像 / 视频生成 | `invocation` 字段（非标准协议） |
| 部署新节点 / 修改 .env 配置 | `api_base_openai_compat`, `api_key_env` |
| 判断模型能力（是否支持视觉/推理） | `capabilities` 数组 |

## 数据结构说明

`references/unified_model_registry.json` 采用按 Provider 分组的树形结构：

```json
{
  "<provider_id>": {
    "display_name": "对人可读的供应商名称",
    "official_pricing_url": "官方定价页链接",
    "api_base": "原生 API 端点",
    "api_base_openai_compat": "OpenAI 兼容模式端点（如有）",
    "api_key_env": "对应的 .env 环境变量名",
    "models": {
      "<model_display_name>": {
        "api_string": "实际发送给 API 的模型字符串",
        "context_window": 128000,
        "max_output_tokens": 8192,
        "input_price_cny_per_1m": 0.0,
        "output_price_cny_per_1m": 0.0,
        "tier": "heavy|medium|light",
        "capabilities": ["chat", "code", "vision", "reasoning"]
      }
    }
  }
}
```

### 非标准调用模型 (invocation 字段)

部分模型（如图像生成、视频生成）**不走标准的 OpenAI Chat Completion 协议**。这些模型在 registry 中会附带一个 `invocation` 对象，明确告知 Agent 应该使用哪种调用方式：

```json
{
  "invocation": {
    "protocol": "dashscope-native",
    "api_base": "https://dashscope.aliyuncs.com/api/v1",
    "sdk_class": "dashscope.aigc.image_generation.ImageGeneration",
    "method": "async_call + wait",
    "api_docs": "https://..."
  }
}
```

**关键区分规则**：
- 如果模型条目**没有 `invocation` 字段** → 使用标准 OpenAI 兼容 Chat Completion API（`openai.ChatCompletion.create()`）。
- 如果模型条目**有 `invocation` 字段** → 必须按其指定的 `sdk_class` 和 `method` 调用。绝不能用 Chat Completion 强行调用。
- `invocation.api_base` 可能与 Provider 顶层的 `api_base` 不同（如阿里云百炼的文本模型走 `/compatible-mode/v1`，但生图/生视频走原生的 `/api/v1`）。

**capabilities 能力标签速查**：
| 标签 | 含义 |
|------|------|
| `chat` | 标准文本对话 |
| `code` | 代码生成与分析 |
| `vision` | 图像理解（多模态输入） |
| `reasoning` | 深度推理 / 思维链 |
| `image-generation` | 文生图 ⚠️ 非标准调用 |
| `video-generation` | 文/图生视频 ⚠️ 非标准调用 |

## 更新流程

当需要添加新模型或更新价格时：
1. 访问 `references/unified_model_registry.json` 中对应 Provider 的 `official_pricing_url`，获取最新官方数据。
2. 修改 `references/unified_model_registry.json` 中的对应条目。
3. 如果新增了 Provider，同步更新 `.env` 文件中的环境变量，并在 `references/env_key_mapping.md` 中记录映射关系。
4. 递增此技能的 `version` 字段。

## 参考文件

| 文件 | 用途 |
|------|------|
| `references/unified_model_registry.json` | 唯一权威：所有 Provider + 模型 + 价格 + 端点大全 |
| `references/env_key_mapping.md` | 每个 Provider 的 API Key 环境变量名映射表 |
