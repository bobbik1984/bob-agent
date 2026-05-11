const { BaseTool } = require('../base');

class SystemTimeTool extends BaseTool {
  constructor() {
    super();
    this.name = 'system_time';
    this.description = '获取当前的系统本地时间、UTC时间、日期和星期。当需要知道"现在几点"或进行任何时间推理前必须使用此工具。';
    this.input_schema = {
      type: 'object',
      properties: {},
      required: []
    };
  }

  async execute() {
    const now = new Date();
    const days = ['星期日', '星期一', '星期二', '星期三', '星期四', '星期五', '星期六'];
    
    return {
      iso_time: now.toISOString(),
      local_time: now.toLocaleString('zh-CN', { timeZoneName: 'short' }),
      weekday: days[now.getDay()],
      timestamp: now.getTime()
    };
  }
}

module.exports = new SystemTimeTool();
