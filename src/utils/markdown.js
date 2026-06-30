import { marked } from 'marked';
import hljs from 'highlight.js';
import { markedHighlight } from 'marked-highlight';
import DOMPurify from 'dompurify';

// 配置 DOMPurify：允许 file://, bob://, asset:// 等协议
DOMPurify.addHook('uponSanitizeAttribute', (node, data) => {
  if (data.attrName === 'href' || data.attrName === 'src') {
    const val = data.attrValue;
    if (/^(file|bob|asset):\/\//.test(val) || /^https?:\/\/(bob|asset)\.localhost\//.test(val) || /^[A-Za-z]:[\\\/]/.test(val)) {
      data.keepAttr = true;
      data.forceKeepAttr = true;
    }
  }
});

// 配置 marked：语法高亮 + GFM
marked.use(markedHighlight({
  langPrefix: 'hljs language-',
  highlight(code, lang) {
    const language = hljs.getLanguage(lang) ? lang : 'plaintext';
    return hljs.highlight(code, { language }).value;
  }
}));
marked.setOptions({ breaks: true, gfm: true });

/** 简单渲染（简报、关于页面等不需要复杂后处理的场景） */
export function renderMarkdownSimple(text) {
  if (!text) return '';
  const raw = marked.parse(text, { breaks: true });
  return DOMPurify.sanitize(raw);
}

/** 
 * 完整渲染（聊天消息，包含自动链接、文件协议桥接等后处理）
 * 由 useChat.js 内部调用，保留其现有的 post-processing 逻辑
 */
export { marked, DOMPurify };
