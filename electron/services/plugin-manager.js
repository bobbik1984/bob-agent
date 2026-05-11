const fs = require('fs');
const path = require('path');
const { app } = require('electron');
const { exec } = require('child_process');

class PluginManager {
  constructor(toolRegistry) {
    this.toolRegistry = toolRegistry;
    this.pluginsDir = path.join(app.getPath('userData'), 'plugins');
    if (!fs.existsSync(this.pluginsDir)) {
      fs.mkdirSync(this.pluginsDir, { recursive: true });
    }
    
    // 确保 plugins 目录下有 package.json，否则 npm install 会往上找
    const pkgPath = path.join(this.pluginsDir, 'package.json');
    if (!fs.existsSync(pkgPath)) {
      fs.writeFileSync(pkgPath, JSON.stringify({ name: 'bob-agent-plugins', version: '1.0.0', private: true }));
    }

    // 预定义的插件注册表
    this.registry = [
      {
        id: 'document_parser',
        name: '智能知识库解析引擎',
        description: '提供原生的 Excel(xlsx)、Word(docx)、PDF 和 PPTX 解析能力，支持构建 LLM 知识库索引。',
        dependencies: ['xlsx', 'pdf-parse', 'mammoth', 'officeparser']
      }
    ];

    // 缓存静态列表以提升 UI 加载速度
    this.cachedList = [];
    this._initCache();
    
    // T-613: 监听外部技能目录实现热重载
    this.watchTimer = null;
    this.startWatcher();
  }

  /**
   * 刷新技能列表并重新注册
   */
  refreshPlugins() {
    console.log('[PluginManager] Refreshing plugins (Hot Reload)...');
    let externalSkillsDir = null;
    try {
      const { Database } = require('./db');
      const tempDb = new Database(app.getPath('userData'));
      externalSkillsDir = tempDb.getConfig('externalSkillsDir');
    } catch(e) {}
    
    if (this.toolRegistry) {
      this.toolRegistry.init(externalSkillsDir);
    }
    
    this._initCache();
    
    // 通知所有渲染进程刷新 UI
    const { BrowserWindow } = require('electron');
    BrowserWindow.getAllWindows().forEach(win => {
      win.webContents.send('plugin:updated', this.getPlugins());
    });
  }

  /**
   * 启动目录监听器
   */
  startWatcher() {
    let externalSkillsDir = path.join(__dirname, '..', '..', 'skills');
    try {
      const { Database } = require('./db');
      const tempDb = new Database(app.getPath('userData'));
      if (tempDb.getConfig('externalSkillsDir')) {
        externalSkillsDir = tempDb.getConfig('externalSkillsDir');
      }
    } catch (e) {}

    if (fs.existsSync(externalSkillsDir)) {
      console.log(`[PluginManager] Watching skills directory: ${externalSkillsDir}`);
      fs.watch(externalSkillsDir, { recursive: true }, (eventType, filename) => {
        if (this.watchTimer) clearTimeout(this.watchTimer);
        // 防抖：500ms 内的多次更改只触发一次
        this.watchTimer = setTimeout(() => {
          this.refreshPlugins();
        }, 500);
      });
    }
  }

  _initCache() {
    this.cachedList = [];

    // 内置工具的中文显示名映射
    const toolDisplayNames = {
      'browser_automation': '网页自动化',
      'link_extractor':     '链接提取器',
      'list_directory':     '目录浏览',
      'read_file':          '文件读取',
      'write_file':         '文件写入',
      'web_search':         '联网搜索',
      'system_info':        '系统信息',
      'system_time':        '系统时间',
      'tinyfish_fetch':     '网页正文抓取',
      'weather':            '天气查询',
      'wechat_reader':      '微信公众号读取',
      'create_event':       '日程创建',
    };

    // 1. 从 ToolRegistry 获取已成功加载的工具
    if (this.toolRegistry) {
      const schemas = this.toolRegistry.getAllSchemas();
      schemas.forEach(schema => {
        const fn = schema.function || schema; // 兼容可能存在的不同格式
        this.cachedList.push({
          id: `tool_${fn.name}`,
          name: toolDisplayNames[fn.name] || fn.name,
          description: fn.description || '内置基础工具',
          type: 'tool',
          typeLabel: '内置能力',
          installed: true,
          dependencies: []
        });
      });
    }

    // 2. 扫描外部认知技能 (Markdown Skills)
    let externalSkillsDir = path.join(__dirname, '..', '..', 'skills');
    try {
      const { Database } = require('./db');
      const tempDb = new Database(app.getPath('userData'));
      if (tempDb.getConfig('externalSkillsDir')) {
        externalSkillsDir = tempDb.getConfig('externalSkillsDir');
      }
    } catch (e) {}

    if (fs.existsSync(externalSkillsDir)) {
      const dirs = fs.readdirSync(externalSkillsDir, { withFileTypes: true });
      dirs.forEach(d => {
        if (d.isDirectory()) {
          let desc = '基于 Markdown 的认知技能';
          try {
            const skillMd = fs.readFileSync(path.join(externalSkillsDir, d.name, 'SKILL.md'), 'utf-8');
            const lines = skillMd.split('\n');
            const descLine = lines.find(l => l.toLowerCase().startsWith('description:') || l.startsWith('描述:'));
            if (descLine) desc = descLine.split(':')[1].trim();
          } catch(e) {}
          
          this.cachedList.push({
            id: `skill_${d.name}`,
            name: d.name,
            description: desc,
            type: 'skill',
            typeLabel: '认知技能',
            installed: true,
            dependencies: []
          });
        }
      });
    }
  }

  /**
   * 获取所有插件的状态，包括内置技能和下载插件
   */
  getPlugins() {
    const list = [...this.cachedList];
    
    // 动态获取需下载的插件状态 (因为可能随时安装)
    this.registry.forEach(plugin => {
      const isInstalled = plugin.dependencies.every(dep => {
        const depPath = path.join(this.pluginsDir, 'node_modules', dep);
        return fs.existsSync(depPath);
      });
      list.unshift({
        ...plugin,
        type: 'engine',
        typeLabel: '核心引擎',
        installed: isInstalled
      });
    });

    return list;
  }

  /**
   * 动态 require 插件目录下的包
   */
  require(packageName) {
    const pkgPath = path.join(this.pluginsDir, 'node_modules', packageName);
    if (fs.existsSync(pkgPath)) {
      return require(pkgPath);
    }
    throw new Error(`Plugin dependency not found: ${packageName}. Please install the plugin first.`);
  }

  /**
   * 安装插件的依赖
   */
  installPlugin(id, onProgress) {
    const plugin = this.registry.find(p => p.id === id);
    if (!plugin) throw new Error(`Plugin not found: ${id}`);

    return new Promise((resolve, reject) => {
      const deps = plugin.dependencies.join(' ');
      onProgress && onProgress(`开始安装 ${plugin.name} 的底层依赖...`);
      
      const cmd = `npm install ${deps} --no-save`;
      
      const child = exec(cmd, { cwd: this.pluginsDir }, (error, stdout, stderr) => {
        if (error) {
          console.error(`[PluginManager] Install Error:`, error);
          reject(error);
        } else {
          onProgress && onProgress(`安装完成！`);
          resolve();
        }
      });

      child.stdout.on('data', (data) => {
        onProgress && onProgress(data.toString());
      });
      child.stderr.on('data', (data) => {
        onProgress && onProgress(data.toString());
      });
    });
  }
}

module.exports = { PluginManager };
