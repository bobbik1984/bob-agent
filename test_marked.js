const { marked } = require('marked');

function processMarkdown(cleaned) {
    cleaned = cleaned.replace(
      /(`)?(?<!\]\()(?<!["'])(https?:\/\/[^\s<>)"\]`]+)(`)?/g,
      (full, b1, url, b2, offset) => {
        const before = cleaned.slice(Math.max(0, offset - 15), offset);
        if (/\]\(\s*$/.test(before) || /href=["']$/.test(before)) return full;
        
        let keepB1 = b1 || '';
        let keepB2 = b2 || '';
        
        // If it's fully enclosed in backticks, strip them to "break out" of the code block
        // and allow it to become a clickable markdown link.
        if (b1 === '`' && b2 === '`') {
            keepB1 = '';
            keepB2 = '';
        }

        try {
          const u = new URL(url);
          let label = u.hostname.replace(/^www\./, '');
          if (u.pathname && u.pathname !== '/') label += u.pathname.slice(0, 30);
          return `${keepB1}[${label}](${url})${keepB2}`;
        } catch { 
          return `${keepB1}[${url.slice(0, 40)}](${url})${keepB2}`; 
        }
      }
    );

    let rawHtml = marked.parse(cleaned);

    rawHtml = rawHtml.replace(
      /\[([^\]<>]+)\]\((https?:\/\/[^)]+)\)/g,
      (match, text, url, offset) => {
        // Simplified safety net
        const cleanUrl = url.replace(/&amp;/g, '&');
        const cleanText = text.replace(/&amp;/g, '&');
        return `<a href="${cleanUrl}" target="_blank">${cleanText}</a>`;
      }
    );

    return rawHtml;
}

const input = `| 站点 | 地址 |
|:----|:----|
| 🏢 **火山引擎官网** | \`https://www.volcengine.com\` |
| 📖 **方舟产品文档** | \`https://www.volcengine.com/docs/82379\` |
| 🤖 **豆包大模型介绍** | \`https://www.volcengine.com/product/doubao\` |
| 🧪 **模型体验中心** | \`https://console.volcengine.com/ark/experiment\` |
| Half backtick | \`https://www.google.com |
| Normal link | [text](https://www.github.com) |`;

console.log(processMarkdown(input));
