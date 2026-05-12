/**
 * bob-agent — LLM Client
 *
 * 多供应商 LLM 引擎，通过 OpenAI SDK 兼容协议统一调用。
 * 支持 Chat + Vision + 流式输出 + Thinking Token。
 *
 * 供应商优先级：
 *   1. DeepSeek (deepseek-v4-pro / deepseek-v4-flash) — 默认
 *   2. OpenAI (gpt-4.1-mini / gpt-4.1) — 备选
 *   3. Ollama (本地模型) — 离线场景
 *   4. Custom (OpenAI 兼容端点) — 高级用户
 */

const OpenAI = require('openai');

// ─── 供应商配置（pricing 单位：CNY / 1M tokens，来源 model-registry）─────
const PROVIDERS = {
  deepseek: {
    name: 'DeepSeek',
    baseURL: 'https://api.deepseek.com',
    models: [
      { id: 'deepseek-v4-flash', label: 'DeepSeek V4 Flash', vision: true, default: true,
        pricing: { input: 1.0, output: 2.0 } },
      { id: 'deepseek-v4-pro', label: 'DeepSeek V4 Pro', vision: false,
        pricing: { input: 3.0, output: 6.0 } },
    ],
  },
  openai: {
    name: 'OpenAI',
    baseURL: 'https://api.openai.com/v1',
    models: [
      { id: 'gpt-4.1', label: 'GPT-4.1', vision: true,
        pricing: { input: 14.0, output: 56.0 } },
      { id: 'gpt-4.1-mini', label: 'GPT-4.1 Mini', vision: true, default: true,
        pricing: { input: 2.8, output: 11.2 } },
      { id: 'gpt-4.1-nano', label: 'GPT-4.1 Nano', vision: true,
        pricing: { input: 0.7, output: 2.8 } },
    ],
  },
  qwen: {
    name: '通义千问 (Qwen)',
    baseURL: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    models: [
      { id: 'qwen3-max', label: 'Qwen3 Max', vision: false,
        pricing: { input: 2.5, output: 10.0 } },
      { id: 'qwen-plus', label: 'Qwen Plus', vision: true, default: true,
        pricing: { input: 2.0, output: 12.0 } },
      { id: 'qwen3.5', label: 'Qwen 3.5', vision: true,
        pricing: { input: 0.4, output: 3.2 } },
      { id: 'qwen3.5-flash', label: 'Qwen 3.5 Flash', vision: false,
        pricing: { input: 0.2, output: 2.0 } },
    ],
  },
  doubao: {
    name: '豆包 (Doubao)',
    baseURL: 'https://ark.cn-beijing.volces.com/api/v3',
    models: [
      { id: 'doubao-seed-1-6-flash-250828', label: 'Doubao Seed 1.6 Flash', vision: false, default: true,
        pricing: { input: 0.15, output: 1.5 } },
      { id: 'doubao-seed-2-0-mini-260215', label: 'Doubao Seed 2.0 Mini', vision: true,
        pricing: { input: 0.2, output: 2.0 } },
    ],
  },
  zhipu: {
    name: '智谱 AI (GLM)',
    baseURL: 'https://open.bigmodel.cn/api/paas/v4',
    models: [
      { id: 'GLM-5.1', label: 'GLM-5.1', vision: false,
        pricing: { input: 6.0, output: 24.0 } },
      { id: 'GLM-4.7', label: 'GLM-4.7', vision: false, default: true,
        pricing: { input: 2.0, output: 8.0 } },
      { id: 'GLM-4.7-FlashX', label: 'GLM-4.7 FlashX', vision: false,
        pricing: { input: 0.5, output: 3.0 } },
    ],
  },
  kimi: {
    name: 'Kimi (Moonshot)',
    baseURL: 'https://api.moonshot.cn/v1',
    models: [
      { id: 'kimi-k2.5', label: 'Kimi k2.5', vision: true, default: true,
        pricing: { input: 4.0, output: 21.0 } },
    ],
  },
  minimax: {
    name: 'MiniMax',
    baseURL: 'https://api.minimax.io/v1',
    models: [
      { id: 'MiniMax-M2.7', label: 'MiniMax M2.7', vision: false, default: true,
        pricing: { input: 2.1, output: 8.4 } },
    ],
  },
  custom: {
    name: '自定义',
    baseURL: '',
    models: [],
  },
};

class LLMClient {
  /**
   * @param {object} config
   * @param {string} config.provider - 供应商 ID (deepseek/openai/ollama/custom)
   * @param {string} config.apiKey - API Key
   * @param {string} config.model - 模型 ID (可选，为空时用供应商默认)
   * @param {string} config.baseURL - 自定义 baseURL (仅 custom 供应商)
   */
  constructor({ provider = 'deepseek', apiKey = '', model = '', baseURL = '', registry = null } = {}) {
    this.registry = registry;
    this.provider = provider;
    this.apiKey = apiKey;
    this.modelId = model;
    this.customBaseURL = baseURL;
    this._abortController = null;
    this._client = null;

    if (apiKey) {
      this._initClient();
    }
  }

