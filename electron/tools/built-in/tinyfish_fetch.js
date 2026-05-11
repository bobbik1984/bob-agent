const { BaseTool } = require('../base');

class TinyfishFetchTool extends BaseTool {
  constructor() {
    super();
    this.name = 'tinyfish_fetch';
    this.description = '使用 TinyFish Web Agent 提取任意网页的纯净正文内容（转为干净的 Markdown）。无视反爬虫，适用于文章抓取、剪报收集和资料阅读。';
    this.input_schema = {
      type: 'object',
      properties: {
        url: {
          type: 'string',
          description: '需要抓取的网页完整 URL'
        }
      },
      required: ['url']
    };
  }

  async execute(args) {
    const apiKey = process.env.TINYFISH_API_KEY;
    if (!apiKey) {
      return '错误: 环境中缺少 TINYFISH_API_KEY，请检查 .env 配置文件。';
    }
    
    try {
      const response = await fetch(`https://agent.tinyfish.ai/v1/fetch?url=${encodeURIComponent(args.url)}`, {
        headers: {
          'X-API-Key': apiKey,
          'Accept': 'application/json'
        }
      });
      
      if (!response.ok) {
        return `TinyFish Fetch 抓取失败: HTTP ${response.status} ${response.statusText}`;
      }
      
      const data = await response.json();
      
      const rawContent = data.markdown || data.content || '未提取到正文';
      return {
        title: data.title || 'Unknown Title',
        markdown_content: `<untrusted_web_content source="${args.url}">\n${rawContent}\n</untrusted_web_content>`
      };
    } catch (err) {
      return `TinyFish 请求发生错误: ${err.message}`;
    }
  }
}

module.exports = new TinyfishFetchTool();
