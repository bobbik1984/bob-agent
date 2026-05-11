const { BaseTool } = require('../base');

class UntrackFolderTool extends BaseTool {
  constructor() {
    super();
    this.name = 'untrack_folder';
    this.description = '将一个文件夹从你的长期关注列表中移除。当用户说"不要再关注这个目录了"或"取消跟踪"时调用此工具。';
    this.input_schema = {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: '要取消关注的本地文件夹绝对路径'
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

      const result = global.folderTracker.untrackFolder(params.path);
      return result.message;
    } catch (err) {
      return `取消关注失败: ${err.message}`;
    }
  }
}

module.exports = new UntrackFolderTool();