  _initClient() {
    const providerConfig = PROVIDERS[this.provider];
    const base = this.provider === 'custom'
      ? this.customBaseURL
      : providerConfig?.baseURL;

    if (!base) return;

    this._client = new OpenAI({
      apiKey: this.apiKey || 'ollama', // Ollama 不需要 key，但 SDK 要求非空
      baseURL: base,
      dangerouslyAllowBrowser: false,
    });
  }

  /** 检查 LLM 是否已配置可用 */
  isConfigured() {
    if (this.provider === 'ollama') return true; // Ollama 不需要 key
    return !!(this._client && this.apiKey);
  }

  /** 获取当前供应商的可用模型列表 */
  getAvailableModels() {
    const providerConfig = PROVIDERS[this.provider];
    if (!providerConfig) return [];
    return providerConfig.models.map(m => ({
      id: m.id,
      label: m.label,
      vision: m.vision,
      default: m.default || false,
      pricing: m.pricing || null,
    }));
  }

  /** 获取指定模型的价格 (CNY / 1M tokens) */
  getPricing(modelId) {
    const id = modelId || this._getModelId();
    const providerConfig = PROVIDERS[this.provider];
    if (!providerConfig) return null;
    const model = providerConfig.models.find(m => m.id === id);
    return model?.pricing || null;
  }

  /** 获取当前使用的模型 ID */
  _getModelId() {
    if (this.modelId) return this.modelId;
    const providerConfig = PROVIDERS[this.provider];
    if (!providerConfig) return '';
    const defaultModel = providerConfig.models.find(m => m.default) || providerConfig.models[0];
    return defaultModel?.id || '';
  }

  /** 中止当前请求 */
  abort() {
    if (this._abortController) {
      this._abortController.abort();
      this._abortController = null;
    }
  }

  /**
   * 流式对话
   * @param {Array} messages - OpenAI 格式的消息数组
   * @yields {{ type: 'text'|'thinking'|'done'|'error', content: string }}
   */
  async *chatStream(messages, agentMode = 'yolo') {
    if (!this._client) throw new Error('LLM 未初始化');

    this._abortController = new AbortController();
    const modelId = this._getModelId();
    let usageData = null;

    try {
      let currentMessages = [...messages];
      let iterationCount = 0;
      const MAX_ITERATIONS = 5;

      while (iterationCount < MAX_ITERATIONS) {
        iterationCount++;

        const params = {
          model: modelId,
          messages: currentMessages,
          stream: true,
          stream_options: { include_usage: true },
        };

        // 针对 DeepSeek 官方模型的特殊优化 (参考最新 API 文档)
        if (this.provider === 'deepseek') {
          params.thinking = { type: 'enabled' };
          params.reasoning_effort = modelId.includes('pro') || modelId.includes('reasoner') ? 'high' : 'low';
          // DeepSeek thinking 模式必须用 max_completion_tokens（包含思考+输出），不能用 max_tokens
          params.max_completion_tokens = 16384;
        } else {
          params.max_tokens = 8192;
        }

        if (this.registry) {
          let schemas = this.registry.getAllSchemas();
          if (agentMode === 'insight') {
            const readonlyTools = ['list_directory', 'read_file', 'web_search', 'browser_automation', 'link_extractor', 'system_time', 'system_info', 'weather', 'wechat_reader'];
            schemas = schemas.filter(s => readonlyTools.includes(s.function.name));
            console.log(`[LLMClient] Insight Mode: Restricting to read-only tools: ${schemas.map(s => s.function.name).join(', ')}`);
          }
          if (schemas && schemas.length > 0) {
            params.tools = schemas;
            console.log('[LLMClient] Sending tools to API:', schemas.map(s => s.function.name).join(', '));
          }
        }

        const stream = await this._client.chat.completions.create(
          params,
          { signal: this._abortController.signal }
        );

        let toolCallsMap = new Map();
        let currentUsage = null;
        let assistantMessageContent = '';
        let assistantReasoningContent = '';

        for await (const chunk of stream) {
          if (chunk.usage) {
            currentUsage = chunk.usage;
          }

          const delta = chunk.choices?.[0]?.delta;
          if (!delta) continue;

          if (delta.tool_calls) {
            for (const toolCall of delta.tool_calls) {
              if (!toolCallsMap.has(toolCall.index)) {
                toolCallsMap.set(toolCall.index, {
                  id: toolCall.id,
                  type: 'function',
                  function: { name: '', arguments: '' }
                });
              }
              const tc = toolCallsMap.get(toolCall.index);
              if (toolCall.id) tc.id = toolCall.id;
              // name 只 set 不 append（防止重复拼接）
              if (toolCall.function?.name) {
                if (!tc.function.name) tc.function.name = toolCall.function.name;
              }
              if (toolCall.function?.arguments) tc.function.arguments += toolCall.function.arguments;
            }
            continue;
          }

          if (delta.reasoning_content) {
            assistantReasoningContent += delta.reasoning_content;
            yield { type: 'thinking', content: delta.reasoning_content };
          }

          if (delta.content) {
            assistantMessageContent += delta.content;
            yield { type: 'text', content: delta.content };
          }
        }
        
        // 累加所有迭代的 token 消耗
        if (currentUsage) {
          if (!usageData) {
            usageData = { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 };
          }
          usageData.prompt_tokens += currentUsage.prompt_tokens || 0;
          usageData.completion_tokens += currentUsage.completion_tokens || 0;
          usageData.total_tokens += currentUsage.total_tokens || 0;
        }

        if (toolCallsMap.size > 0) {
          const toolCalls = Array.from(toolCallsMap.values());
          console.log('[LLMClient] Tool calls received:', toolCalls.map(tc => `${tc.function.name}(${tc.function.arguments})`).join(', '));
          currentMessages.push({
            role: 'assistant',
            tool_calls: toolCalls,
            content: assistantMessageContent || null,
            ...(assistantReasoningContent ? { reasoning_content: assistantReasoningContent } : {})
          });

          for (const tc of toolCalls) {
            let result = '';
            try {
              const args = JSON.parse(tc.function.arguments || '{}');
              console.log(`[LLMClient] Executing tool: ${tc.function.name}`, args);
              yield { type: 'tool_start', name: tc.function.name, args };
              if (this.registry) {
                const res = await this.registry.executeTool(tc.function.name, args);
                result = typeof res === 'string' ? res : JSON.stringify(res);
                console.log(`[LLMClient] Tool result (first 200 chars):`, result.substring(0, 200));
              } else {
                result = 'ToolRegistry not available.';
              }
            } catch (e) {
              console.error(`[LLMClient] Tool execution error:`, e);
              result = e.toString();
            }
            yield { type: 'tool_end', name: tc.function.name, result: result.substring(0, 500) };
            currentMessages.push({ role: 'tool', tool_call_id: tc.id, content: result });
          }
        } else {
          break;
        }
      }

      yield { type: 'done', content: '', usage: usageData, model: modelId };
    } catch (err) {
      if (err.name === 'AbortError') {
        yield { type: 'done', content: '', usage: usageData, model: modelId };
        return;
      }
      yield { type: 'error', content: this._friendlyError(err) };
    } finally {
      this._abortController = null;
    }
  }

