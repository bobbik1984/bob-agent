const { BaseTool } = require('../base');
const path = require('path');

class BrainSearchTool extends BaseTool {
  constructor() {
    super();
    this.name = 'brain_search';
    this.description = '搜索你的长期记忆（历史对话总结）和知识库（项目 Wiki）。当用户提到你不记得的历史事件、过去的项目、或需要回顾旧决策时，主动调用此工具。';
    this.input_schema = {
      type: 'object',
      properties: {
        query: {
          type: 'string',
          description: '搜索关键词，例如项目名、技术栈、日期、事件描述等'
        },
        scope: {
          type: 'string',
          enum: ['memory', 'wiki', 'all'],
          description: '搜索范围：memory=历史对话记忆, wiki=项目知识库, all=全部。默认 all。'
        }
      },
      required: ['query']
    };
  }

  async execute(params) {
    try {
      if (!global.memoryEngine) {
        return 'MemoryEngine 未初始化，无法搜索记忆。';
      }

      const { query, scope = 'all' } = params;
      const results = global.memoryEngine.searchBrain(query, scope);

      if (results.length === 0) {
        return `在${scope === 'all' ? '记忆和知识库' : scope === 'memory' ? '历史对话' : '知识库'}中没有找到与"${query}"相关的内容。`;
      }

      let output = `找到 ${results.length} 条相关记录：\n\n`;
      for (const r of results) {
        output += `📁 [${r.source}] ${r.title}\n`;
        output += `${r.excerpt}\n\n`;
      }
      return output;
    } catch (err) {
      return `搜索失败: ${err.message}`;
    }
  }
}

module.exports = new BrainSearchTool();
