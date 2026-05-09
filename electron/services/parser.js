/**
 * bob-agent — Parser (自然语言 → 结构化事件)
 *
 * 移植自 TodoList parser.py (126行)
 * 核心资产：System Prompt 必须与 TodoList 保持同步
 *
 * TODO: Jules 完成完整实现
 */

const PARSER_SYSTEM_PROMPT = `你是一个精准的日程解析助手。用户会发给你一段自然语言文字，你需要从中提取日程事件信息。

## 输出规则
1. 必须输出**严格的 JSON**，不要添加任何 markdown 标记或解释文字。
2. 所有时间使用 ISO 8601 格式，时区默认为 Asia/Shanghai (+08:00)。
3. 如果用户未指定结束时间，默认为开始时间后 1 小时。
4. 如果用户描述的是一个没有明确时间的待办任务，type 设为 "todo"。
5. 如果完全无法理解用户意图，type 设为 "unknown"。

## 输出 Schema
{
  "type": "event" | "todo" | "reminder",
  "confidence": 0.0-1.0,
  "title": "简洁的事件标题",
  "start_time": "2026-05-10T14:00:00+08:00",
  "end_time": "2026-05-10T15:00:00+08:00",
  "location": "地点（如果提到）",
  "description": "补充描述",
  "participants": ["参与者列表"],
  "priority": "low" | "medium" | "high",
  "tags": ["标签"],
  "message": null
}

如果 type 为 "unknown":
{
  "type": "unknown",
  "confidence": 0.0,
  "title": "",
  "message": "无法理解的原因描述"
}

## 当前日期时间
{current_datetime}`;

/**
 * 解析自然语言为结构化事件
 * @param {string} text - 用户输入的自然语言
 * @param {import('./llm-client').LLMClient} llmClient - LLM 客户端
 * @returns {Promise<object>} 解析结果
 */
async function parseNaturalLanguage(text, llmClient) {
  const now = new Date();
  const weekdays = ['日', '一', '二', '三', '四', '五', '六'];
  const currentDatetime = `${now.toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' })} 星期${weekdays[now.getDay()]} Asia/Shanghai`;

  const systemMsg = PARSER_SYSTEM_PROMPT.replace('{current_datetime}', currentDatetime);

  try {
    const response = await llmClient.chat(
      [
        { role: 'system', content: systemMsg },
        { role: 'user', content: text },
      ],
      {
        responseFormat: { type: 'json_object' },
        temperature: 0.1,
        maxTokens: 1024,
      }
    );

    const parsed = JSON.parse(response.content);

    // 补全缺失的 end_time
    if (parsed.start_time && !parsed.end_time) {
      const start = new Date(parsed.start_time);
      start.setHours(start.getHours() + 1);
      parsed.end_time = start.toISOString();
    }

    parsed.raw_input = text;
    return parsed;
  } catch (err) {
    console.error('[Parser] Failed to parse:', err.message);
    return {
      type: 'unknown',
      confidence: 0.0,
      title: '',
      raw_input: text,
      message: 'AI 解析失败，请尝试更明确的描述',
    };
  }
}

module.exports = { parseNaturalLanguage, PARSER_SYSTEM_PROMPT };
