/**
 * MCP Client Manager — 连接外部 MCP Servers 并将其工具注册到 ToolRegistry
 * 
 * 设计：
 * - 读取 mcp_config.json 中定义的 server 列表
 * - 通过 stdio 方式启动每个 MCP Server 子进程
 * - 调用 tools/list 获取 Server 提供的工具
 * - 将每个工具动态包装成 BaseTool 子类并注册到 ToolRegistry
 */

const { Client } = require('@modelcontextprotocol/sdk/client/index.js');
const { StdioClientTransport } = require('@modelcontextprotocol/sdk/client/stdio.js');
const { BaseTool } = require('../tools/base');
const fs = require('fs');
const path = require('path');

class MCPClientManager {
  constructor(configPath) {
    this.configPath = configPath;
    this.clients = new Map(); // serverName -> { client, transport }
    this.registeredToolNames = new Set(); // 追踪已注册的 MCP 工具名，用于卸载
  }

  /**
   * 读取 mcp_config.json
   */
  loadConfig() {
    try {
      if (!fs.existsSync(this.configPath)) {
        // 不存在就创建默认空配置
        const defaultConfig = { mcpServers: {} };
        fs.writeFileSync(this.configPath, JSON.stringify(defaultConfig, null, 2), 'utf-8');
        return defaultConfig;
      }
      const raw = fs.readFileSync(this.configPath, 'utf-8');
      return JSON.parse(raw);
    } catch (err) {
      console.error('[MCPClient] Failed to load config:', err.message);
      return { mcpServers: {} };
    }
  }

  /**
   * 保存配置
   */
  saveConfig(config) {
    try {
      const dir = path.dirname(this.configPath);
      if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(this.configPath, JSON.stringify(config, null, 2), 'utf-8');
    } catch (err) {
      console.error('[MCPClient] Failed to save config:', err.message);
    }
  }

  /**
   * 启动所有配置好的 MCP Servers 并注册工具
   * @param {ToolRegistry} toolRegistry
   */
  async startAll(toolRegistry) {
    const config = this.loadConfig();
    const servers = config.mcpServers || {};

    for (const [serverName, serverConfig] of Object.entries(servers)) {
      if (serverConfig.disabled) continue;
      try {
        await this.startServer(serverName, serverConfig, toolRegistry);
      } catch (err) {
        console.error(`[MCPClient] Failed to start server "${serverName}":`, err.message);
      }
    }
  }

  /**
   * 启动单个 MCP Server
   */
  async startServer(serverName, serverConfig, toolRegistry) {
    const { command, args = [], env = {} } = serverConfig;
    if (!command) {
      console.warn(`[MCPClient] Server "${serverName}" has no command, skipping.`);
      return;
    }

    console.log(`[MCPClient] Starting server "${serverName}": ${command} ${args.join(' ')}`);

    const transport = new StdioClientTransport({
      command,
      args,
      env: { ...process.env, ...env },
    });

    const client = new Client({
      name: 'bob-agent',
      version: '0.1.0',
    });

    await client.connect(transport);
    this.clients.set(serverName, { client, transport });

    // 获取工具列表并注册
    try {
      const { tools } = await client.listTools();
      console.log(`[MCPClient] Server "${serverName}" provides ${tools.length} tools.`);

      for (const tool of tools) {
        const toolName = `mcp_${serverName}_${tool.name}`;
        const mcpTool = this._createToolProxy(toolName, tool, client);
        toolRegistry.register(mcpTool);
        this.registeredToolNames.add(toolName);
      }
    } catch (err) {
      console.error(`[MCPClient] Failed to list tools from "${serverName}":`, err.message);
    }
  }

  /**
   * 将 MCP 远端工具包装为 BaseTool 子类
   */
  _createToolProxy(toolName, mcpToolDef, client) {
    const tool = new BaseTool();
    tool.name = toolName;
    tool.description = mcpToolDef.description || `MCP tool: ${mcpToolDef.name}`;
    tool.input_schema = mcpToolDef.inputSchema || { type: 'object', properties: {} };

    // 覆写 execute 方法，委托给 MCP Client 调用远端
    tool.execute = async (params) => {
      try {
        const result = await client.callTool({
          name: mcpToolDef.name,
          arguments: params,
        });
        // 拼接 content 数组中的文本
        if (result.content && Array.isArray(result.content)) {
          return result.content.map(c => c.text || JSON.stringify(c)).join('\n');
        }
        return JSON.stringify(result);
      } catch (err) {
        return `MCP tool call failed: ${err.message}`;
      }
    };

    return tool;
  }

  /**
   * 关闭所有 MCP 连接
   */
  async stopAll() {
    for (const [name, { client, transport }] of this.clients) {
      try {
        await client.close();
        console.log(`[MCPClient] Closed server "${name}"`);
      } catch (err) {
        console.error(`[MCPClient] Error closing "${name}":`, err.message);
      }
    }
    this.clients.clear();
    this.registeredToolNames.clear();
  }

  /**
   * 重新加载：停止所有 -> 重新启动
   */
  async reload(toolRegistry) {
    await this.stopAll();
    // 从 ToolRegistry 中移除旧的 MCP 工具（ToolRegistry.tools 是 Map）
    if (toolRegistry && toolRegistry.tools) {
      for (const name of this.registeredToolNames) {
        toolRegistry.tools.delete(name);
      }
    }
    this.registeredToolNames.clear();
    await this.startAll(toolRegistry);
  }
}

module.exports = { MCPClientManager };
