/**
 * bob-agent — SQLite 持久化层
 *
 * 使用 better-sqlite3 管理本地数据：
 * - conversations / messages (对话历史)
 * - events (日程事件，schema 与 TodoList 一致)
 * - config (键值配置)
 */

const BetterSqlite3 = require('better-sqlite3');
const path = require('path');
const crypto = require('crypto');

class Database {
  /**
   * @param {string} userDataPath - Electron app.getPath('userData') 目录
   */
  constructor(userDataPath) {
    const dbPath = path.join(userDataPath, 'bob-agent.db');
    this.db = new BetterSqlite3(dbPath);

    // 开启 WAL 模式提升并发性能
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('foreign_keys = ON');

    this._initTables();
  }

  _initTables() {
    this.db.exec(`
      -- 对话历史
      CREATE TABLE IF NOT EXISTS conversations (
        id TEXT PRIMARY KEY,
        title TEXT,
        model TEXT,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
      );

      CREATE TABLE IF NOT EXISTS messages (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
        role TEXT NOT NULL,
        content TEXT NOT NULL,
        image_base64 TEXT,
        thinking TEXT,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
      );

      -- 日程事件 (与 TodoList schema 一致)
      CREATE TABLE IF NOT EXISTS events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        type TEXT DEFAULT 'event',
        title TEXT NOT NULL,
        start_time TEXT,
        end_time TEXT,
        location TEXT,
        description TEXT,
        participants TEXT,
        priority TEXT DEFAULT 'medium',
        tags TEXT,
        status TEXT DEFAULT 'pending',
        raw_input TEXT,
        calendar_event_id TEXT,
        confidence REAL,
        source TEXT DEFAULT 'manual',
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
      );

      -- 配置键值表
      CREATE TABLE IF NOT EXISTS config (
        key TEXT PRIMARY KEY,
        value TEXT
      );

      -- 关注的文件夹
      CREATE TABLE IF NOT EXISTS tracked_folders (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        path TEXT NOT NULL UNIQUE,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
      );
    `);
  }

  // ── 对话管理 ─────────────────────────────────────────

  createConversation(title = '新对话', model = '') {
    const id = crypto.randomUUID();
    this.db.prepare(
      'INSERT INTO conversations (id, title, model) VALUES (?, ?, ?)'
    ).run(id, title, model);
    return { id, title, model, created_at: new Date().toISOString() };
  }

  getConversations() {
    return this.db.prepare(`
      SELECT c.*,
        (SELECT SUBSTR(m.content, 1, 80) FROM messages m
         WHERE m.conversation_id = c.id
         ORDER BY m.created_at DESC LIMIT 1) AS last_message,
        (SELECT m.role FROM messages m
         WHERE m.conversation_id = c.id
         ORDER BY m.created_at DESC LIMIT 1) AS last_role
      FROM conversations c
      ORDER BY c.updated_at DESC
    `).all();
  }

  deleteConversation(id) {
    this.db.prepare('DELETE FROM conversations WHERE id = ?').run(id);
  }

  updateConversationTitle(id, title) {
    this.db.prepare(
      'UPDATE conversations SET title = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?'
    ).run(title, id);
  }

  // ── 消息管理 ─────────────────────────────────────────

  addMessage(conversationId, role, content, imageBase64 = null, thinking = null) {
    const result = this.db.prepare(
      'INSERT INTO messages (conversation_id, role, content, image_base64, thinking) VALUES (?, ?, ?, ?, ?)'
    ).run(conversationId, role, content, imageBase64, thinking);

    // 更新对话时间戳
    this.db.prepare(
      'UPDATE conversations SET updated_at = CURRENT_TIMESTAMP WHERE id = ?'
    ).run(conversationId);

    return { id: result.lastInsertRowid };
  }

  getMessages(conversationId) {
    return this.db.prepare(
      'SELECT * FROM messages WHERE conversation_id = ? ORDER BY created_at ASC'
    ).all(conversationId);
  }

  // ── 事件管理 ─────────────────────────────────────────

  createEvent(event) {
    const result = this.db.prepare(`
      INSERT INTO events (type, title, start_time, end_time, location, description,
        participants, priority, tags, status, raw_input, confidence, source)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).run(
      event.type || 'event',
      event.title,
      event.start_time || null,
      event.end_time || null,
      event.location || null,
      event.description || null,
      JSON.stringify(event.participants || []),
      event.priority || 'medium',
      JSON.stringify(event.tags || []),
      event.status || 'pending',
      event.raw_input || null,
      event.confidence || null,
      event.source || 'manual'
    );
    return result.lastInsertRowid;
  }

  listEvents(startDate, endDate) {
    if (startDate && endDate) {
      return this.db.prepare(
        'SELECT * FROM events WHERE start_time >= ? AND start_time <= ? ORDER BY start_time ASC'
      ).all(startDate, endDate);
    }
    return this.db.prepare(
      'SELECT * FROM events ORDER BY start_time ASC'
    ).all();
  }

  deleteEvent(id) {
    this.db.prepare('DELETE FROM events WHERE id = ?').run(id);
  }

  updateEventStatus(id, status) {
    this.db.prepare(
      'UPDATE events SET status = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?'
    ).run(status, id);
  }

  updateEventTime(id, startTime, endTime) {
    this.db.prepare(
      'UPDATE events SET start_time = ?, end_time = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?'
    ).run(startTime, endTime, id);
  }

  // ── 文件夹跟踪管理 ────────────────────────────────────

  addTrackedFolder(name, folderPath) {
    const id = crypto.randomUUID();
    this.db.prepare(
      'INSERT OR IGNORE INTO tracked_folders (id, name, path) VALUES (?, ?, ?)'
    ).run(id, name, folderPath);
    return { id, name, path: folderPath };
  }

  removeTrackedFolder(folderPath) {
    this.db.prepare('DELETE FROM tracked_folders WHERE path = ?').run(folderPath);
  }

  getTrackedFolders() {
    return this.db.prepare('SELECT * FROM tracked_folders ORDER BY created_at DESC').all();
  }

  isTrackedFolder(folderPath) {
    const row = this.db.prepare('SELECT id FROM tracked_folders WHERE path = ?').get(folderPath);
    return !!row;
  }

  // ── 配置管理 ─────────────────────────────────────────

  getConfig(key) {
    const row = this.db.prepare('SELECT value FROM config WHERE key = ?').get(key);
    return row?.value || null;
  }

  setConfig(key, value) {
    this.db.prepare(
      'INSERT OR REPLACE INTO config (key, value) VALUES (?, ?)'
    ).run(key, value);
  }

  getAllConfig() {
    const rows = this.db.prepare('SELECT * FROM config').all();
    const config = {};
    for (const row of rows) {
      config[row.key] = row.value;
    }
    return config;
  }

  /** 关闭数据库 */
  close() {
    this.db.close();
  }
}

module.exports = { Database };
