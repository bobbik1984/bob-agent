<template>
  <div class="chat-view">
    <!-- 消息区域 -->
    <div class="messages-area" ref="messagesArea">
      <!-- 统一的页面标题 -->
      <div v-if="messages.length > 0" class="view-header" :style="{ opacity: logoOpacity }">
        <h2 class="view-title">
          <img src="/bob_logo.svg" class="title-bob-logo" alt="Bob" />
        </h2>
      </div>

      <!-- 空状态 -->
      <div v-if="messages.length === 0" class="empty-state animate-fade-in">
        <div class="empty-logo-wrapper">
          <img src="/bob_logo.svg" class="empty-bob-logo" alt="Bob" />
        </div>
      </div>

      <!-- 消息列表 -->
      <div
        v-for="(msg, idx) in displayMessages"
        :key="msg.id || idx"
        class="message-row animate-slide-up"
        :class="[`message-${msg.role}`]"
      >
        <!-- 头像 -->
        <div class="message-avatar" :class="msg.role === 'user' ? 'avatar-user' : 'avatar-bob'">
          <User v-if="msg.role === 'user'" :size="16" />
          <img v-else src="/bob_logo.svg" class="bob-avatar-img" alt="Bob" />
        </div>

        <!-- 内容 -->
        <div class="message-body">
          <!-- 解析出的事件卡片 -->
          <template v-if="msg.type === 'confirm-card'">
            <ConfirmCard
              :event="msg.event"
              @confirm="(e) => handleConfirmEvent(e, msg)"
              @cancel="() => handleCancelEvent(msg)"
            />
          </template>

          <!-- 思维链折叠 -->
          <div v-else-if="msg.thinking" class="thinking-card" :class="{ expanded: msg._thinkingExpanded }">
            <button class="thinking-toggle" @click="msg._thinkingExpanded = !msg._thinkingExpanded">
              <ChevronRight :size="14" class="thinking-arrow" :class="{ 'expanded': msg._thinkingExpanded }" />
              <span>Thought process</span>
            </button>
            <div v-if="msg._thinkingExpanded" class="thinking-content selectable">
              {{ msg.thinking }}
            </div>
          </div>
          <!-- 错误卡片 -->
          <div v-if="msg._isError" class="error-card">
            <div class="error-icon">!</div>
            <div class="error-body">
              <div class="error-title">{{ $t('chat.error_title') }}</div>
              <div class="error-detail">{{ msg.content }}</div>
            </div>
          </div>
          <!-- 消息内容（block 数组渲染：text + file-card 交替）-->
          <div v-else class="message-content selectable" @click="onMessageLinkClick">
            <template v-for="(block, bi) in renderMessageBlocks(msg.content)" :key="bi">
              <div v-if="block.type === 'html'" v-html="block.content"></div>
              <FileCard v-else-if="block.type === 'file'" :filePath="block.path" />
            </template>
          </div>
          <!-- 图片预览 -->
          <div v-if="msg.image_base64" class="message-image">
            <img :src="'data:image/png;base64,' + msg.image_base64" alt="用户图片" />
          </div>
        </div>
      </div>

      <!-- 流式输出中 -->
      <div v-if="isStreaming" class="message-row message-assistant animate-slide-up">
        <div class="message-avatar avatar-bob"><img src="/bob_logo.svg" class="bob-avatar-img" alt="Bob" /></div>
        <div class="message-body">
          <div v-if="streamThinking" class="thinking-card expanded">
            <button class="thinking-toggle">
              <ChevronDown :size="14" class="thinking-arrow" />
              <span>Thinking...</span>
            </button>
            <div class="thinking-content selectable">{{ streamThinking }}</div>
          </div>
          <!-- 工具调用状态 -->
          <div v-if="activeTools.length > 0" class="tool-calls-panel">
            <div
              v-for="(tool, idx) in activeTools"
              :key="idx"
              class="tool-call-item"
              :class="{ 'is-running': tool.status === 'running' }"
            >
              <div class="tool-call-header" @click="tool._expanded = !tool._expanded">
                <span class="tool-dot" :class="tool.status === 'running' ? 'dot-running' : 'dot-done'"></span>
                <span class="tool-name">{{ tool.name }}</span>
                <ChevronRight v-if="tool.result" :size="12" class="tool-expand-icon" :class="{ 'rotate-90': tool._expanded }" />
              </div>
              <div v-if="tool._expanded && tool.result" class="tool-result-preview">
                {{ tool.result }}
              </div>
            </div>
          </div>
          <div v-if="streamContent" class="message-content selectable" v-html="renderMarkdown(streamContent)"></div>
          <div v-if="!streamContent && !streamThinking && activeTools.length === 0" class="typing-indicator">
            <span></span><span></span><span></span>
          </div>
        </div>
      </div>
    <!-- Pending Folder Drop Card -->
    <div v-if="pendingFolderInfo" class="message-row">
      <div class="message-bubble system-bubble">
        <FolderDropCard
          :folder-path="pendingFolderInfo.path"
          :folder-name="pendingFolderInfo.name"
          :scan-result="pendingFolderInfo.scanResult"
          @confirm="confirmFolderTrack"
          @cancel="cancelFolderTrack"
        />
      </div>
    </div>
    
    <!-- Pending KB Estimate Card -->
    <div v-if="pendingKBEstimate" class="message-row">
      <div class="message-bubble system-bubble">
        <KBEstimateCard
          :folder-name="pendingKBEstimate.name"
          :estimate-result="pendingKBEstimate.result"
          @confirm="(plan) => startKBBuild(pendingKBEstimate.path, plan)"
          @cancel="cancelKBEstimate"
        />
      </div>
    </div>
    </div>

    <!-- 文件拖拽遮罩 -->
    <div
      v-if="isDragging"
      class="drop-overlay animate-fade-in"
      @dragover.prevent
      @dragleave="isDragging = false"
      @drop.prevent="handleDrop"
    >
      <div class="drop-content">
        <FileUp :size="48" class="drop-icon" />
        <span>{{ $t('chat.drop_hint_full') }}</span>
      </div>
    </div>

    <!-- 输入区 -->
    <div class="input-area">
      <div class="quick-actions-bar" v-if="inputText.trim().length > 0">
        <div class="actions-spacer"></div>
        <button
          class="btn-parse-event"
          @click="parseTextAsEvent"
          :disabled="isParsing"
        >
          <Calendar v-if="!isParsing" :size="14" />
          <Loader2 v-else :size="14" class="animate-spin" />
          <span>{{ isParsing ? $t('chat.parsing') : $t('chat.parse_event') }}</span>
        </button>
      </div>
      <div class="input-row">
        <!-- 图片预览 -->
        <div v-if="pendingImage" class="inline-image-preview">
          <img :src="'data:image/png;base64,' + pendingImage" alt="Pending Image" />
          <button class="image-remove-inline btn-icon" @click="pendingImage = null"><X :size="10" /></button>
        </div>
        <!-- 文本输入 -->
        <textarea
          ref="inputRef"
          v-model="inputText"
          class="chat-input"
          :placeholder="$t('chat.input_placeholder')"
          rows="3"
          @keydown="handleKeydown"
          @input="autoResize"
          @paste="handlePaste"
        ></textarea>
        <!-- 底部工具栏 -->
        <div class="input-toolbar">
          <button class="toolbar-item attach-btn" :title="$t('chat.attach_tooltip')" @click="handleAttach">
            <Paperclip :size="14" />
          </button>
          <!-- 模型切换器 -->
          <div class="model-switcher-wrap" v-if="currentModelName">
            <button class="toolbar-item model-indicator" @click="toggleModelSwitcher">
              <img v-if="currentModelLogo" :src="currentModelLogo" class="model-logo-sm" @error="(e) => e.target.style.display = 'none'" />
              <span>{{ currentModelName }}</span>
              <ChevronUp :size="10" class="chevron-icon" />
            </button>
            <!-- 弹出选择面板 -->
            <div v-if="showModelSwitcher" class="model-popup">
              <button
                v-for="m in availableModels"
                :key="m.id"
                class="model-option"
                :class="{ active: currentModelRaw === m.id }"
                @click="switchModel(m.id)"
              >
                <img v-if="getModelLogo(m.id)" :src="getModelLogo(m.id)" class="model-logo-sm" @error="(e) => e.target.style.display = 'none'" />
                <span class="model-option-label">{{ m.label }}</span>
              </button>
              <div v-if="availableModels.length === 0" class="model-option-empty">
                {{ $t('chat.no_models') }}
              </div>
            </div>
          </div>
          
          <!-- 代理模式切换器 -->
          <div class="model-switcher-wrap">
            <button class="toolbar-item model-indicator" @click="showAgentModeSwitcher = !showAgentModeSwitcher">
              <Shield v-if="agentMode === 'insight'" :size="12" style="color: var(--text-tertiary);" />
              <Zap v-else :size="12" style="color: var(--accent-primary);" />
              <span>{{ agentMode === 'insight' ? $t('chat.mode_qa') : $t('chat.mode_act') }}</span>
              <ChevronUp :size="10" class="chevron-icon" />
            </button>
            <div v-if="showAgentModeSwitcher" class="model-popup">
              <button class="model-option" :class="{ active: agentMode === 'insight' }" @click="agentMode = 'insight'; showAgentModeSwitcher = false">
                <Shield :size="14" style="margin-right: 8px;" />
                <span class="model-option-label">{{ $t('chat.mode_qa_desc') }}</span>
              </button>
              <button class="model-option" :class="{ active: agentMode === 'yolo' }" @click="agentMode = 'yolo'; showAgentModeSwitcher = false">
                <Zap :size="14" style="margin-right: 8px;" />
                <span class="model-option-label">{{ $t('chat.mode_act_desc') }}</span>
              </button>
            </div>
          </div>

          <!-- 全局权限开关 -->
          <label class="toolbar-item global-access-toggle" :class="{ active: globalFileAccess }" :title="$t('chat.global_access_tooltip')">
            <input type="checkbox" v-model="globalFileAccess" style="display: none;" />
            <Unlock v-if="globalFileAccess" :size="12" style="color: var(--accent-primary);" />
            <Lock v-else :size="12" style="opacity: 0.5;" />
            <span class="global-access-text">{{ $t('chat.global_access') }}</span>
          </label>

          <div class="toolbar-spacer"></div>
          <!-- 导出按钮 -->
          <button class="toolbar-item" @click="exportConversation" :title="$t('chat.export_tooltip')">
            <Download :size="13" />
          </button>
          <!-- 计费指示器 -->
          <span class="toolbar-item cost-indicator" :title="$t('chat.cost_tooltip')">
            ¥{{ sessionCost.toFixed(4) }}
          </span>
          <button
            v-if="isStreaming"
            class="action-btn stop-btn"
            @click="stopGeneration"
            :title="$t('chat.stop')"
          >
            <span class="icon-stop"></span>
          </button>
          <button
            v-else
            class="action-btn send-btn"
            :disabled="!canSend"
            @click="sendMessage"
            :title="$t('chat.send')"
          >
            <span class="icon-send"></span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script>
