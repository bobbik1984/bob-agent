/**
 * bob-agent — Folder Tracker (文件夹语义速读引擎)
 *
 * 当用户把一个文件夹拖入聊天框或在设置里添加时，
 * 该服务会"速读"文件夹内容（读文件名 + 目录结构），
 * 调用廉价 LLM 生成语义摘要，并持久化到 wiki/folders/。
 *
 * 参见 AGENTS.md D-008 Tier 3 长期记忆。
 */

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

// 忽略的目录名（不需要速读的噪音）
const IGNORED_DIRS = new Set([
  'node_modules', '.git', '.svn', '__pycache__', '.vscode',
  '.idea', 'dist', 'build', '.next', 'target', 'vendor',
  '$RECYCLE.BIN', 'System Volume Information', '.DS_Store'
]);

// 感兴趣的文件扩展名（用于标注）
const FILE_CATEGORIES = {
  文档: ['.docx', '.doc', '.pdf', '.txt', '.md', '.rtf', '.odt'],
  表格: ['.xlsx', '.xls', '.csv'],
  演示: ['.pptx', '.ppt', '.key'],
  图片: ['.jpg', '.jpeg', '.png', '.gif', '.bmp', '.svg', '.webp', '.psd', '.ai'],
  视频: ['.mp4', '.avi', '.mov', '.mkv', '.wmv'],
  音频: ['.mp3', '.wav', '.flac', '.aac'],
  压缩包: ['.zip', '.rar', '.7z', '.tar', '.gz'],
};

class FolderTracker {
  constructor(wikiDir, llmClient, db) {
    this.wikiFoldersDir = path.join(wikiDir, 'folders');
    this.llmClient = llmClient;
    this.db = db;

    if (!fs.existsSync(this.wikiFoldersDir)) {
      fs.mkdirSync(this.wikiFoldersDir, { recursive: true });
    }
  }

  /**
   * 扫描文件夹前两层，返回结构化的文件列表
   */
  scanFolder(folderPath, maxDepth = 2) {
    const result = { dirs: [], files: [], stats: {} };

    const walk = (dir, depth) => {
      if (depth > maxDepth) return;
      try {
        const entries = fs.readdirSync(dir, { withFileTypes: true });
        for (const entry of entries) {
          if (entry.name.startsWith('.') && IGNORED_DIRS.has(entry.name)) continue;
          if (IGNORED_DIRS.has(entry.name)) continue;

          const relativePath = path.relative(folderPath, path.join(dir, entry.name));

          if (entry.isDirectory()) {
            result.dirs.push(relativePath + '/');
            walk(path.join(dir, entry.name), depth + 1);
          } else if (entry.isFile()) {
            result.files.push(relativePath);
            // 统计文件类型
            const ext = path.extname(entry.name).toLowerCase();
            for (const [category, exts] of Object.entries(FILE_CATEGORIES)) {
              if (exts.includes(ext)) {
                result.stats[category] = (result.stats[category] || 0) + 1;
              }
            }
          }
        }
      } catch (err) {
        // 权限不足等，静默跳过
      }
    };

    walk(folderPath, 1);
    return result;
  }

  /**
   * 为文件夹生成语义摘要并持久化
   */
  async trackFolder(folderPath) {
    const folderName = path.basename(folderPath);

    // 1. 检查是否已跟踪
    if (this.db.isTrackedFolder(folderPath)) {
      return { success: false, message: `"${folderName}" 已在关注列表中。` };
    }

    // 2. 扫描文件夹结构
    const scan = this.scanFolder(folderPath);
    const totalFiles = scan.files.length;
    const totalDirs = scan.dirs.length;

    if (totalFiles === 0 && totalDirs === 0) {
      return { success: false, message: `"${folderName}" 是一个空文件夹。` };
    }

    // 3. 生成语义摘要
    let summary = '';
    if (this.llmClient) {
      summary = await this._generateSummary(folderName, folderPath, scan);
    } else {
      // 降级：无 LLM 时用统计信息
      summary = this._generateFallbackSummary(folderName, scan);
    }

    // 4. 写入数据库
    const record = this.db.addTrackedFolder(folderName, folderPath);

    // 5. 写入 wiki Markdown
    const wikiContent = this._buildWikiMarkdown(record.id, folderName, folderPath, scan, summary);
    const wikiPath = path.join(this.wikiFoldersDir, `${record.id}.md`);
    fs.writeFileSync(wikiPath, wikiContent, 'utf-8');

    console.log(`[FolderTracker] Tracked "${folderName}" (${totalFiles} files, ${totalDirs} dirs)`);

    return {
      success: true,
      message: summary,
      name: folderName,
      fileCount: totalFiles,
      dirCount: totalDirs,
      stats: scan.stats
    };
  }

