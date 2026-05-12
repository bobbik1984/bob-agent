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
   * 扫描文件夹，递归深度受 maxDepth 限制。
   * 收集文件的相对路径、绝对路径、大小、修改时间、扩展名等。
   */
  scanFolder(folderPath, maxDepth = 4) {
    const result = { dirs: [], files: [], stats: {}, totalSize: 0 };

    const walk = (dir, depth) => {
      if (depth > maxDepth) return;
      try {
        const entries = fs.readdirSync(dir, { withFileTypes: true });
        for (const entry of entries) {
          if (entry.name.startsWith('.') && IGNORED_DIRS.has(entry.name)) continue;
          if (IGNORED_DIRS.has(entry.name)) continue;

          const absolutePath = path.join(dir, entry.name);
          const relativePath = path.relative(folderPath, absolutePath);

          if (entry.isDirectory()) {
            result.dirs.push(relativePath + '/');
            walk(absolutePath, depth + 1);
          } else if (entry.isFile()) {
            try {
              const fileStats = fs.statSync(absolutePath);
              const ext = path.extname(entry.name).toLowerCase();
              let matchedCategory = '其他';
              
              for (const [category, exts] of Object.entries(FILE_CATEGORIES)) {
                if (exts.includes(ext)) {
                  result.stats[category] = (result.stats[category] || 0) + 1;
                  matchedCategory = category;
                  break;
                }
              }

              result.files.push({
                relative_path: relativePath.replace(/\\/g, '/'),
                absolute_path: absolutePath,
                size_bytes: fileStats.size,
                ext: ext,
                category: matchedCategory,
                modified_at: fileStats.mtime.toISOString()
              });
              result.totalSize += fileStats.size;
            } catch (err) {
              // Ignore stats errors
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
   * 收藏文件夹，生成标准化 manifest.json (无 LLM 成本)
   */
  async trackFolder(folderPath) {
    const folderName = path.basename(folderPath);

    // 1. 父子目录互斥检测
    const existing = this.db.getTrackedFolders();
    const parent = existing.find(f => folderPath.startsWith(f.path + path.sep));
    if (parent) {
      return { success: false, message: `父目录「${parent.name}」已在关注列表中，无需重复收藏。` };
    }

    if (this.db.isTrackedFolder(folderPath)) {
      return { success: false, message: `"${folderName}" 已在关注列表中。` };
    }

    // 2. 扫描文件夹结构
    const scan = this.scanFolder(folderPath, 4);
    const totalFiles = scan.files.length;
    const totalDirs = scan.dirs.length;

    if (totalFiles === 0 && totalDirs === 0) {
      return { success: false, message: `"${folderName}" 是一个空文件夹。` };
    }

    // 3. 处理已存在的子目录 (吸收合并)
    const children = existing.filter(f => f.path.startsWith(folderPath + path.sep));
    for (const child of children) {
      console.log(`[FolderTracker] Absorbing child folder: ${child.path}`);
      // 以后可以在这里将子目录的 docs 迁移到父目录，目前仅删除 DB 记录和旧的维基文件夹
      this.untrackFolder(child.path);
    }

    // 4. 写入数据库
    const record = this.db.addTrackedFolder(folderName, folderPath);

    // 5. 生成并写入 manifest.json (程序接口) 和 tree.md (LLM接口)
    const folderDataDir = path.join(this.wikiFoldersDir, record.id);
    if (!fs.existsSync(folderDataDir)) {
      fs.mkdirSync(folderDataDir, { recursive: true });
    }

    const manifestContent = this._buildManifest(record.id, folderName, folderPath, scan);
    fs.writeFileSync(path.join(folderDataDir, 'manifest.json'), JSON.stringify(manifestContent, null, 2), 'utf-8');

    const treeContent = this._buildTree(folderName, folderPath, scan);
    fs.writeFileSync(path.join(folderDataDir, 'tree.md'), treeContent, 'utf-8');

    // 兼容性保留: 在上级写一个 README.md，方便直接用 brain_search
    fs.writeFileSync(path.join(this.wikiFoldersDir, `${record.id}.md`), treeContent, 'utf-8');

    console.log(`[FolderTracker] Tracked "${folderName}" (${totalFiles} files, ${totalDirs} dirs) [ID: ${record.id}]`);

    return {
      success: true,
      message: `已收藏「${folderName}」。包含 ${totalFiles} 个文件。`,
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

    // 从数据库获取 ID 以便删除目录
    const folders = this.db.getTrackedFolders();
    const target = folders.find(f => f.path === folderPath);

    if (!target) {
      return { success: false, message: `"${folderName}" 不在关注列表中。` };
    }

    // 删除兼容性的 wiki 文件
    const wikiPath = path.join(this.wikiFoldersDir, `${target.id}.md`);
    try {
      if (fs.existsSync(wikiPath)) fs.unlinkSync(wikiPath);
    } catch (err) {}

    // 删除存储了 manifest 的专属文件夹
    const folderDataDir = path.join(this.wikiFoldersDir, target.id);
    try {
      if (fs.existsSync(folderDataDir)) {
        fs.rmSync(folderDataDir, { recursive: true, force: true });
      }
    } catch (err) {
      console.error(`[FolderTracker] Failed to delete folder data dir:`, err);
    }

    // 从数据库移除
    this.db.removeTrackedFolder(folderPath);
    console.log(`[FolderTracker] Untracked "${folderName}"`);

    return { success: true, message: `已将"${folderName}"从关注列表中移除。` };
  }

  /**
   * 构建程序读取的标准清单 manifest.json
   */
  _buildManifest(id, folderName, folderPath, scan) {
    return {
      version: 1,
      folder_id: id,
      name: folderName,
      source_path: folderPath,
      created_at: new Date().toISOString(),
      summary: {
        total_files: scan.files.length,
        total_dirs: scan.dirs.length,
        total_size_bytes: scan.totalSize,
        by_category: scan.stats
      },
      files: scan.files,
      dirs: scan.dirs,
      pipeline_status: {
        phase_0_scan: "done",
        phase_1_estimate: null,
        phase_2_convert: null,
        phase_3_index: null
      }
    };
  }

  /**
   * 构建 LLM 和人类渐进式阅读的分层目录树 tree.md
   */
  _buildTree(folderName, folderPath, scan) {
    const formatSize = (bytes) => {
      if (bytes < 1024) return bytes + 'B';
      if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + 'KB';
      return (bytes / (1024 * 1024)).toFixed(1) + 'MB';
    };

    const statsText = Object.entries(scan.stats)
      .map(([k, v]) => `${k} ${v}`)
      .join(', ');

    let content = `# ${folderName}\n`;
    content += `> ${scan.files.length} 个文件 · ${scan.dirs.length} 个子目录 · 总计 ${formatSize(scan.totalSize)}\n`;
    content += `> 路径: ${folderPath}\n\n`;

    content += `## 目录概览\n`;
    
    // Group files by top-level directory
    const grouped = { '/': [] };
    for (const dir of scan.dirs) {
      const topLevel = dir.split('/')[0] + '/';
      if (!grouped[topLevel]) grouped[topLevel] = [];
    }

    for (const file of scan.files) {
      const parts = file.relative_path.split('/');
      if (parts.length === 1) {
        grouped['/'].push(file);
      } else {
        const topLevel = parts[0] + '/';
        if (grouped[topLevel]) {
          grouped[topLevel].push(file);
        } else {
            // Should be handled by dir scan, but just in case
            grouped[topLevel] = [file];
        }
      }
    }

    // Write overview
    for (const [dir, files] of Object.entries(grouped)) {
      if (dir === '/') {
        if (files.length > 0) content += `- 根目录 — ${files.length} 个文件\n`;
      } else {
        const docCount = files.filter(f => f.category === '文档').length;
        const tableCount = files.filter(f => f.category === '表格').length;
        const types = [];
        if (docCount) types.push(`文档${docCount}`);
        if (tableCount) types.push(`表格${tableCount}`);
        const typeStr = types.length > 0 ? ` (${types.join(', ')})` : '';
        content += `- 📁 ${dir} — ${files.length} 个文件${typeStr}\n`;
      }
    }
    content += `\n`;

    // Write detailed lists for top-level sections (limit to avoid massive files)
    let sectionsWritten = 0;
    for (const [dir, files] of Object.entries(grouped)) {
      if (files.length === 0) continue;
      if (sectionsWritten >= 10) break; // Limit to 10 sections
      
      content += `### ${dir === '/' ? '根目录' : dir}\n`;
      
      // Limit to 20 files per section
      for (const file of files.slice(0, 20)) {
        const icon = file.category === '表格' ? '📊' : 
                     file.category === '演示' ? '📽️' : 
                     file.category === '图片' ? '🖼️' : 
                     file.category === '文档' ? '📄' : '📝';
        const basename = file.relative_path.split('/').pop();
        content += `- ${icon} ${basename} (${formatSize(file.size_bytes)})\n`;
      }
      if (files.length > 20) {
        content += `- ... 以及其他 ${files.length - 20} 个文件\n`;
      }
      content += `\n`;
      sectionsWritten++;
    }

    return content;
  }
}

module.exports = { FolderTracker };

