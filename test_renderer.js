const { marked } = require('marked');

// Custom renderer to shorten link text if the text is exactly the URL
const renderer = {
  link(href, title, text) {
    if (text === href) {
      try {
        const u = new URL(href);
        let label = u.hostname.replace(/^www\./, '');
        if (u.pathname && u.pathname !== '/') label += u.pathname.slice(0, 30);
        text = label;
      } catch (e) {
        text = href.length > 40 ? href.slice(0, 40) + '...' : href;
      }
    }
    const titleAttr = title ? ` title="${title}"` : '';
    return `<a href="${href}"${titleAttr} target="_blank">${text}</a>`;
  }
};

marked.use({ renderer });

const input = `| 站点 | 地址 |
|:----|:----|
| 🏢 **火山引擎官网** | \`https://www.volcengine.com\` |
| 📖 **方舟产品文档** | \`https://www.volcengine.com/docs/82379\` |
| 🤖 **豆包大模型介绍** | \`https://www.volcengine.com/product/doubao\` |
| 🧪 **模型体验中心** | \`https://console.volcengine.com/ark/experiment\` |
| 裸链接 | https://www.google.com/search?q=test |
| 正常链接 | [点击这里](https://www.github.com) |`;

console.log(marked.parse(input));
