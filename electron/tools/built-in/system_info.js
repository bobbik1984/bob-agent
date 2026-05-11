const os = require('os');
const { BaseTool } = require('../base');

class SystemInfoTool extends BaseTool {
  constructor() {
    super();
    this.name = 'system_info';
    this.description = '获取当前操作系统的基础硬件、内存、平台和 Node.js 运行环境信息。';
    this.input_schema = {
      type: 'object',
      properties: {},
      required: []
    };
  }

  async execute() {
    const totalMem = (os.totalmem() / (1024 ** 3)).toFixed(2) + ' GB';
    const freeMem = (os.freemem() / (1024 ** 3)).toFixed(2) + ' GB';
    const cpus = os.cpus();
    const cpuModel = cpus.length > 0 ? cpus[0].model : 'Unknown';
    
    return {
      platform: os.platform(),
      release: os.release(),
      arch: os.arch(),
      hostname: os.hostname(),
      memory: {
        total: totalMem,
        free: freeMem,
        usage_percent: ((1 - os.freemem() / os.totalmem()) * 100).toFixed(1) + '%'
      },
      cpu: {
        model: cpuModel,
        cores: cpus.length
      },
      node_version: process.version,
      uptime_seconds: os.uptime()
    };
  }
}

module.exports = new SystemInfoTool();
