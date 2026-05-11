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
   * @param {boolean} isExternal - 是否为外部不受信任的技能
   */
  scanDirectory(dirPath, isExternal = false) {
    if (!fs.existsSync(dirPath)) {
      console.warn(`[ToolRegistry] Directory does not exist: ${dirPath}`);
      return;
    }

    try {
      const items = fs.readdirSync(dirPath, { withFileTypes: true });
      for (const item of items) {
        const fullPath = path.join(dirPath, item.name);

        if (item.isDirectory()) {
          this.scanDirectory(fullPath, isExternal);
        } else if (item.isFile() && item.name.endsWith('.js')) {
          try {
            if (isExternal) {
              // TODO: Migrating to a safer sandbox execution environment is the ultimate goal.
              console.warn(`[SECURITY WARNING] Loading external skill from ${fullPath}. This is an arbitrary code execution risk (RCE).`);
              const { dialog } = require('electron');
              const response = dialog.showMessageBoxSync({
                type: 'warning',
                buttons: ['Allow', 'Deny'],
                defaultId: 1,
                title: 'Security Warning',
                message: `The application is trying to load an external skill from:\n${fullPath}\n\nLoading external scripts allows arbitrary code execution. Do you want to allow this?`
              });
              if (response !== 0) {
                console.log(`[ToolRegistry] User denied loading external skill: ${fullPath}`);
                continue;
              }
            }
            const mod = require(fullPath);
            if (mod && mod.name && mod.execute) {
              // Wrap simple exported objects into a BaseTool
              class DynamicTool extends BaseTool {
                constructor() {
                  super();
                  this.name = mod.name;
                  this.description = mod.description || 'No description';
                  this.input_schema = mod.parameters || { type: 'object', properties: {} };
                }
                async execute(params) {
                  return await mod.execute(params);
                }
              }
              this.register(new DynamicTool());
            }
          } catch (reqErr) {
             console.error(`[ToolRegistry] Failed to load tool from ${fullPath}:`, reqErr);
          }
        }
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
    const builtInDir = path.join(__dirname, 'built-in');
    this.scanDirectory(builtInDir, false);

    // 2. 加载外部扩展工具
    if (externalSkillsDir) {
      console.log(`[ToolRegistry] Loading external skills from: ${externalSkillsDir}`);
      this.scanDirectory(externalSkillsDir, true);
    }
  }
}

module.exports = { ToolRegistry };
