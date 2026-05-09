# 环境变量 ↔ Provider 密钥映射表

本文件记录了 `unified_model_registry.json` 中每个 Provider 对应的环境变量名称，以便在 `.env` 文件中查找和配置。

| Provider ID | 显示名称 | `.env` 中的变量名 | 备注 |
|---|---|---|---|
| `google` | Google (Gemini/Gemma) | `GOOGLE_API_KEY` | 多账号时用 `GOOGLE_API_KEY_1`, `_2` |
| `openai` | OpenAI | `OPENAI_DIRECT_API_KEY` | 注意：`OPENAI_API_KEY` 已被 ModelScope 占用 |
| `anthropic` | Anthropic (Claude) | `ANTHROPIC_API_KEY` | |
| `deepseek` | DeepSeek | `DEEPSEEK_API_KEY` | |
| `qwen` | 阿里云百炼 (Qwen) | `DASHSCOPE_API_KEY` | |
| `modelscope` | ModelScope 魔搭 | `MODELSCOPE_API_KEY` | 免费推理，推荐 VPS 默认使用 |
| `kimi` | Kimi (月之暗面) | `KIMI_API_KEY` | 品牌已从 Moonshot 统一为 Kimi，API 域名仍为 moonshot.cn |
| `zhipu` | 智谱 AI (GLM) | `ZHIPU_API_KEY` | |
| `xai` | xAI (Grok) | `XAI_API_KEY` | |
| `openrouter` | OpenRouter 聚合网关 | `OPENROUTER_API_KEY` | |
| `mimo` | 小米 MiMo | `MIMO_API_KEY` | |
| `minimax` | MiniMax | `MINIMAX_API_KEY` | |
| `doubao` | Doubao (豆包/火山引擎) | `DOUBAO_API_KEY` | |

## ⚠️ 重要提醒

1. `OPENAI_API_KEY` + `OPENAI_BASE_URL` + `OPENAI_MODEL` 这组环境变量目前被用作 **OpenClaw/Claude Code 默认模型指针**，指向的是 ModelScope 免费推理。如果未来需要用同一环境变量连 OpenAI 官方，请先切换 `OPENAI_BASE_URL`。
2. 各 VPS 节点 `.env` 中实际激活的 Key 可能不同。以 `auth_profiles_registry.json` 的 `_note` 字段为参考。
