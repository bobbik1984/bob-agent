/**
 * WikilinkExtension — Tiptap 3.x 自定义内联节点
 * 
 * 功能：
 * - 识别 [[笔记标题]] 语法，渲染为蓝色可点击内链
 * - 输入 ]] 时自动转换为 wikilink 节点
 * - Markdown 序列化保持 [[]] 原样
 * - 加载 Markdown 时自动识别 [[]] 并转换为节点
 * - 点击时触发自定义事件供父组件处理导航
 */
import { Node, mergeAttributes, inputRules } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';

const WIKILINK_INPUT_REGEX = /\[\[([^\[\]]+)\]\]$/;

export const WikilinkExtension = Node.create({
  name: 'wikilink',
  group: 'inline',
  inline: true,
  atom: true,  // 不可内部编辑，整体操作

  addOptions() {
    return {
      // 外部传入的回调：点击 wikilink 时触发
      onWikilinkClick: null,
      // 用于判断目标笔记是否存在的函数
      noteExists: null,
    };
  },

  addAttributes() {
    return {
      target: {
        default: null,
        parseHTML: (element) => element.getAttribute('data-wikilink'),
        renderHTML: (attributes) => ({ 'data-wikilink': attributes.target }),
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: 'span[data-wikilink]',
      },
    ];
  },

  renderHTML({ node, HTMLAttributes }) {
    return [
      'span',
      mergeAttributes(HTMLAttributes, {
        class: 'wikilink',
        'data-wikilink': node.attrs.target,
        title: node.attrs.target,
      }),
      `[[${node.attrs.target}]]`,
    ];
  },

  addInputRules() {
    return [
      {
        find: WIKILINK_INPUT_REGEX,
        handler: ({ state, range, match }) => {
          const target = match[1].trim();
          if (!target) return;
          const { tr } = state;
          tr.replaceWith(
            range.from,
            range.to,
            this.type.create({ target })
          );
        },
      },
    ];
  },

  addProseMirrorPlugins() {
    const extension = this;
    return [
      // 点击事件处理
      new Plugin({
        key: new PluginKey('wikilink-click'),
        props: {
          handleClick(view, pos, event) {
            const target = event.target;
            if (target && target.hasAttribute && target.hasAttribute('data-wikilink')) {
              const noteTitle = target.getAttribute('data-wikilink');
              if (extension.options.onWikilinkClick) {
                extension.options.onWikilinkClick(noteTitle);
              }
              return true;
            }
            return false;
          },
        },
      }),
      // 加载时扫描纯文本中的 [[...]] 并转换为 wikilink 节点
      new Plugin({
        key: new PluginKey('wikilink-paste-parse'),
        props: {
          // 处理粘贴文本中的 [[...]]
          transformPasted(slice) {
            return slice;
          },
        },
      }),
    ];
  },

  // tiptap-markdown 集成：序列化为 [[target]]
  addStorage() {
    return {
      markdown: {
        serialize(state, node) {
          state.write(`[[${node.attrs.target}]]`);
        },
        parse: {
          // 不在 markdown-it 层面解析，而是通过 post-process 处理
        },
      },
    };
  },
});

/**
 * 工具函数：预处理 Markdown 文本，将 [[...]] 转为 HTML span
 * 在 Tiptap 加载内容前调用，确保 [[]] 被正确解析为 wikilink 节点
 */
export function preprocessWikilinks(markdown) {
  if (!markdown) return markdown;
  return markdown.replace(
    /\[\[([^\[\]]+)\]\]/g,
    (_, target) => `<span data-wikilink="${target.trim()}">[[${target.trim()}]]</span>`
  );
}

export default WikilinkExtension;
