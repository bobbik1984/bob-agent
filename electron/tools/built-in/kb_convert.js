const fs = require('fs');
const path = require('path');
const { BaseTool } = require('../base');
// 重用现有的文件读取逻辑，它内部已经包含了 mammoth, pdf-parse 等支持
const { readFile } = require('../../services/file-reader');

/**
 * 遍历 manifest，将文档批量转换为 Markdown 的内部工具 (P1)
 */
class KBConvertTool extends BaseTool {
  constructor() {
    super();
    this.name = 'kb_convert';
    this.description = 'Convert documents in a knowledge base folder to Markdown based on its manifest';
    this.input_schema = {
      type: 'object',
      properties: {
        folder_path: { type: 'string', description: 'Path to the local folder' }
      },
      required: ['folder_path']
    };
  }

  async execute({ folder_path }) {
    try {
      const appData = process.env.APPDATA || (process.platform === 'darwin' ? process.env.HOME + '/Library/Application Support' : process.env.HOME + '/.config');
      const wikiFoldersDir = path.join(appData, 'bob-agent', 'data', 'wiki', 'folders');
      
      let manifestPath = null;
      let manifest = null;
      let folderDataDir = null;

      if (fs.existsSync(wikiFoldersDir)) {
        const dirs = fs.readdirSync(wikiFoldersDir);
        for (const dir of dirs) {
          const testManifestPath = path.join(wikiFoldersDir, dir, 'manifest.json');
          if (fs.existsSync(testManifestPath)) {
            const data = JSON.parse(fs.readFileSync(testManifestPath, 'utf-8'));
            if (data.source_path === folder_path) {
              manifestPath = testManifestPath;
              manifest = data;
              folderDataDir = path.join(wikiFoldersDir, dir);
              break;
            }
          }
        }
      }

      if (!manifest) {
        return { success: false, message: "找不到该文件夹的 manifest.json，请先执行收藏 (Phase 0)。" };
      }

      // 检查状态
      if (manifest.pipeline_status.phase_2_convert === "done") {
        return { success: true, message: "文档转换已完成过，跳过。" };
      }

      const docsDir = path.join(folderDataDir, 'docs');
      if (!fs.existsSync(docsDir)) {
        fs.mkdirSync(docsDir, { recursive: true });
      }

      const convertableTypes = ['文档', '表格', '演示'];
      const plainTextExts = ['.txt', '.md', '.json', '.js', '.py', '.html', '.css', '.vue', '.yaml', '.yml'];
      
      let successCount = 0;
      let failCount = 0;

      for (const file of manifest.files) {
        if (!convertableTypes.includes(file.category) && !plainTextExts.includes(file.ext)) {
           continue; // Skip images, videos, archives, etc.
        }

        const absPath = file.absolute_path;
        if (!fs.existsSync(absPath)) continue;

        // 生成扁平化的安全文件名：将目录的斜杠替换为双下划线
        const safeName = file.relative_path.replace(/[\/\\]/g, '__') + '.md';
        const outPath = path.join(docsDir, safeName);

        try {
          const res = await readFile(absPath);
          if (res.error) {
            failCount++;
            continue;
          }

          const mdContent = `---
original_path: ${file.relative_path}
size_bytes: ${file.size_bytes}
category: ${file.category}
converted_at: ${new Date().toISOString()}
---

# ${path.basename(file.relative_path)}

${res.content}
`;
          fs.writeFileSync(outPath, mdContent, 'utf-8');
          successCount++;
        } catch (err) {
          console.error(`[kb_convert] Failed to convert ${absPath}:`, err);
          failCount++;
        }
      }

      // 更新状态
      manifest.pipeline_status.phase_2_convert = "done";
      fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));

      return {
        success: true,
        message: `转换完成: 成功 ${successCount} 个, 失败 ${failCount} 个.`,
        docs_dir: docsDir,
        success_count: successCount,
        fail_count: failCount
      };

    } catch (err) {
      return { success: false, message: err.message };
    }
  }
}

module.exports = new KBConvertTool();
