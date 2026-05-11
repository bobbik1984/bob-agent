const cheerio = require('cheerio');
const { BaseTool } = require('../base');

class LinkExtractorTool extends BaseTool {
  constructor() {
    super();
    this.name = 'link_extractor';
    this.description = '提取目标网页中的所有链接（支持按文件类型过滤）。比 LLM 直接解析更可靠。当用户要求提取网页链接、下载页面文件时使用。';
    this.input_schema = {
      type: 'object',
      properties: {
        url: {
          type: 'string',
          description: '目标网页 URL'
        },
        filter: {
          type: 'string',
          enum: ['all', 'pdf', 'doc', 'img', 'video', 'external'],
          description: '过滤类型 (默认 all)',
          default: 'all'
        }
      },
      required: ['url']
    };
  }

  async execute(args) {
    const url = args.url;
    const filter = args.filter || 'all';
    
    try {
      const response = await fetch(url, {
        headers: {
          'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36'
        }
      });
      
      if (!response.ok) {
        return `抓取失败: HTTP ${response.status} ${response.statusText}`;
      }
      
      const html = await response.text();
      const $ = cheerio.load(html);
      
      let links = [];
      $('a').each((i, el) => {
        const text = $(el).text().trim() || $(el).attr('title') || '无文本';
        let href = $(el).attr('href');
        
        if (!href || href.startsWith('javascript:') || href.startsWith('#')) return;
        
        try {
          href = new URL(href, url).href;
        } catch(e) {
          return;
        }
        
        links.push({ text, href });
      });
      
      links = Array.from(new Map(links.map(item => [item.href, item])).values());
      
      if (filter !== 'all') {
        const parsedUrl = new URL(url);
        const host = parsedUrl.hostname;
        
        links = links.filter(link => {
          const lowerHref = link.href.toLowerCase();
          switch(filter) {
            case 'pdf': return lowerHref.endsWith('.pdf');
            case 'doc': return lowerHref.match(/\.(doc|docx|xls|xlsx|ppt|pptx)$/);
            case 'img': return lowerHref.match(/\.(jpg|jpeg|png|gif|svg|webp)$/);
            case 'video': return lowerHref.match(/\.(mp4|avi|mov|mkv|webm)$/);
            case 'external': 
              try { return new URL(link.href).hostname !== host; } catch(e) { return false; }
            default: return true;
          }
        });
      }
      
      return {
        total_found: links.length,
        filter_applied: filter,
        source_url: url,
        links: links
      };
    } catch (err) {
      return `提取链接失败: ${err.message}`;
    }
  }
}

module.exports = new LinkExtractorTool();
