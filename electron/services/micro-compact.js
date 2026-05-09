/**
 * bob-agent — Micro-compact 上下文截断中间件
 *
 * 移植自 CodeRunner micro_compact.py (66行)
 * 零 LLM 成本：在 API 调用前截断历史中超长的消息
 *
 * TODO: Jules 增加测试
 */

/**
 * 截断上下文中超长的工具返回/助手消息
 *
 * @param {Array<object>} messages - chat messages 列表
 * @param {object} options
 * @param {number} options.threshold - 超过此长度才截断 (默认 8000)
 * @param {number} options.keepHead - 保留开头字符数 (默认 2000)
 * @param {number} options.keepTail - 保留结尾字符数 (默认 2000)
 * @returns {Array<object>} 处理后的 messages (不修改原数组)
 */
function microCompact(messages, { threshold = 8000, keepHead = 2000, keepTail = 2000 } = {}) {
  let truncatedCount = 0;

  const result = messages.map(msg => {
    const content = msg.content || '';
    if (typeof content !== 'string') return msg;

    // 只截断 assistant 的超长回复（保留 user 和 system 完整）
    const shouldTruncate =
      msg.role === 'assistant' && content.length > threshold;

    if (shouldTruncate) {
      const head = content.slice(0, keepHead);
      const tail = content.slice(-keepTail);
      const removed = content.length - keepHead - keepTail;
      truncatedCount++;

      return {
        ...msg,
        content: `${head}\n\n[... micro-compact: truncated ${removed} chars ...]\n\n${tail}`,
      };
    }

    return msg;
  });

  if (truncatedCount > 0) {
    console.log(`[micro-compact] Truncated ${truncatedCount} messages`);
  }

  return result;
}

module.exports = { microCompact };
