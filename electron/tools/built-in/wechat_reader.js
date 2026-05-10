const cheerio = require('cheerio');

module.exports = {
  name: 'wechat_reader',
  description: '专门用于读取微信公众号文章正文内容的工具。当需要分析或获取 mp.weixin.qq.com 的文章时，必须使用此工具，它可以自动绕过微信的防爬环境检测。',
  parameters: {
    type: 'object',
    properties: {
      url: {
        type: 'string',
        description: '微信公众号文章的完整 URL (通常以 https://mp.weixin.qq.com 开头)'
      }
    },
    required: ['url']
  },
  execute: async (args) => {
    try {
      const url = args.url;
      if (!url.includes('mp.weixin.qq.com')) {
        return '警告: 提供的 URL 不是微信公众号链接，建议使用 tinyfish_fetch 或 browser_automation。';
      }

      // 伪装成安卓微信内置浏览器以绕过验证
      const headers = {
        'User-Agent': 'Mozilla/5.0 (Linux; Android 11; Pixel 5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.210 Mobile Safari/537.36 MicroMessenger/8.0.22.2140(0x28001633) WeChat/arm64 Weixin NetType/WIFI Language/zh_CN ABI/arm64',
        'Accept': 'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.9',
        'Accept-Language': 'zh-CN,zh;q=0.9'
      };

      const response = await fetch(url, { headers });
      if (!response.ok) {
        return `抓取失败: HTTP ${response.status} ${response.statusText}`;
      }

      const html = await response.text();
      const $ = cheerio.load(html);

      // 提取标题
      const title = $('h1.rich_media_title').text().trim() || $('meta[property="og:title"]').attr('content') || 'Unknown Title';
      
      // 提取正文主体
      const contentNode = $('#js_content');
      if (!contentNode.length) {
         // 可能遇到验证页面或已被删文章
         return '未找到文章正文内容 (#js_content)，可能该文章已被删除或仍被反爬拦截。部分 HTML 预览: ' + html.substring(0, 500);
      }

      // 为了防止 cheerio 的 .text() 粘连块级元素，我们将常用块级元素后面加上换行符
      contentNode.find('p, div, section, h1, h2, h3, h4, li, br').append('\n');
      
      let text = contentNode.text();
      
      // 清理多余空行
      text = text.replace(/\n\s*\n/g, '\n\n').replace(/\n{3,}/g, '\n\n').trim();

      // 针对微信公众号尾部冗余 UI 文本的裁剪
      const cutoffKeywords = [
        "微信扫一扫可打开此内容", 
        "微信扫一扫关注该公众号",
        "预览时标签不可点",
        "轻点两下取消赞",
        "轻点两下取消在看",
        "人划线"
      ];
      
      let earliestIdx = text.length;
      for (const keyword of cutoffKeywords) {
        const idx = text.lastIndexOf(keyword);
        if (idx !== -1 && idx < earliestIdx) {
          earliestIdx = idx;
        }
      }
      
      if (earliestIdx < text.length) {
        text = text.substring(0, earliestIdx);
      }

      // 截取前 30000 字符避免 token 爆炸
      text = text.substring(0, 30000).trim();

      return `标题: ${title}\n\n正文:\n${text}`;
    } catch (err) {
      return `执行 wechat_reader 发生错误: ${err.message}`;
    }
  }
};