  /**
   * 流式图片识别对话
   * @param {Array} messages - 历史消息
   * @param {string} imageBase64 - Base64 编码的图片数据（不含 data:image 前缀）
   * @yields {{ type: 'text'|'thinking'|'done'|'error', content: string }}
   */
  async *visionStream(messages, imageBase64, agentMode = 'yolo') {
    // 构建带图片的消息
    const lastUserMsg = messages[messages.length - 1];
    const textContent = lastUserMsg?.content || '请分析这张图片';

    const visionMessages = [
      ...messages.slice(0, -1),
      {
        role: 'user',
        content: [
          { type: 'text', text: textContent },
          {
            type: 'image_url',
            image_url: {
              url: `data:image/png;base64,${imageBase64}`,
              detail: 'auto',
            },
          },
        ],
      },
    ];

    yield* this.chatStream(visionMessages, agentMode);
  }

  /**
   * 非流式调用 (给 Parser 等内部模块用)
   * @param {Array} messages
   * @param {object} options - { responseFormat, temperature, maxTokens }
   * @returns {Promise<{content: string}>}
   */
  async chat(messages, options = {}) {
    if (!this._client) throw new Error('LLM 未初始化');

    const params = {
      model: this._getModelId(),
      messages,
      temperature: options.temperature ?? 0.7,
    };

    if (options.responseFormat) {
      params.response_format = options.responseFormat;
    }
    if (options.maxTokens) {
      params.max_tokens = options.maxTokens;
    }

    const response = await this._client.chat.completions.create(params);
    return {
      content: response.choices?.[0]?.message?.content || '',
    };
  }

  /** 将 API 错误转为用户友好消息 */
  _friendlyError(err) {
    const msg = err.message || String(err);
    if (msg.includes('401') || msg.includes('Unauthorized')) {
      return 'API Key 无效，请在设置中检查';
    }
    if (msg.includes('429') || msg.includes('rate')) {
      return '请求太频繁，请稍后再试';
    }
    if (msg.includes('ECONNREFUSED') || msg.includes('ENOTFOUND')) {
      return '无法连接到 AI 服务，请检查网络';
    }
    if (msg.includes('timeout')) {
      return 'AI 响应超时，请重试';
    }
    return `AI 服务异常: ${msg.slice(0, 100)}`;
  }
}

module.exports = { LLMClient, PROVIDERS };
