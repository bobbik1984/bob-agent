const fs = require('fs');
const path = require('path');

module.exports = {
  name: 'write_file',
  description: '将文本内容写入到本地文件中（如果文件存在则覆盖，如果目录不存在会自动创建）。当你需要保存剪报、生成 HTML 报告、或输出任何代码文件时使用。',
  parameters: {
    type: 'object',
    properties: {
      absolutePath: {
        type: 'string',
        description: '文件的绝对路径'
      },
      content: {
        type: 'string',
        description: '要写入的文件完整内容'
      }
    },
    required: ['absolutePath', 'content']
  },
  execute: async (args) => {
    try {
      const resolvedPath = path.resolve(args.absolutePath);

      if (!global.securityState?.globalFileAccess) {
        const workspaceDir = global.db?.getConfig('workspaceDir');
        if (!workspaceDir) {
          return `Error: Workspace directory is not set. Cannot write files outside workspace.`;
        }
        const normalizedWorkspace = path.resolve(workspaceDir);
        if (resolvedPath !== normalizedWorkspace && !resolvedPath.startsWith(normalizedWorkspace + path.sep)) {
          return `Error: Access denied. Cannot write files outside workspace unless globalFileAccess is true.`;
        }
      }

      const dir = path.dirname(resolvedPath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      fs.writeFileSync(resolvedPath, args.content, 'utf-8');
      return `成功！文件已成功写入到: ${resolvedPath}`;
    } catch (err) {
      return `写入文件失败: ${err.message}`;
    }
  }
};