import { marked } from 'marked';
import hljs from 'highlight.js';
import { markedHighlight } from 'marked-highlight';
import DOMPurify from 'dompurify';

// 允许渲染 file:// 和本地磁盘路径
DOMPurify.addHook('uponSanitizeAttribute', function (node, data) {
  if (data.attrName === 'href') {
    const href = data.attrValue;
    if (href.startsWith('file://') || /^[A-Za-z]:[\\/]/.test(href)) {
      data.keepAttr = true;
      data.forceKeepAttr = true;
    }
  }
});

marked.use(markedHighlight({
  langPrefix: 'hljs language-',
  highlight(code, lang) {
    const language = hljs.getLanguage(lang) ? lang : 'plaintext';
    return hljs.highlight(code, { language }).value;
  }
}));
marked.setOptions({ breaks: true, gfm: true });
</script>

<script setup>
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue';
import { Sparkles, FileText, Camera, Calendar, User, ChevronRight, ChevronDown, ChevronUp, X, FileUp, Paperclip, Loader2, Shield, Zap, Lock, Unlock, Download } from 'lucide-vue-next';
import ConfirmCard from '../components/ConfirmCard.vue';
import FileCard from '../components/FileCard.vue';
import FolderDropCard from '../components/FolderDropCard.vue';
import KBEstimateCard from '../components/KBEstimateCard.vue';

