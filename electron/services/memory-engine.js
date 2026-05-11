/**
 * bob-agent — Memory Engine
 *
 * 三层记忆引擎：
 * - Tier 1: SOUL.md（静态人格，每次注入）
 * - Tier 2: memory/sessions/（≤7天，自动压缩注入）
 * - Tier 3: wiki/（>7天 + 项目知识，工具检索）
 *
 * 参见 AGENTS.md D-008 了解完整架构规范。
 */

const fs = require('fs');
const path = require('path');

const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000;

class MemoryEngine {
  constructor(baseDir, llmClient, db) {
    this.baseDir = baseDir;
    this.memoryDir = path.join(baseDir, 'data', 'memory');
    this.sessionsDir = path.join(this.memoryDir, 'sessions');
    this.wikiDir = path.join(baseDir, 'data', 'wiki');
    this.wikiSessionsDir = path.join(this.wikiDir, 'sessions');
    this.soulPath = path.join(this.memoryDir, 'SOUL.md');
    this.llmClient = llmClient;
    this.db = db;

    // 防止并发总结冲突
    this._summarizing = new Set();

    this._initDirs();
  }

  _initDirs() {
    for (const dir of [this.sessionsDir, this.wikiSessionsDir]) {
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
    }
    if (!fs.existsSync(this.soulPath)) {
      const defaultSoul = [
        '# SOUL',
        '',
        '**身份设定**：',
        '你的名字是 bob-agent，你是我的全栈私人 AI 桌面助理。',
        '',
        '**核心纪律**：',
        '- 保持专业、简洁、有帮助。永远使用用户的语言（中文）回答。',
        '- 你具备长期的记忆能力和使用内置工具解决复杂问题的能力。',
        '',
        '**当前偏好**：',
        '- (这里可以手动添加你个人的全局偏好)',
        ''
      ].join('\n');
      fs.writeFileSync(this.soulPath, defaultSoul, 'utf-8');
    }
  }

  // ── Tier 1: 读取 SOUL ──────────────────────────────

  getSoul() {
    try {
      return fs.readFileSync(this.soulPath, 'utf-8');
    } catch (err) {
      console.error('[MemoryEngine] Failed to read SOUL.md:', err);
      return '';
    }
  }

  // ── Tier 2: 读取近期 Session（≤7天）──────────────────

  getRecentSessions(days = 7, limit = 5) {
    try {
      const now = Date.now();
      const cutoff = now - days * 24 * 60 * 60 * 1000;

      const files = fs.readdirSync(this.sessionsDir)
        .filter(f => f.endsWith('.md'))
        .map(f => {
          const filePath = path.join(this.sessionsDir, f);
          const stat = fs.statSync(filePath);
          return { file: f, path: filePath, mtime: stat.mtime.getTime() };
        })
        .filter(f => f.mtime >= cutoff)
        .sort((a, b) => b.mtime - a.mtime)
        .slice(0, limit);

      if (files.length === 0) return '';

      let recentContext = '';
      for (const f of files) {
        const content = fs.readFileSync(f.path, 'utf-8');
        // 跳过元数据头，只取正文
        const body = this._stripFrontmatter(content);
        if (body.trim()) {
          recentContext += `\n--- ${this._extractTitle(content) || f.file} ---\n${body.trim()}\n`;
        }
      }
      return recentContext;
    } catch (err) {
      console.error('[MemoryEngine] Failed to read recent sessions:', err);
      return '';
    }
  }

  // ── Session 总结（切换对话时触发）──────────────────

  async summarizeSession(conversationId) {
    if (!this.db || !this.llmClient) return false;
    if (this._summarizing.has(conversationId)) return false; // 防并发

    this._summarizing.add(conversationId);
    try {
      const messages = this.db.getMessages(conversationId);
      if (messages.length < 3) return false;

      // 获取对话标题
      const conversations = this.db.getConversations();
      const conv = conversations.find(c => c.id === conversationId);
      const title = conv?.title || '未命名对话';
      const createdDate = conv?.created_at ? conv.created_at.substring(0, 10) : new Date().toISOString().substring(0, 10);

      // 截取消息：前5条 + 后20条（防超长对话撑爆上下文）
      let selectedMsgs = messages;
      if (messages.length > 25) {
        selectedMsgs = [...messages.slice(0, 5), ...messages.slice(-20)];
      }

      const chatLog = selectedMsgs
        .map(m => `[${m.role}]: ${m.content.substring(0, 500)}`)
        .join('\n');

      const prompt = `请用不超过100字的中文，客观总结以下对话的核心内容。重点记录：做了什么决定、遇到了什么阻碍、下一步计划是什么。不需要记录闲聊和问候。如果对话没有实质内容，回复"无实质内容"。\n\n对话记录：\n${chatLog}`;

      const summary = await this.llmClient.chat([{ role: 'user', content: prompt }]);
      if (summary && summary.content) {
        const summaryContent = summary.content.trim();
        if (summaryContent === '无实质内容') return false;

        // 写入带元数据头的 Markdown
        const mdContent = [
          '---',
          `conversation_id: ${conversationId}`,
          `title: ${title}`,
          `created: ${createdDate}`,
          '---',
          summaryContent,
          ''
        ].join('\n');

        const summaryPath = path.join(this.sessionsDir, `${conversationId}.md`);
        fs.writeFileSync(summaryPath, mdContent, 'utf-8');
        console.log(`[MemoryEngine] Summarized session "${title}" (${conversationId})`);
        return true;
      }
      return false;
    } catch (err) {
      console.error('[MemoryEngine] Failed to summarize session:', err);
      return false;
    } finally {
      this._summarizing.delete(conversationId);
    }
  }

