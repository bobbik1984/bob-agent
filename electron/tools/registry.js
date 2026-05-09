const fs = require('fs');
const path = require('path');
const { BaseTool } = require('./base');

class ToolRegistry {
  constructor() {
    this.tools = new Map();
  }

  /**
   * 注册单个工具实例
   */
  register(toolInstance) {
    if (!(toolInstance instanceof BaseTool)) {
      throw new Error(`[ToolRegistry] Tool must inherit from BaseTool`);
    }
    this.tools.set(toolInstance.name, toolInstance);
    console.log(`[ToolRegistry] Registered tool: ${toolInstance.name}`);
  }

  /**
   * 获取所有注册工具的 Schema (用于发给 LLM)
   */
  getAllSchemas() {
    return Array.from(this.tools.values()).map(tool => tool.getSchema());
  }

  /**
   * 执行指定工具
   */
  async executeTool(name, params) {
    const tool = this.tools.get(name);
    if (!tool) {
      throw new Error(`[ToolRegistry] Tool not found: ${name}`);
    }
    console.log(`[ToolRegistry] Executing tool ${name} with params:`, params);
    return await tool.execute(params);
  }

  /**
   * 扫描并加载目录下的所有技能模块
   * @param {string} dirPath - 包含技能的目录路径 (可以是内部 skills/ 也可以是外部 config 设置的目录)
   */
  scanDirectory(dirPath) {
    if (!fs.existsSync(dirPath)) {
      console.warn(`[ToolRegistry] Directory does not exist: ${dirPath}`);
      return;
    }

    try {
      const items = fs.readdirSync(dirPath, { withFileTypes: true });
      for (const item of items) {
        // 这里只是一个骨架，Jules 会实现具体的扫描逻辑：
        // 比如寻找 index.js 或 *.js， require 进来，并调用 this.register()
        // 需要处理外部目录可能带来的安全性问题
      }
    } catch (err) {
      console.error(`[ToolRegistry] Error scanning directory ${dirPath}:`, err);
    }
  }

  /**
   * 加载内置技能与外部技能
   * @param {string} externalSkillsDir - 来自用户配置的外部技能路径 (可选)
   */
  init(externalSkillsDir = null) {
    this.tools.clear();
    
    // 1. 加载内置基础工具 (如文件读写、事件管理等)
    const builtInDir = path.join(__dirname, '..', '..', 'skills');
    this.scanDirectory(builtInDir);

    // 2. 加载外部扩展工具
    if (externalSkillsDir) {
      console.log(`[ToolRegistry] Loading external skills from: ${externalSkillsDir}`);
      this.scanDirectory(externalSkillsDir);
    }
  }
}

module.exports = { ToolRegistry };