const props = defineProps({
  conversationId: String,
});
const emit = defineEmits(['update-title']);

// ── 状态 ─────────────────────────────────────────────
const messages = ref([]);
const inputText = ref('');
const isStreaming = ref(false);
const streamContent = ref('');
const streamThinking = ref('');
const pendingImage = ref(null);
const pendingFolderInfo = ref(null);
const pendingKBEstimate = ref(null);
const isDragging = ref(false);
const logoOpacity = ref(1);
const activeTools = ref([]);
const showScrollButton = ref(false);
const isParsing = ref(false);
const messagesArea = ref(null);
const inputRef = ref(null);
const currentModelRaw = ref('');
const showModelSwitcher = ref(false);
const availableModels = ref([]);
const sessionCost = ref(0);
const globalFileAccess = ref(false);
const agentMode = ref('insight');
const showAgentModeSwitcher = ref(false);

const canSend = ref(true);

// 过滤掉系统消息（如 __rename__）
const displayMessages = computed(() => {
  return messages.value.filter(m =>
    m.role !== 'system' && !(m.content && m.content.startsWith('__rename__'))
  );
});

// ── 模型指示器 & 切换 ────────────────────────────────
function getModelLogo(modelId) {
  const name = (modelId || '').toLowerCase();
  if (name.includes('deepseek')) return '/logos/deepseek.png';
  if (name.includes('gpt') || name.includes('openai')) return '/logos/openai.png';
  if (name.includes('gemini')) return '/logos/gemini.png';
  return null;
}

const currentModelName = computed(() => {
  const found = availableModels.value.find(m => m.id === currentModelRaw.value);
  if (found) return found.label;
  return currentModelRaw.value || '';
});

const currentModelLogo = computed(() => {
  return getModelLogo(currentModelRaw.value);
});

async function toggleModelSwitcher() {
  if (!showModelSwitcher.value) {
    try {
      const models = await window.electronAPI.getModels();
      availableModels.value = models || [];
    } catch (e) {
      availableModels.value = [];
    }
  }
  showModelSwitcher.value = !showModelSwitcher.value;
}

async function switchModel(modelId) {
  await window.electronAPI.setConfig('model', modelId);
  currentModelRaw.value = modelId;
  showModelSwitcher.value = false;
}