  /**
   * 取消跟踪文件夹
   */
  untrackFolder(folderPath) {
    const folderName = path.basename(folderPath);

    // 从数据库获取 ID 以便删除 wiki 文件
    const folders = this.db.getTrackedFolders();
    const target = folders.find(f => f.path === folderPath);

    if (!target) {
      return { success: false, message: `"${folderName}" 不在关注列表中。` };
    }

    // 删除 wiki 文件
    const wikiPath = path.join(this.wikiFoldersDir, `${target.id}.md`);
    try {
      if (fs.existsSync(wikiPath)) fs.unlinkSync(wikiPath);
    } catch (err) {
      console.error(`[FolderTracker] Failed to delete wiki file:`, err);
    }

    // 从数据库移除
    this.db.removeTrackedFolder(folderPath);
    console.log(`[FolderTracker] Untracked "${folderName}"`);

    return { success: true, message: `已将"${folderName}"从关注列表中移除。` };
  }

  /**
   * 调用 LLM 生成语义摘要
   */
  async _generateSummary(folderName, folderPath, scan) {
    // 准备文件列表（截取前 50 个，防止过长）
    const fileList = scan.files.slice(0, 50).join('\n');
    const dirList = scan.dirs.slice(0, 20).join('\n');
    const statsText = Object.entries(scan.stats)
      .map(([k, v]) => `${k}: ${v}个`)
      .join('、');

    const prompt = `用户让我关注了一个本地文件夹。请用不超过100字的中文，概括这个文件夹是关于什么的、包含哪些核心资料。只输出摘要内容，不要加标题。

文件夹名称: ${folderName}
路径: ${folderPath}
文件类型统计: ${statsText || '无特殊类型'}
子目录:
${dirList || '(无子目录)'}
文件列表:
${fileList}`;

    try {
      const response = await this.llmClient.chat([{ role: 'user', content: prompt }]);
      if (response && response.content) {
        return response.content.trim();
      }
    } catch (err) {
      console.error('[FolderTracker] LLM summary failed:', err);
    }

    return this._generateFallbackSummary(folderName, scan);
  }

  /**
   * 无 LLM 时的降级摘要
   */
  _generateFallbackSummary(folderName, scan) {
    const parts = [`文件夹"${folderName}"`];
    if (scan.files.length > 0) parts.push(`包含 ${scan.files.length} 个文件`);
    if (scan.dirs.length > 0) parts.push(`${scan.dirs.length} 个子目录`);

    const statsText = Object.entries(scan.stats)
      .map(([k, v]) => `${k}(${v})`)
      .join('、');
    if (statsText) parts.push(`主要类型：${statsText}`);

    return parts.join('，') + '。';
  }

  /**
   * 组装 Wiki Markdown 文件
   */
  _buildWikiMarkdown(id, folderName, folderPath, scan, summary) {
    const now = new Date().toISOString();
    const statsText = Object.entries(scan.stats)
      .map(([k, v]) => `- ${k}: ${v} 个`)
      .join('\n');

    return [
      '---',
      `folder_id: ${id}`,
      `name: ${folderName}`,
      `path: ${folderPath}`,
      `last_indexed: ${now}`,
      `file_count: ${scan.files.length}`,
      `dir_count: ${scan.dirs.length}`,
      '---',
      `# ${folderName}`,
      '',
      summary,
      '',
      '## 文件统计',
      statsText || '(无可识别的文件类型)',
      '',
      '## 顶层结构',
      ...scan.dirs.slice(0, 15).map(d => `- 📁 ${d}`),
      ...scan.files.slice(0, 20).map(f => `- 📄 ${f}`),
      ''
    ].join('\n');
  }
}

module.exports = { FolderTracker };
