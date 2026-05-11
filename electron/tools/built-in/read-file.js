const fs = require('fs');

module.exports = {
  name: 'read_file',
  description: 'Reads the content of the specified text file.',
  parameters: {
    type: 'object',
    properties: {
      path: {
        type: 'string',
        description: 'The path of the file to read.'
      }
    },
    required: ['path']
  },
  async execute({ path }) {
    try {
      const p = require('path');
      const resolvedPath = p.resolve(path);

      if (!global.securityState?.globalFileAccess) {
        const workspaceDir = global.db?.getConfig('workspaceDir');
        if (!workspaceDir) {
          return `Error: Workspace directory is not set. Cannot read files outside workspace.`;
        }
        const normalizedWorkspace = p.resolve(workspaceDir);
        if (resolvedPath !== normalizedWorkspace && !resolvedPath.startsWith(normalizedWorkspace + p.sep)) {
          return `Error: Access denied. Cannot read files outside workspace unless globalFileAccess is true.`;
        }
      }

      if (!fs.existsSync(resolvedPath)) {
        return `Error: File does not exist: ${resolvedPath}`;
      }
      const stat = fs.statSync(resolvedPath);
      if (!stat.isFile()) {
        return `Error: Path is not a file: ${resolvedPath}`;
      }
      const content = fs.readFileSync(resolvedPath, { encoding: 'utf-8' });
      return content;
    } catch (err) {
      return `Error: ${err.message}`;
    }
  }
};
