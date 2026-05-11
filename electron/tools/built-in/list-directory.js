const fs = require('fs');
const { BaseTool } = require('../base');

class ListDirectoryTool extends BaseTool {
  constructor() {
    super();
    this.name = 'list_directory';
    this.description = 'Lists all files and directories under the given directory.';
    this.input_schema = {
      type: 'object',
      properties: {
        path: {
          type: 'string',
          description: 'The directory path to list files from.'
        }
      },
      required: ['path']
    };
  }

  async execute({ path }) {
    try {
      if (!fs.existsSync(path)) {
        return `Error: Directory does not exist: ${path}`;
      }
      const stat = fs.statSync(path);
      if (!stat.isDirectory()) {
        return `Error: Path is not a directory: ${path}`;
      }
      const entries = fs.readdirSync(path, { withFileTypes: true });
      const items = entries.map(e => ({
        name: e.name,
        isDirectory: e.isDirectory()
      }));
      return JSON.stringify({ path, items }, null, 2);
    } catch (err) {
      return `Error: ${err.message}`;
    }
  }
}

module.exports = new ListDirectoryTool();