// ── 日程解析 ─────────────────────────────────────────
async function parseTextAsEvent() {
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
    // 使用 JSON 序列化去除 Vue Proxy，否则 Electron IPC 无法 clone
    const plainEvent = JSON.parse(JSON.stringify(event));
    const res = await window.electronAPI.confirmEvent(plainEvent);
    if (res.ok) {
      msgObj.content = `已成功保存为${event.type === 'todo' ? '待办' : '日程'}：${event.title}`;
      msgObj.type = 'text'; // 将卡片转化为普通文本消息
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

// ── 流式监听 ─────────────────────────────────────────
let cleanupStreamListener = null;

onMounted(async () => {
  cleanupStreamListener = window.electronAPI.onStreamChunk(handleStreamChunk);
  loadMessages();

  // 预加载模型列表用于显示 label
  try {
    availableModels.value = await window.electronAPI.getModels() || [];
  } catch (e) { /* ignore */ }

  // 拖拽监听
  document.addEventListener('dragenter', onDragEnter);

  // Logo 滚动视差
  if (messagesArea.value) {
    messagesArea.value.addEventListener('scroll', onMessagesScroll);
    messagesArea.value.addEventListener('click', onMessageLinkClick);
  }
});

onUnmounted(() => {
  if (cleanupStreamListener) cleanupStreamListener();
  document.removeEventListener('dragenter', onDragEnter);
  if (messagesArea.value) {
    messagesArea.value.removeEventListener('scroll', onMessagesScroll);
    messagesArea.value.removeEventListener('click', onMessageLinkClick);
  }
});

// 切换对话时重新加载消息
watch(() => props.conversationId, async () => {
  loadMessages();
  sessionCost.value = 0;
  globalFileAccess.value = false;
  currentModelRaw.value = await window.electronAPI.getConfig('model') || '';
}, { immediate: true });

// ── 消息加载 ─────────────────────────────────────────
async function loadMessages() {
  if (!props.conversationId) return;
  const rawMessages = await window.electronAPI.getMessages(props.conversationId);
  messages.value = rawMessages.map(m => ({
    ...m,
    _thinkingExpanded: false,
  }));
  scrollToBottom();
}

// ── 发送消息 ─────────────────────────────────────────
async function sendMessage() {
  const text = inputText.value.trim();
  if (!text && !pendingImage.value) return;
  if (isStreaming.value) return;

  const userMessage = {
    role: 'user',
    content: text || (pendingImage.value ? '请分析这张图片' : ''),
    image_base64: pendingImage.value || null,
  };

  // 添加到 UI
  messages.value.push(userMessage);
  inputText.value = '';
  pendingImage.value = null;
  resetTextareaHeight();
  scrollToBottom();

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
      content: m.content || '',
    }));

  // 开始流式响应
  isStreaming.value = true;
  streamContent.value = '';
  streamThinking.value = '';
  activeTools.value = [];

  try {
    let result;
    console.log('[sendMessage] image_base64 present:', !!userMessage.image_base64, 'apiMessages count:', apiMessages.length);
    if (userMessage.image_base64) {
      result = await window.electronAPI.sendVision(apiMessages, userMessage.image_base64, globalFileAccess.value, agentMode.value);
    } else {
      result = await window.electronAPI.sendChat(apiMessages, globalFileAccess.value, agentMode.value);
    }
    console.log('[sendMessage] result:', JSON.stringify(result).slice(0, 300));

    // 计算本次费用
    if (result.usage && result.pricing) {
      const inputCost = (result.usage.prompt_tokens || 0) / 1_000_000 * result.pricing.input;
      const outputCost = (result.usage.completion_tokens || 0) / 1_000_000 * result.pricing.output;
      sessionCost.value += inputCost + outputCost;
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

      // 始终显示回复，即使内容为空也给出提示
      const assistantMsg = {
        role: 'assistant',
        content: finalContent || '（模型未返回内容，请检查 API 配置或重试）',
        thinking: finalThinking,
        _thinkingExpanded: false,
      };
      
      messages.value.push(assistantMsg);
      await window.electronAPI.addMessage(
        props.conversationId,
        'assistant',
        assistantMsg.content,
        null
      );
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

function handleStreamChunk(chunk) {
  if (chunk.type === 'text') {
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
    }
  }
  scrollToBottom();
}

async function stopGeneration() {
  await window.electronAPI.stopGeneration();
  isStreaming.value = false;
}

// ── 对话导出 ─────────────────────────────────────────
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

// ── Markdown 渲染 ───────────────────────────────────
function renderMarkdown(text) {
  if (!text) return '';
  // 隐藏流式输出过程中的日历块
  const cleaned = text.replace(/<calendar_event>[\s\S]*?(?:<\/calendar_event>|$)/gi, '');
  const rawHtml = marked.parse(cleaned);
  return DOMPurify.sanitize(rawHtml);
}

// ── 消息 Block 渲染（拆分文件链接为 FileCard）────────
// 正则匹配 file:// 链接或 Windows 绝对路径的 <a> 标签
const FILE_LINK_RE = /<a\s+[^>]*href="((?:file:\/\/\/[^"]+)|(?:[A-Za-z]:[\\][^"]+))"[^>]*>[^<]*<\/a>/gi;

function renderMessageBlocks(text) {
  if (!text) return [{ type: 'html', content: '' }];
  const html = renderMarkdown(text);

  // 如果没有文件链接，直接返回单个 HTML block（快速路径）
  FILE_LINK_RE.lastIndex = 0;
  if (!FILE_LINK_RE.test(html)) {
    return [{ type: 'html', content: html }];
  }

  // 拆分 HTML 为 text blocks 和 file-card blocks
  const blocks = [];
  let lastIndex = 0;
  FILE_LINK_RE.lastIndex = 0;
  let match;

  while ((match = FILE_LINK_RE.exec(html)) !== null) {
    // 匹配前的 HTML 文本
    if (match.index > lastIndex) {
      blocks.push({ type: 'html', content: html.slice(lastIndex, match.index) });
    }
    // 文件卡片 block
    let filePath = match[1];
    // 清理 file:/// 前缀
    if (filePath.startsWith('file:///')) {
      filePath = filePath.replace('file:///', '');
    }
    try { filePath = decodeURIComponent(filePath); } catch(e) {}
    // 把正斜杠转成反斜杠（Windows 路径）
    filePath = filePath.replace(/\//g, '\\');
    blocks.push({ type: 'file', path: filePath });
    lastIndex = match.index + match[0].length;
  }

  // 剩余的 HTML 文本
  if (lastIndex < html.length) {
    blocks.push({ type: 'html', content: html.slice(lastIndex) });
  }

  return blocks;
}

// ── 附件/图片处理 ────────────────────────────────────
async function handleAttach() {
  try {
    const result = await window.electronAPI.selectFile();
    if (!result) return; // User cancelled

    // New structured return: { name, type, content }
    if (typeof result === 'object' && result.type === 'image' && result.content) {
      pendingImage.value = result.content;
      return;
    }
    if (typeof result === 'object' && result.type === 'text' && result.content) {
      inputText.value = `请分析以下文件内容 (${result.name}):\n\n${result.content}`;
      return;
    }
    // Old fallback: result is just a file path string (shouldn't happen now)
    if (typeof result === 'string') {
      inputText.value = `用户选择了文件: ${result}`;
      return;
    }
  } catch (e) {
    console.error('[handleAttach]', e);
  }

  // Fallback: try clipboard image
  const base64 = await window.electronAPI.getClipboardImage();
  if (base64) {
    pendingImage.value = base64;
  }
}



async function pasteImage() {
  const base64 = await window.electronAPI.getClipboardImage();
  if (base64) {
    pendingImage.value = base64;
  }
}

function handlePaste(event) {
  const items = event.clipboardData?.items;
  if (!items) return;

  for (const item of items) {
    if (item.type.startsWith('image/')) {
      event.preventDefault();
      const file = item.getAsFile();
      const reader = new FileReader();
      reader.onload = (e) => {
        const base64 = e.target.result.replace(/^data:image\/\w+;base64,/, '');
        pendingImage.value = base64;
      };
      reader.readAsDataURL(file);
      return;
    }
  }
}

// ── 文件拖拽 ────────────────────────────────────────
function onDragEnter(e) {
  if (e.dataTransfer?.types?.includes('Files')) {
    isDragging.value = true;
  }
}

// ── Logo 滚动视差 ────────────────────────────────────
function onMessagesScroll() {
  const el = messagesArea.value;
  if (!el) return;
  // 在 0~100px 滚动范围内，从 1 平滑衰减到 0
  const scrollY = el.scrollTop;
  const fadeDistance = 100;
  logoOpacity.value = Math.max(0, 1 - scrollY / fadeDistance);
}

// ── 拦截消息链接点击 ─────────────────────────────────
function onMessageLinkClick(e) {
  const a = e.target.closest('a');
  if (!a || !a.href) return;
  
  e.preventDefault();
  const href = a.getAttribute('href');
  
  if (href.startsWith('file://') || /^[A-Za-z]:[\\/]/.test(href)) {
    let filePath = href.replace('file:///', '');
    // 兼容可能残留的 url 编码
    try { filePath = decodeURIComponent(filePath); } catch(e){}
    window.electronAPI.openFile(filePath).catch(err => {
      console.error('打开文件失败:', err);
    });
  } else {
    // 调用系统浏览器打开其他网址 (如果没实现openExternal，可以以后再加)
    if (window.electronAPI.openExternal) {
      window.electronAPI.openExternal(href);
    }
  }
}

async function handleDrop(event) {
  isDragging.value = false;
  const files = event.dataTransfer?.files;
  if (!files || files.length === 0) return;

  const file = files[0];
  const filePath = window.electronAPI.getFilePath ? window.electronAPI.getFilePath(file) : file.path;

  // ── 路由 1: 文件夹 → 弹确认卡 (P0 流) ──
  if (filePath) {
    try {
      const meta = await window.electronAPI.getFileMeta(filePath);
      if (meta && meta.isDir) {
        // 零成本扫描文件夹结构
        const scanResult = await window.electronAPI.scanFolder(filePath);
        if (scanResult && !scanResult.error) {
           pendingFolderInfo.value = {
             path: filePath,
             name: meta.name,
             scanResult
           };
           scrollToBottom();
           return;
        } else {
           inputText.value = `文件夹扫描失败: ${scanResult?.message || '未知错误'}`;
           return;
        }
      }
    } catch (err) {
      console.warn("Failed to check file meta", err);
    }
  }

  // ── 路由 2: 图片 → Vision 附件 ──
  if (file.type.startsWith('image/')) {
    const reader = new FileReader();
    reader.onload = (e) => {
      const base64 = e.target.result.replace(/^data:image\/\w+;base64,/, '');
      pendingImage.value = base64;
    };
    reader.readAsDataURL(file);
    return;
  }

  // ── 路由 3: 文档/其他文件 → 上下文附件 ──
  if (!filePath) {
    inputText.value = `文件处理失败: 无法获取文件的本地路径。`;
    return;
  }

  try {
    const result = await window.electronAPI.readFile(filePath);
    if (result.error) {
      inputText.value = `文件读取失败: ${result.error}`;
    } else {
      inputText.value = `请分析以下文件内容 (${result.name}):\n\n${result.content}`;
    }
  } catch (err) {
    inputText.value = `文件处理失败: ${err.message}`;
  }
}

// ── 输入辅助 ────────────────────────────────────────
function handleKeydown(event) {
  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault();
    sendMessage();
  }
}

function autoResize() {
  const textarea = inputRef.value;
  if (!textarea) return;
  textarea.style.height = 'auto';
  textarea.style.height = Math.min(textarea.scrollHeight, 160) + 'px';
}

function resetTextareaHeight() {
  nextTick(() => {
    if (inputRef.value) {
      inputRef.value.style.height = 'auto';
    }
  });
}

function insertPrompt(text) {
  inputText.value = text;
  nextTick(() => inputRef.value?.focus());
}

function scrollToBottom() {
  nextTick(() => {
    if (messagesArea.value) {
      messagesArea.value.scrollTop = messagesArea.value.scrollHeight;
    }
  });
}

// ── 文件夹处理 ──────────────────────────────────────
function cancelFolderTrack() {
  pendingFolderInfo.value = null;
}

async function confirmFolderTrack() {
  const folder = pendingFolderInfo.value;
  pendingFolderInfo.value = null;
  if (!folder) return;

  // 添加用户消息
  messages.value.push({ role: 'user', content: `我已经将文件夹「${folder.name}」拖入。` });
  
  // 插入系统处理中状态消息
  const systemMsgId = Date.now().toString();
  const systemMsg = { 
    id: systemMsgId, 
    role: 'assistant', 
    content: '正在处理文件夹，请稍候...' 
  };
  messages.value.push(systemMsg);
  scrollToBottom();

  try {
    // 实际调用内置工具 API 来处理 (这最终会调用 folderTracker.trackFolder)
    const result = await window.electronAPI.sendChat([{
      role: 'user',
      content: `Please execute the "track_folder" tool on this path: ${folder.path}`
    }], globalFileAccess.value, agentMode.value);

    // 替换为简短成功提示
    const index = messages.value.findIndex(m => m.id === systemMsgId);
    if (index !== -1) {
       messages.value[index].content = `✅ 已将「${folder.name}」收藏到目录列表。`;
    }
    
    // 显示预估卡片
    pendingKBEstimate.value = {
      name: folder.name,
      path: folder.path,
      result: null // null 表示 loading
    };
    scrollToBottom();
    
    // 异步获取预估结果
    const estimateResult = await window.electronAPI.estimateKB(folder.path);
    if (pendingKBEstimate.value && pendingKBEstimate.value.path === folder.path) {
       pendingKBEstimate.value.result = estimateResult;
    }

  } catch (err) {
    const index = messages.value.findIndex(m => m.id === systemMsgId);
    if (index !== -1) {
       messages.value[index].content = `❌ 文件夹收藏失败: ${err.message}`;
    }
  }
}

function cancelKBEstimate() {
  pendingKBEstimate.value = null;
}

function startKBBuild(folderPath, plan) {
  pendingKBEstimate.value = null;
  messages.value.push({ role: 'user', content: `请使用 ${plan === 'cheap' ? '基础模型' : '核心模型'} 为这个文件夹搭建语义知识库。` });
  
  // 这会触发 LLM 进而调用 kb_convert -> kb_index
  inputText.value = `Please use the "kb_convert" and then "kb_index" tools (or the semantic index skill) to build a knowledge base for: ${folderPath}. I prefer the ${plan} model plan.`;
  sendMessage();
}
</script>

<style scoped>
.chat-view {
  flex: 1;
  min-width: 0;
  height: 100%;
  display: flex;
  flex-direction: column;
  position: relative;
}

/* ── 消息区域 ───────────────────────────────────────── */
.messages-area {
  flex: 1;
  overflow-y: auto;
  padding: var(--space-6) var(--space-8);
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.view-header {
  position: sticky;
  top: 0;
  z-index: 10;
  text-align: left;
  padding-bottom: var(--space-2);
  pointer-events: none;
}

.view-title {
  display: flex;
  align-items: center;
  height: 36px;
}

.title-bob-logo {
  height: 24px;
  width: auto;
  filter: var(--logo-filter);
}

/* ── 空状态 ─────────────────────────────────────────── */
.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-end;
  padding-bottom: 0;
  width: 100%;
}

.empty-logo-wrapper {
  max-width: 1000px;
  width: 100%;
  margin: 0 auto;
  display: flex;
  justify-content: center;
  pointer-events: none;
}

.empty-bob-logo {
  width: 100%;
  height: auto;
  opacity: 0.05;
  filter: var(--logo-filter);
  display: block;
}

/* ── 消息行（聊天气泡布局）─────────────────────────── */
.message-row {
  display: flex;
  gap: var(--space-2);
  max-width: 1000px;
  width: 100%;
  margin: 0 auto;
  align-items: flex-start;
}

/* 用户消息：头像在左，内容靠左 */
.message-user {
  flex-direction: row;
}

/* Bob 消息：头像在右，内容靠右 */
.message-assistant {
  flex-direction: row-reverse;
}

/* 头像 */
.message-avatar {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-sm);
  flex-shrink: 0;
  margin-top: 4px;
  font-size: 12px;
  font-weight: 600;
}

.avatar-user {
  background: var(--surface-input);
  color: var(--text-secondary);
}

.avatar-bob {
  background: var(--surface-glass);
  border: 1px solid var(--border-subtle);
}

.bob-avatar-img {
  width: 60%;
  height: 60%;
  object-fit: contain;
  filter: var(--logo-filter);
}

/* 内容块：最宽占 80%，文字始终左对齐 */
.message-body {
  max-width: 80%;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
  text-align: left;
}

.message-content {
  padding: var(--space-2) 0;
  line-height: 1.6;
  word-break: break-word;
}

/* 用户消息稍暗 */
.message-user .message-content {
  color: var(--text-secondary);
}

/* Bob 回复正常亮度 */
.message-assistant .message-content {
  color: var(--text-primary);
}

.message-content :deep(code) {
  font-family: var(--font-mono);
  font-size: 0.9em;
  padding: 2px 6px;
  border-radius: 4px;
  background: var(--bg-code-inline);
}

.message-content :deep(strong) {
  font-weight: 600;
  color: var(--text-primary);
}

/* ── 思维链折叠 ─────────────────────────────────────── */
.thinking-card {
  margin-top: var(--space-2);
  border-radius: 0;
  border: none;
  background: transparent;
  border-left: 2px solid var(--border-subtle);
  margin-left: 2px;
}

.thinking-toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: auto;
  padding: var(--space-1) var(--space-3);
  border: none;
  background: transparent;
  color: var(--text-muted);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  cursor: pointer;
  transition: color var(--duration-fast);
}

