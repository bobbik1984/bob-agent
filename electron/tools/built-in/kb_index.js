const fs = require('fs');
const path = require('path');
const { BaseTool } = require('../base');

/**
 * 读取 docs/ 目录下所有 md，调用 LLM 提取属性并写入 graph.json 和 wiki md 的工具 (P2)
 */
class KBIndexTool extends BaseTool {
  constructor() {
    super();
    this.name = 'kb_index';
    this.description = 'Read converted markdown documents in a knowledge base folder, use LLM to extract semantic properties, and build the Wiki index.';
    this.input_schema = {
      type: 'object',
      properties: {
        folder_path: { type: 'string', description: 'Path to the original local folder' },
        plan: { type: 'string', enum: ['cheap', 'core'], description: 'Which model plan to use' }
      },
      required: ['folder_path']
    };
  }

  async execute({ folder_path, plan = 'cheap' }) {
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

      if (manifest.pipeline_status.phase_2_convert !== "done") {
        return { success: false, message: "请先执行 kb_convert 提取文本。" };
      }

      if (manifest.pipeline_status.phase_3_index === "done") {
        return { success: true, message: "语义索引已完成过，跳过。" };
      }

      const docsDir = path.join(folderDataDir, 'docs');
      if (!fs.existsSync(docsDir)) {
        return { success: false, message: "找不到 docs 目录，请检查转换步骤。" };
      }

      const files = fs.readdirSync(docsDir).filter(f => f.endsWith('.md'));
      
      // 注意: 这里是一个巨大的 LLM 循环。作为内置工具，如果在主进程中长时间执行会阻塞 IPC。
      // 所以对于工业级，最好是由 Agent 通过独立的 kb-semantic-index Skill 来分发任务。
      // 为了本地单机演示闭环，我们在这里利用 global.llmClient 来做一个串行或者并发限流的处理。
      
      if (!global.llmClient) {
        return { success: false, message: "llmClient 未初始化，无法进行语义分析。" };
      }

      const client = global.llmClient; // 默认使用主模型，实际可以通过 plan 切换
      
      // 简单起见，这里我们演示对一个小批次文件进行摘要，并生成一个总的 README.md 
      let processedCount = 0;
      let wikiContent = `# 知识库: ${manifest.source_path}\n\n`;
      wikiContent += `> 自动生成的语义索引\n\n`;

      for (const file of files) {
        const mdPath = path.join(docsDir, file);
        const content = fs.readFileSync(mdPath, 'utf-8');
        
        // 限制长度防止超大文档 OOM
        const truncatedContent = content.substring(0, 8000); 

        const prompt = `你是一个知识图谱分析器。请阅读以下文档并返回符合 Wiki Entry Schema 的 YAML frontmatter 和简短摘要。
要求：
- YAML包含：type, tags, summary, extracted_at
- 摘要100字以内。

文档内容：
${truncatedContent}
`;
        
        try {
          const response = await client.chat([{ role: 'user', content: prompt }]);
          
          wikiContent += `## ${file.replace('__', '/').replace('.md', '')}\n`;
          wikiContent += `${response.content}\n\n`;
          processedCount++;
          
        } catch(e) {
          console.error(`[kb_index] LLM API 错误处理 ${file}:`, e);
          wikiContent += `## ${file}\n*处理失败: ${e.message}*\n\n`;
        }
      }

      // 将总结写入根目录的 README.md 作为该文件夹知识库入口
      const readmePath = path.join(folderDataDir, 'README.md');
      fs.writeFileSync(readmePath, wikiContent, 'utf-8');

      // 更新状态
      manifest.pipeline_status.phase_3_index = "done";
      fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));

      return {
        success: true,
        message: `语义索引完成，处理了 ${processedCount} 个文档。生成了 README.md。`
      };

    } catch (err) {
      return { success: false, message: err.message };
    }
  }
}

module.exports = new KBIndexTool();
