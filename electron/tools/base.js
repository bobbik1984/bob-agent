/**
 * BaseTool - Agent 工具的基础类
 * 
 * 所有的 Agent 技能工具都必须继承此类并实现 execute() 方法。
 */
class BaseTool {
  constructor() {
    this.name = 'unnamed_tool';
    this.description = 'No description provided';
    this.input_schema = {
      type: 'object',
      properties: {},
      required: []
    };
  }

  /**
   * 返回 OpenAI Function Calling 标准格式的 Schema
   */
  getSchema() {
    return {
      type: 'function',
      function: {
        name: this.name,
        description: this.description,
        parameters: this.input_schema
      }
    };
  }

  /**
   * 工具的实际执行逻辑
   * @param {Object} params - 工具执行参数
   * @returns {Promise<any>}
   */
  async execute(params) {
    throw new Error('BaseTool: execute() method must be implemented by subclass');
  }
}

module.exports = { BaseTool };