.thinking-arrow {
  transition: transform var(--duration-fast);
}

.thinking-arrow.expanded {
  transform: rotate(90deg);
}

.thinking-toggle:hover {
  color: var(--text-secondary);
}

.thinking-content {
  padding: var(--space-2) var(--space-4);
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  line-height: 1.6;
  white-space: pre-wrap;
  max-height: 300px;
  overflow-y: auto;
}

/* ── 文件卡片 ───────────────────────────────────────── */
.message-content a[href^="file://"],
.message-content a[href^="C:\\"],
.message-content a[href^="D:\\"],
.message-content a[href^="E:\\"],
.message-content a[href^="F:\\"] {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  background: color-mix(in srgb, var(--accent-primary) 10%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent-primary) 30%, transparent);
  border-radius: 6px;
  text-decoration: none;
  color: var(--accent-primary);
  font-weight: 500;
  font-size: 13px;
  margin: 4px 0;
  transition: all 0.2s;
}
.message-content a[href^="file://"]:hover,
.message-content a[href^="C:\\"]:hover,
.message-content a[href^="D:\\"]:hover,
.message-content a[href^="E:\\"]:hover,
.message-content a[href^="F:\\"]:hover {
  background: color-mix(in srgb, var(--accent-primary) 20%, transparent);
}
.message-content a[href^="file://"]::before,
.message-content a[href^="C:\\"]::before,
.message-content a[href^="D:\\"]::before,
.message-content a[href^="E:\\"]::before,
.message-content a[href^="F:\\"]::before {
  content: "";
  display: inline-block;
  width: 14px;
  height: 14px;
  background-image: url('data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="%236366f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline></svg>');
  background-size: cover;
}

