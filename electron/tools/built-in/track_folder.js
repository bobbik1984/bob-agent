const { BaseTool } = require('../base');

class TrackFolderTool extends BaseTool {
  constructor() {
    super();
    this.name = 'track_folder';
    this.description = '将一个本地文件夹添加到你的长期关注列表。你会扫描文件夹内容并生成摘要，以便将来快速回忆。当用户拖入文件夹、或口头说"帮我记住/关注这个目录"时调用此工具。';
    this.input_schema = {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: '要关注的本地文件夹绝对路径，例如 D:\\Work\\年会筹备'
        }
      },
      required: ['path']
    };
  }

  async execute(params) {
    try {
      if (!global.folderTracker) {
        return '文件夹跟踪服务未初始化。';
      }

      const fs = require('fs');
      const folderPath = params.path;

      // 验证路径存在且是目录
      if (!fs.existsSync(folderPath)) {
        return `路径不存在: ${folderPath}`;
      }
      const stat = fs.statSync(folderPath);
      if (!stat.isDirectory()) {
        return `这不是一个文件夹: ${folderPath}`;
      }

      const result = await global.folderTracker.trackFolder(folderPath);

      if (!result.success) {
        return result.message;
      }

      let output = `✅ 已将"${result.name}"加入关注列表。\n\n`;
      output += `📊 概况：${result.fileCount} 个文件，${result.dirCount} 个子目录\n`;
      if (Object.keys(result.stats).length > 0) {
        output += `📂 内容类型：${Object.entries(result.stats).map(([k, v]) => `${k}(${v})`).join('、')}\n`;
      }
      output += `\n📝 我的理解：${result.message}`;
      return output;
    } catch (err) {
      return `关注文件夹失败: ${err.message}`;
    }
  }
}

module.exports = new TrackFolderTool();
