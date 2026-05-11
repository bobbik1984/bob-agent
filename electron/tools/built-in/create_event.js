const { BaseTool } = require('../base');

/**
 * T-601 / T-604: 使用 BaseTool 的标准工具类示例
 * Jules 需要将其他所有的 built-in 工具都改写成这种格式。
 */
class CreateEventTool extends BaseTool {
  constructor() {
    super();
    this.name = 'create_event';
    this.description = 'Create a new calendar event or to-do item in the local database.';
    this.input_schema = {
      type: 'object',
      properties: {
        type: { type: 'string', enum: ['event', 'todo'], description: 'Whether this is a scheduled event or a floating todo' },
        title: { type: 'string', description: 'The title of the event or todo' },
        start_time: { type: 'string', description: 'Start time in YYYY-MM-DD HH:mm:ss format (optional for todo)' },
        end_time: { type: 'string', description: 'End time in YYYY-MM-DD HH:mm:ss format (optional)' }
      },
      required: ['type', 'title']
    };
  }

  async execute(params) {
    try {
      // 在 electron/main.js 中已经初始化了 db，可以通过全局变量访问，或者使用传参
      // 这里作为示例，假设全局变量 global.db 或由 Registry 注入
      if (!global.db) throw new Error("Database instance not available");
      
      const id = global.db.createEvent(params);
      return `Successfully created ${params.type}: ${params.title} (ID: ${id})`;
    } catch (err) {
      return `Failed to create event: ${err.message}`;
    }
  }
}

module.exports = new CreateEventTool();
