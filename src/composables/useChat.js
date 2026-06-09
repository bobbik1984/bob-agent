/**
 * useChat — 核心聊天逻辑 composable
 *
 * 职责:
 *   - 消息列表管理 (messages, displayMessages, loadMessages)
 *   - 消息发送 (sendMessage, handleStreamChunk, stopGeneration)
 *   - 流式状态 (isStreaming, streamContent, streamThinking, activeTools)
 *   - 会话费用 (sessionCost)
 *   - 对话导出 (exportConversation)
 *   - Outbox bob-config 代码块拦截
 *   - Markdown 渲染 (renderMarkdown, renderMessageBlocks)
 *   - 日程解析交互
 *
 * 注意：此模块不包含 UI 引用 (如 DOM ref)，滚动行为由外部注入的 scrollToBottom 回调驱动
 */

import { ref, computed, nextTick } from 'vue';
import { marked } from 'marked';
import DOMPurify from 'dompurify';

// ── 文件链接正则 (拆分 FileCard) ──
const FILE_LINK_RE = /\<a\s+[^>]*href="((?:file:\/\/\/[^"]+)|(?:[A-Za-z]:[\\][^"]+))"[^>]*>[^<]*<\/a>/gi;

export function useChat(props, emit, { scrollToBottom, currentModelName, globalFileAccess, agentMode }) {
  // ── 状态 ─────────────────────────────────────────
  const messages = ref([]);
  const inputText = ref('');
  const isStreaming = ref(false);
  const streamContent = ref('');
  const streamThinking = ref('');
  const activeTools = ref([]);
  const isParsing = ref(false);
  const sessionCost = ref(0);

  const canSend = ref(true);

  // 过滤掉系统消息
  const displayMessages = computed(() => {
    return messages.value.filter(m =>
      m.role !== 'system' && !(m.content && m.content.startsWith('__rename__'))
    );
  });

  // ── 消息加载 ─────────────────────────────────────
  async function loadMessages() {
    if (!props.conversationId) return;
    const rawMessages = await window.electronAPI.getMessages(props.conversationId);
    messages.value = rawMessages.map(m => ({
      ...m,
      _thinkingExpanded: false,
    }));
    await nextTick();
    scrollToBottom();
    setTimeout(scrollToBottom, 150);
    setTimeout(scrollToBottom, 500);
    setTimeout(scrollToBottom, 1200);
  }

  // ── 晨间汇报交互 ──────────────────────────────────
  function onBriefingChat(briefingContent) {
    inputText.value = `关于你刚才的晨间回顾，我想继续聊聊：\n\n${briefingContent}`;
    nextTick(() => sendMessage());
  }

  // ── 发送消息 ─────────────────────────────────────
  async function sendMessage(pendingImage, pendingFiles, resetTextareaHeight) {
    const text = inputText.value.trim();
    if (!text && !pendingImage.value && pendingFiles.value.length === 0) return;
    if (isStreaming.value) return;

    const filesToRead = [...pendingFiles.value];
    const imageBase64 = pendingImage.value;

    const userMessage = {
      role: 'user',
      content: text || (imageBase64 ? '请分析这张图片' : '请分析附件内容'),
      image_base64: imageBase64 || null,
    };

    // 立即添加到 UI 并清空输入框
    messages.value.push(userMessage);
    inputText.value = '';
    pendingImage.value = null;
    pendingFiles.value = [];
    resetTextareaHeight();

    // 立即激活打字机指示器
    isStreaming.value = true;
    streamContent.value = '';
    streamThinking.value = '';
    activeTools.value = [];

    await nextTick();
    scrollToBottom();

    // 将附件路径展示在界面上
    if (filesToRead.length > 0) {
      for (const f of filesToRead) {
        userMessage.content += `\n\n[📎 附件已就绪: ${f.path}]`;
      }
    }

    // 持久化
    await window.electronAPI.addMessage(
      props.conversationId,
      'user',
      userMessage.content,
      userMessage.image_base64
    );

    // 自动更新对话标题（第一条消息）
    if (messages.value.filter(m => m.role === 'user').length === 1) {
      const title = userMessage.content.slice(0, 30) || '图片分析';
      emit('update-title', props.conversationId, title);
    }

    // 构建 API 消息格式
    const apiMessages = messages.value
      .filter(m => m.role !== 'system' && m.type !== 'confirm-card')
      .map(m => ({
        role: m.role,
        content: (m.content || '').replace(/<\|mem\|>/g, '').trim(),
      }));

    // 在发给大模型的最终载荷里，偷偷塞入系统指令（不污染前端 UI 和数据库）
    if (filesToRead.length > 0) {
      const lastApiMsg = apiMessages[apiMessages.length - 1];
      lastApiMsg.content += `\n\n（系统内部提示：如果用户要求分析或总结上述附件，请调用 read_file 工具阅读；如果用户要求"整理进知识库"，请绝对不要尝试自己阅读，直接调用 build_knowledge_base 工具将其发往后台 Clerk 引擎）`;
    }

    await nextTick();
    scrollToBottom();

    try {
      let result;
      console.log('[sendMessage] image_base64 present:', !!userMessage.image_base64, 'apiMessages count:', apiMessages.length);
      if (userMessage.image_base64) {
        result = await window.electronAPI.sendVision(apiMessages, userMessage.image_base64, globalFileAccess.value, agentMode.value, props.conversationId);
      } else {
        result = await window.electronAPI.sendChat(apiMessages, globalFileAccess.value, agentMode.value, props.conversationId);
      }
      console.log('[sendMessage] result:', JSON.stringify(result).slice(0, 300));

      // 计算本次费用
      if (result.usage && result.pricing) {
        const inputCost = (result.usage.prompt_tokens || 0) / 1_000_000 * result.pricing.input;
        const outputCost = (result.usage.completion_tokens || 0) / 1_000_000 * result.pricing.output;
        sessionCost.value += inputCost + outputCost;
        if (props.conversationId) {
          window.electronAPI.updateConversationCost(props.conversationId, sessionCost.value);
        }
      }

      if (result.error) {
        messages.value.push({
          role: 'assistant',
          content: result.error,
          _isError: true,
          _thinkingExpanded: false,
        });
      } else {
        let finalContent = streamContent.value || result.content || '';
        let finalThinking = streamThinking.value || result.thinking || null;

        // ── Outbox: 检测 bob-config 代码块 (T-812) ──
        const configBlockRegex = /```bob-config\n([\s\S]*?)\n```/g;
        let match;
        const outboxOps = [];
        while ((match = configBlockRegex.exec(finalContent)) !== null) {
          try {
            const op = JSON.parse(match[1]);
            outboxOps.push(op);
          } catch (e) {
            console.warn('[Outbox] bob-config 块 JSON 解析失败:', e);
          }
        }
        if (outboxOps.length > 0) {
          try {
            await window.electronAPI.writeOutbox(outboxOps);
            console.log(`[Outbox] 已写入 ${outboxOps.length} 条配置操作`);
          } catch (e) {
            console.error('[Outbox] writeOutbox 失败:', e);
          }
          finalContent = finalContent.replace(configBlockRegex, '').trim();
        }

        // ── T-1306: 行动项捕获 (bob-action-items 代码块) ──
        const actionBlockRegex = /```bob-action-items\n([\s\S]*?)\n```/g;
        const extractedItems = [];
        let actionMatch;
        while ((actionMatch = actionBlockRegex.exec(finalContent)) !== null) {
          try {
            const items = JSON.parse(actionMatch[1]);
            if (Array.isArray(items)) {
              extractedItems.push(...items);
            }
          } catch (e) {
            console.warn('[ActionItems] bob-action-items JSON 解析失败:', e);
          }
        }
        if (extractedItems.length > 0) {
          finalContent = finalContent.replace(actionBlockRegex, '').trim();
        }

        const assistantMsg = {
          role: 'assistant',
          content: finalContent || '（模型未返回内容，请检查 API 配置或重试）',
          thinking: finalThinking,
          _thinkingExpanded: false,
          _modelLabel: currentModelName.value || result.model || '',
        };

        messages.value.push(assistantMsg);
        await window.electronAPI.addMessage(
          props.conversationId,
          'assistant',
          assistantMsg.content,
          null
        );

        if (messages.value.filter(m => m.role === 'user').length === 1) {
          window.electronAPI.autoRenameConversation(props.conversationId).then(title => {
            if (title) {
              emit('update-title', props.conversationId, title);
            }
          }).catch(console.error);
        }

        // T-1306: 将提取的行动项推入消息列表作为交互卡片
        for (const item of extractedItems) {
          messages.value.push({
            role: 'assistant',
            type: 'action-item-card',
            actionItem: {
              title: item.title || '',
              type: item.type || 'todo',
              date: item.date || null,
            },
          });
        }
      }
    } catch (err) {
      console.error('[sendMessage] exception:', err);
      messages.value.push({
        role: 'assistant',
        content: err.message,
        _isError: true,
        _thinkingExpanded: false,
      });
    } finally {
      isStreaming.value = false;
      streamContent.value = '';
      streamThinking.value = '';
      activeTools.value = [];
      scrollToBottom();
    }
  }

  // ── 流式块处理 ─────────────────────────────────────
  function handleStreamChunk(chunk) {
    // 跨会话隔离：忽略不属于当前对话的 chunk（WeChat 等远程通道产生的流）
    if (chunk.conv_id && props.conversationId && chunk.conv_id !== props.conversationId) {
      return;
    }
    if (chunk.type === 'clear') {
      streamContent.value = '';
      return;
    } else if (chunk.type === 'text') {
      streamContent.value += chunk.content;
    } else if (chunk.type === 'thinking') {
      streamThinking.value += chunk.content;
    } else if (chunk.type === 'tool_start') {
      activeTools.value.push({ name: chunk.name, status: 'running', result: null, _expanded: false });
    } else if (chunk.type === 'tool_end') {
      const tool = activeTools.value.find(t => t.name === chunk.name && t.status === 'running');
      if (tool) {
        tool.status = 'done';
        tool.result = chunk.result;
        if (chunk.name === 'web_search' && chunk.result) {
          try {
            const parsed = JSON.parse(chunk.result);
            if (parsed.results && Array.isArray(parsed.results)) {
              tool._searchResults = parsed.results;
              tool._expanded = true;
            }
          } catch (e) { /* 解析失败则显示原始文本 */ }
        }
        // 浏览器增强发现式 UX: browse_page 返回 action_required 时弹出确认卡片
        if (chunk.name === 'browse_page' && chunk.result) {
          try {
            const parsed = JSON.parse(chunk.result);
            if (parsed.action_required === 'browser_enable') {
              tool._browserEnable = {
                url: parsed.original_url || '',
                browserPath: parsed.browser_path || '',
                browserDetected: !!parsed.browser_detected,
              };
              tool._expanded = true;
              tool.result = parsed.message || '需要启用浏览器增强';
            }
          } catch (e) { /* 非 JSON 则正常展示 */ }
        }
      }
    }
    scrollToBottom();
  }

  async function stopGeneration() {
    await window.electronAPI.stopGeneration();
    isStreaming.value = false;
  }

  // ── 对话导出 ─────────────────────────────────────
  async function exportConversation() {
    if (messages.value.length === 0) return;
    const lines = [];
    const title = messages.value.find(m => m.role === 'user')?.content?.slice(0, 30) || '对话';
    const date = new Date().toLocaleDateString('zh-CN');
    lines.push(`# ${title}`);
    lines.push(`> 导出时间: ${date}\n`);
    for (const msg of messages.value) {
      if (msg.role === 'system' || msg.type === 'confirm-card') continue;
      const role = msg.role === 'user' ? '👤 用户' : '🤖 Bob';
      lines.push(`## ${role}\n`);
      lines.push(msg.content || '');
      lines.push('');
    }
    const md = lines.join('\n');
    const safeName = title.replace(/[<>:"/\\|?*]/g, '_');
    await window.electronAPI.exportMarkdown(md, `${safeName}.md`);
  }

  // ── Markdown 渲染 ─────────────────────────────────
  function renderMarkdown(text) {
    if (!text) return '';
    const cleaned = text.replace(/<calendar_event>[\s\S]*?(?:<\/calendar_event>|$)/gi, '')
      .replace(/<\|mem\|>/g, ''); // 视觉过滤进化引擎隐式标记
    let rawHtml = marked.parse(cleaned);

    // ── bob:// 本地文件协议桥接 ──
    // 将 <img src="D:\...\file.png"> 或 <img src="file:///D:/..."> 
    // 转换为 <img src="bob://localhost/D:/..."> 以触发 Rust 后端流式读取
    rawHtml = rawHtml.replace(
      /(<img\s+[^>]*src=")(?:file:\/\/\/)?([A-Za-z]:[\\\/][^"]+)(")/gi,
      (_, pre, path, post) => pre + 'bob://localhost/' + path.replace(/\\/g, '/') + post
    );
    // 对 video / source 标签做同样处理
    rawHtml = rawHtml.replace(
      /(<(?:video|source)\s+[^>]*src=")(?:file:\/\/\/)?([A-Za-z]:[\\\/][^"]+)(")/gi,
      (_, pre, path, post) => pre + 'bob://localhost/' + path.replace(/\\/g, '/') + post
    );

    return DOMPurify.sanitize(rawHtml, { ADD_TAGS: ['video', 'source'], ADD_ATTR: ['controls', 'autoplay', 'loop', 'muted'] });
  }

  function renderMessageBlocks(text) {
    if (!text) return [{ type: 'html', content: '' }];
    const html = renderMarkdown(text);
    FILE_LINK_RE.lastIndex = 0;
    if (!FILE_LINK_RE.test(html)) {
      return [{ type: 'html', content: html }];
    }
    const blocks = [];
    let lastIndex = 0;
    FILE_LINK_RE.lastIndex = 0;
    let match;
    while ((match = FILE_LINK_RE.exec(html)) !== null) {
      if (match.index > lastIndex) {
        blocks.push({ type: 'html', content: html.slice(lastIndex, match.index) });
      }
      let filePath = match[1];
      if (filePath.startsWith('file:///')) {
        filePath = filePath.replace('file:///', '');
      }
      try { filePath = decodeURIComponent(filePath); } catch(e) {}
      filePath = filePath.replace(/\//g, '\\');
      blocks.push({ type: 'file', path: filePath });
      lastIndex = match.index + match[0].length;
    }
    if (lastIndex < html.length) {
      blocks.push({ type: 'html', content: html.slice(lastIndex) });
    }
    return blocks;
  }

  // ── 日程解析 ─────────────────────────────────────
  async function parseTextAsEvent(resetTextareaHeight) {
    const text = inputText.value.trim();
    if (!text) return;
    isParsing.value = true;
    try {
      const parsed = await window.electronAPI.parseEvent(text);
      messages.value.push({ role: 'assistant', type: 'confirm-card', event: parsed });
      scrollToBottom();
    } catch (err) {
      messages.value.push({ role: 'assistant', content: `解析日程失败: ${err.message}` });
    } finally {
      isParsing.value = false;
      inputText.value = '';
      resetTextareaHeight();
    }
  }

  async function handleConfirmEvent(event, msgObj) {
    try {
      const plainEvent = JSON.parse(JSON.stringify(event));
      const res = await window.electronAPI.confirmEvent(plainEvent);
      if (res.ok) {
        msgObj.content = `已成功保存为${event.type === 'todo' ? '待办' : '日程'}：${event.title}`;
        msgObj.type = 'text';
      } else {
        msgObj.content = `保存失败: ${res.error}`;
        msgObj.type = 'text';
      }
    } catch (err) {
      msgObj.content = `⚠️ 保存失败: ${err.message}`;
      msgObj.type = 'text';
    }
  }

  function handleCancelEvent(msgObj) {
    msgObj.content = '已取消保存';
    msgObj.type = 'text';
  }

  // ── T-1306: 行动项卡片交互 ─────────────────────────
  async function handleSaveActionItem(item, msgObj) {
    try {
      const event = {
        title: item.title,
        type: item.type || 'todo',
        status: 'pending',
        date: item.date || null,
        startTime: null,
        endTime: null,
        description: '',
      };
      const res = await window.electronAPI.confirmEvent(event);
      if (res.ok) {
        msgObj.content = `已保存${item.type === 'todo' ? '待办' : '日程'}: ${item.title}`;
        msgObj.type = 'text';
      } else {
        msgObj.content = `保存失败: ${res.error}`;
        msgObj.type = 'text';
      }
    } catch (err) {
      msgObj.content = `保存失败: ${err.message}`;
      msgObj.type = 'text';
    }
  }

  function handleDismissActionItem(msgObj) {
    msgObj.content = '已忽略';
    msgObj.type = 'text';
  }

  return {
    // 状态
    messages,
    displayMessages,
    inputText,
    isStreaming,
    streamContent,
    streamThinking,
    activeTools,
    isParsing,
    sessionCost,
    canSend,
    // 方法
    loadMessages,
    onBriefingChat,
    sendMessage,
    handleStreamChunk,
    stopGeneration,
    exportConversation,
    renderMarkdown,
    renderMessageBlocks,
    parseTextAsEvent,
    handleConfirmEvent,
    handleCancelEvent,
    handleSaveActionItem,
    handleDismissActionItem,
  };
}
