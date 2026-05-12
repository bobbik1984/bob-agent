const fs = require('fs');
const path = require('path');
const { BaseTool } = require('../base');

/**
 * 转换文件为Markdown并预估Token和费用的内部工具 (P1)
 */
class KBEstimateTool extends BaseTool {
  constructor() {
    super();
    this.name = 'kb_estimate';
    this.description = 'Evaluate a knowledge base folder manifest and estimate document conversion cost';
    this.input_schema = {
      type: 'object',
      properties: {
        folder_path: { type: 'string', description: 'Path to the local folder' }
      },
      required: ['folder_path']
    };

    // 假设常见的模型单价 (人民币)
    // 便宜模型 (例如 glm-4-flash / doubao-lite)
    this.cheapModelPricePer1M = 1.0; 
    // 贵价核心模型 (例如 gpt-4o / claude-3-5)
    this.coreModelPricePer1M = 30.0;
  }

  async execute({ folder_path }) {
    try {
      // 1. 在 folder_path 寻找对应或者在 wiki_folders 中寻找对应的 manifest
      // 为了简单起见，假设用户传来的是实际的原始 folder_path
      // 我们需要通过 DB 或者配置系统反查其 manifest
      // 这里作为内部工具，由于它目前并没有依赖 DB，我们在实际管线中可能通过 folder_id 传递会更好
      // 暂且我们通过直接在 wikiFolders 里扫描匹配 source_path 来寻找
      
      const appData = process.env.APPDATA || (process.platform === 'darwin' ? process.env.HOME + '/Library/Application Support' : process.env.HOME + '/.config');
      const wikiFoldersDir = path.join(appData, 'bob-agent', 'data', 'wiki', 'folders');
      
      let manifestPath = null;
      let manifest = null;

      if (fs.existsSync(wikiFoldersDir)) {
        const dirs = fs.readdirSync(wikiFoldersDir);
        for (const dir of dirs) {
          const testManifestPath = path.join(wikiFoldersDir, dir, 'manifest.json');
          if (fs.existsSync(testManifestPath)) {
            const data = JSON.parse(fs.readFileSync(testManifestPath, 'utf-8'));
            if (data.source_path === folder_path) {
              manifestPath = testManifestPath;
              manifest = data;
              break;
            }
          }
        }
      }

      if (!manifest) {
        return { success: false, message: "找不到该文件夹的 manifest.json，请先执行收藏 (Phase 0)。" };
      }

      // 2. 筛选可以转换的文件
      const convertableTypes = ['文档', '表格', '演示']; // 文本提取为主
      const plainTextExts = ['.txt', '.md', '.json', '.js', '.py', '.html', '.css', '.vue', '.yaml', '.yml'];
      
      let convertableCount = 0;
      let convertableBytes = 0;

      for (const file of manifest.files) {
        if (convertableTypes.includes(file.category) || plainTextExts.includes(file.ext)) {
           convertableCount++;
           convertableBytes += file.size_bytes;
        }
      }

      // 3. 粗略预估
      // 经验法则: 对于中文/代码混合，1 Token 约等于 2-3 Bytes (取平均 2.5 Bytes/Token)
      const estimatedTokens = Math.ceil(convertableBytes / 2.5);
      
      const costCheap = (estimatedTokens / 1_000_000) * this.cheapModelPricePer1M;
      const costCore = (estimatedTokens / 1_000_000) * this.coreModelPricePer1M;

      // 4. 更新状态
      manifest.pipeline_status.phase_1_estimate = "done";
      fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));

      return {
        success: true,
        manifest_path: manifestPath,
        convertable_files: convertableCount,
        convertable_bytes: convertableBytes,
        estimated_tokens: estimatedTokens,
        estimated_cost_cheap_rmb: costCheap,
        estimated_cost_core_rmb: costCore
      };

    } catch (err) {
      return { success: false, message: err.message };
    }
  }
}

module.exports = new KBEstimateTool();