/* ── 消息图片 ───────────────────────────────────────── */
.message-image {
  border-radius: var(--radius-md);
  overflow: hidden;
}

.message-image img {
  width: auto;
  height: auto;
  max-width: 300px;
  max-height: 240px;
  display: block;
  object-fit: contain;
  border-radius: var(--radius-sm);
}

/* ── 图片预览条 ─────────────────────────────────────── */
.image-preview-bar {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-8);
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-subtle);
}

.image-preview-thumb {
  position: relative;
  width: 48px;
  height: 48px;
  border-radius: var(--radius-sm);
  overflow: hidden;
  border: 1px solid var(--border-default);
}

.image-preview-thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.image-remove {
  position: absolute;
  top: -4px;
  right: -4px;
  width: 20px;
  height: 20px;
  font-size: 10px;
  background: var(--color-error);
  color: white;
  border-radius: 50%;
}

.image-hint {
  font-size: var(--text-sm);
  color: var(--text-tertiary);
}

/* ── 错误卡片 ─────────────────────────────────────── */
.error-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 16px;
  background: color-mix(in srgb, var(--color-error, #ef4444) 8%, var(--bg-secondary));
  border: 1px solid color-mix(in srgb, var(--color-error, #ef4444) 20%, transparent);
  border-radius: 8px;
  margin: 4px 0;
}

.error-icon {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  background: var(--color-error, #ef4444);
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 800;
  font-size: 13px;
  flex-shrink: 0;
  line-height: 1;
}

.error-title {
  font-weight: 600;
  font-size: 13px;
  color: var(--text-primary);
  margin-bottom: 2px;
}

.error-detail {
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.5;
  word-break: break-word;
}

/* ── 工具调用面板 ─────────────────────────────────────── */
.tool-calls-panel {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 8px 0;
}

.tool-call-item {
  border-left: 2px solid var(--border-subtle);
  padding: 4px 0 4px 12px;
  font-size: 12px;
  transition: border-color 0.2s ease;
}

.tool-call-item.is-running {
  border-left-color: var(--accent-primary);
}

.tool-call-header {
  display: flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  user-select: none;
}

.tool-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-running {
  background: transparent;
  border: 1.5px solid var(--accent-primary);
  animation: dot-pulse 1.2s ease-in-out infinite;
}

.dot-done {
  background: var(--accent-primary);
  border: 1.5px solid var(--accent-primary);
}

@keyframes dot-pulse {
  0%, 100% { opacity: 0.4; }
  50% { opacity: 1; }
}

.tool-name {
  color: var(--text-secondary);
  font-family: var(--font-mono, monospace);
  font-size: 12px;
}

.tool-expand-icon {
  color: var(--text-tertiary);
  transition: transform 0.15s ease;
  flex-shrink: 0;
  margin-left: auto;
}

.tool-expand-icon.rotate-90 {
  transform: rotate(90deg);
}

.tool-result-preview {
  margin-top: 6px;
  padding: 8px 10px;
  background: var(--bg-secondary);
  border-radius: 6px;
  font-size: 11px;
  font-family: var(--font-mono, monospace);
  color: var(--text-tertiary);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 120px;
  overflow-y: auto;
  line-height: 1.4;
}

/* ── 拖拽遮罩 ───────────────────────────────────────── */
.drop-overlay {
  position: absolute;
  inset: 0;
  background: color-mix(in srgb, var(--bg-primary) 80%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent-primary) 30%, transparent),
              inset 0 0 50px color-mix(in srgb, var(--accent-primary) 20%, transparent);
  backdrop-filter: blur(4px);
}

.drop-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-3);
  color: var(--accent-tertiary);
  font-size: var(--text-xl);
}

.drop-icon {
  font-size: 3rem;
}

/* ── 聊天头部 ───────────────────────────────────────── */
.view-header {
  max-width: 1000px;
  width: 100%;
  margin: 0 auto;
  padding: 0 0 var(--space-6) 0;
  text-align: left;
}

.view-title {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-2xl);
  font-weight: 600;
  color: var(--text-primary);
}