  // ── 启动时补偿扫描 ────────────────────────────────

  async compensateOnStartup() {
    if (!this.db || !this.llmClient) return;

    try {
      const conversations = this.db.getConversations();
      let compensated = 0;

      for (const conv of conversations) {
        const sessionFile = path.join(this.sessionsDir, `${conv.id}.md`);
        const wikiFile = path.join(this.wikiSessionsDir, `${conv.id}.md`);

        // 如果两个位置都没有总结文件，需要补偿
        if (!fs.existsSync(sessionFile) && !fs.existsSync(wikiFile)) {
          const messages = this.db.getMessages(conv.id);
          if (messages.length >= 3) {
            console.log(`[MemoryEngine] Compensating unsummarized session: "${conv.title}"`);
            await this.summarizeSession(conv.id);
            compensated++;
          }
        }
      }

      if (compensated > 0) {
        console.log(`[MemoryEngine] Startup compensation complete: ${compensated} sessions summarized.`);
      }
    } catch (err) {
      console.error('[MemoryEngine] Startup compensation failed:', err);
    }
  }

  // ── 7天迁移：memory/sessions → wiki/sessions ──────

  migrateOldSessions() {
    try {
      const now = Date.now();
      const files = fs.readdirSync(this.sessionsDir)
        .filter(f => f.endsWith('.md'))
        .map(f => {
          const filePath = path.join(this.sessionsDir, f);
          const stat = fs.statSync(filePath);
          return { file: f, path: filePath, mtime: stat.mtime.getTime() };
        })
        .filter(f => (now - f.mtime) > SEVEN_DAYS_MS);

      let migrated = 0;
      for (const f of files) {
        const dest = path.join(this.wikiSessionsDir, f.file);
        fs.renameSync(f.path, dest);
        migrated++;
      }

      if (migrated > 0) {
        console.log(`[MemoryEngine] Migrated ${migrated} old sessions to wiki/sessions/.`);
      }
    } catch (err) {
      console.error('[MemoryEngine] Migration failed:', err);
    }
  }

  // ── 级联删除 ─────────────────────────────────────

  deleteSessionFiles(conversationId) {
    const hotFile = path.join(this.sessionsDir, `${conversationId}.md`);
    const coldFile = path.join(this.wikiSessionsDir, `${conversationId}.md`);

    for (const filePath of [hotFile, coldFile]) {
      try {
        if (fs.existsSync(filePath)) {
          fs.unlinkSync(filePath);
          console.log(`[MemoryEngine] Deleted session file: ${filePath}`);
        }
      } catch (err) {
        console.error(`[MemoryEngine] Failed to delete ${filePath}:`, err);
      }
    }
  }

  // ── brain_search: 搜索 wiki 和冷记忆 ──────────────

  searchBrain(query, scope = 'all') {
    const results = [];
    const searchDirs = [];

    if (scope === 'memory' || scope === 'all') {
      searchDirs.push({ dir: this.wikiSessionsDir, label: '历史对话' });
    }
    if (scope === 'wiki' || scope === 'all') {
      searchDirs.push({ dir: path.join(this.wikiDir, 'projects'), label: '项目知识' });
      searchDirs.push({ dir: path.join(this.wikiDir, 'clippings'), label: '知识剪报' });
    }

    const queryLower = query.toLowerCase();

    for (const { dir, label } of searchDirs) {
      if (!fs.existsSync(dir)) continue;

      const files = fs.readdirSync(dir).filter(f => f.endsWith('.md'));
      for (const file of files) {
        const filePath = path.join(dir, file);
        const content = fs.readFileSync(filePath, 'utf-8');

        if (content.toLowerCase().includes(queryLower)) {
          const title = this._extractTitle(content) || file;
          // 找到匹配行及上下文
          const lines = content.split('\n');
          const matchLines = [];
          for (let i = 0; i < lines.length; i++) {
            if (lines[i].toLowerCase().includes(queryLower)) {
              const start = Math.max(0, i - 1);
              const end = Math.min(lines.length - 1, i + 1);
              matchLines.push(lines.slice(start, end + 1).join('\n'));
            }
          }
          results.push({
            source: label,
            title: title,
            file: file,
            excerpt: matchLines.slice(0, 3).join('\n...\n')
          });
        }

        if (results.length >= 5) break;
      }
      if (results.length >= 5) break;
    }

    return results;
  }

  // ── 工具方法 ─────────────────────────────────────

  _stripFrontmatter(content) {
    const match = content.match(/^---\n[\s\S]*?\n---\n([\s\S]*)$/);
    return match ? match[1] : content;
  }

  _extractTitle(content) {
    const match = content.match(/^---\n[\s\S]*?title:\s*(.+)\n[\s\S]*?---/);
    return match ? match[1].trim() : null;
  }
}

module.exports = { MemoryEngine };
