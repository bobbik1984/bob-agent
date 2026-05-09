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
      if (!fs.existsSync(path)) {
        return `Error: File does not exist: ${path}`;
      }
      const stat = fs.statSync(path);
      if (!stat.isFile()) {
        return `Error: Path is not a file: ${path}`;
      }
      const content = fs.readFileSync(path, { encoding: 'utf-8' });
      return content;
    } catch (err) {
      return `Error: ${err.message}`;
    }
  }
};