/* ── 输入区 ─────────────────────────────────────────── */
.quick-actions-bar {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  margin-bottom: var(--space-2);
  max-width: 1000px;
  margin-left: auto;
  margin-right: auto;
}

.actions-spacer {
  flex: 1;
}

.model-switcher-wrap {
  position: relative;
}

/* ── 统一工具栏项基线 ────────────────────────────── */
.toolbar-item {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 6px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  font-size: var(--text-xs);
  font-family: var(--font-sans);
  cursor: pointer;
  transition: all var(--duration-fast);
  white-space: nowrap;
  flex-shrink: 0;
}

.toolbar-item:hover {
  color: var(--text-secondary);
  background: var(--surface-glass);
}

.chevron-icon {
  opacity: 0.5;
}

.model-logo-sm {
  width: 12px;
  height: 12px;
  object-fit: contain;
  border-radius: 2px;
}

/* ── 模型切换弹窗 ─────────────────────────────────── */
.model-popup {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  min-width: 200px;
  background: var(--bg-primary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
  padding: var(--space-1);
  z-index: 200;
}

.model-option {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 10px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  text-align: left;
  cursor: pointer;
  transition: all var(--duration-fast);
}

.model-option:hover {
  background: var(--surface-glass);
  color: var(--text-primary);
}

.model-option.active {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.model-option-label {
  font-weight: 500;
}

.model-option-id {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  font-family: var(--font-mono);
}

.model-option-empty {
  padding: 8px 10px;
  color: var(--text-tertiary);
  font-size: var(--text-xs);
  text-align: center;
}

.btn-parse-event {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  background: transparent;
  color: var(--text-secondary);
  border: none;
  padding: 4px 12px;
  font-size: var(--text-xs);
  cursor: pointer;
  transition: color 0.2s;
  margin-left: auto;
}

.btn-parse-event:hover:not(:disabled) {
  color: var(--text-primary);
}

.btn-parse-event:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.input-area {
  padding: var(--space-4) var(--space-8) var(--space-6);
}

.input-row {
  display: flex;
  flex-direction: column;
  max-width: 1000px;
  width: 100%;
  margin: 0 auto;
  box-sizing: border-box;
  background: var(--surface-card);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  padding: var(--space-2) var(--space-3);
  transition: border-color var(--duration-fast) var(--ease-out);
}

.input-row:focus-within {
  border-color: var(--border-default);
}

/* ── 内联图片预览 ─────────────────────────────────── */
.inline-image-preview {
  position: relative;
  display: inline-block;
  padding: 4px 0 6px 0;
}

.inline-image-preview img {
  width: auto;
  height: auto;
  max-width: 64px;
  max-height: 48px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-subtle);
  object-fit: contain;
}

.image-remove-inline {
  position: absolute;
  top: 0;
  right: -8px;
  width: 16px;
  height: 16px;
  font-size: 8px;
  background: var(--color-error);
  color: white;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  border: none;
}

.chat-input {
  width: 100%;
  border: none;
  background: transparent;
  color: var(--text-primary);
  font-family: var(--font-sans);
  font-size: var(--text-base);
  line-height: var(--leading-normal);
  resize: none;
  outline: none;
  padding: var(--space-1) 0;
  min-height: 56px;
  max-height: 160px;
}

.chat-input::placeholder {
  color: var(--text-tertiary);
}

/* ── 底部工具栏 ───────────────────────────────────── */
.input-toolbar {
  display: flex;
  align-items: center;
  gap: 2px;
  padding-top: var(--space-1);
}

.toolbar-spacer {
  flex: 1;
}

.attach-btn:hover {
  color: var(--text-primary);
}

/* ── 发送/停止按钮（统一方形） ────────────────────── */
.action-btn {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 17px;
  height: 17px;
  border-radius: 2px;
  border: 1px solid var(--border-default);
  background: transparent;
  cursor: pointer;
  transition: all var(--duration-fast);
}

.action-btn:hover {
  border-color: var(--text-secondary);
}

.send-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

/* CSS 实心三角形 ▶ */
.icon-send {
  display: block;
  width: 0;
  height: 0;
  border-style: solid;
  border-width: 3.5px 0 3.5px 6px;
  border-color: transparent transparent transparent var(--text-primary);
  margin-left: 1px;
}

.send-btn:disabled .icon-send {
  border-left-color: var(--text-tertiary);
}

/* 红色实心方块 ■ */
.icon-stop {
  display: block;
  width: 5px;
  height: 5px;
  border-radius: 0.5px;
  background: var(--color-error);
}

.stop-btn {
  border-color: var(--border-default);
}

.stop-btn:hover {
  border-color: var(--text-secondary);
  background: var(--color-error-bg);
}

/* ── 计费指示器 ───────────────────────────────────── */
.cost-indicator {
  font-family: var(--font-mono);
  cursor: default;
}

/* ── 全局权限开关 ─────────────────────────────────── */
.global-access-toggle {
  opacity: 0.6;
}

.global-access-toggle:hover {
  opacity: 1;
}

.global-access-toggle.active {
  opacity: 1;
}

.global-access-text {
  font-size: inherit;
  color: inherit;
  font-weight: 500;
}
.global-access-toggle.active .global-access-text {
  color: var(--accent-primary);
}
</style>
