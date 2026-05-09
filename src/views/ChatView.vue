<template>
  <div class="chat-view">
    <!-- 消息区域 -->
    <div class="messages-area" ref="messagesArea">
      <!-- 空状态 -->
      <div v-if="messages.length === 0" class="empty-state animate-fade-in">
        <div class="empty-icon">✨</div>
        <h2 class="empty-title">你好，有什么可以帮你的？</h2>
        <p class="empty-subtitle">对话、粘贴图片、拖入文件 — 我都能处理</p>
        <div class="quick-actions">
          <button class="quick-action card" @click="insertPrompt('帮我总结一下这段文字')">
            📝 文字总结
          </button>
          <button class="quick-action card" @click="insertPrompt('帮我分析这张图片')">
            📸 图片分析
          </button>
          <button class="quick-action card" @click="insertPrompt('下周三下午3点和李总开会')">
            📅 创建日程
          </button>
        </div>
      </div>

      <!-- 消息列表 -->
      <div
        v-for="(msg, idx) in messages"
        :key="msg.id || idx"
        class="message-row animate-slide-up"
        :class="[`message-${msg.role}`]"
      >
        <div class="message-avatar">
          {{ msg.role === 'user' ? '👤' : '🤖' }}
        </div>
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
              <span class="thinking-icon">💭</span>
              <span>思考过程</span>
              <span class="thinking-arrow">{{ msg._thinkingExpanded ? '▾' : '▸' }}</span>
            </button>
            <div v-if="msg._thinkingExpanded" class="thinking-content selectable">
              {{ msg.thinking }}
            </div>
          </div>
          <!-- 消息内容 -->
          <div class="message-content selectable" v-html="renderMarkdown(msg.content)"></div>
          <!-- 图片预览 -->
          <div v-if="msg.image_base64" class="message-image">
            <img :src="'data:image/png;base64,' + msg.image_base64" alt="用户图片" />
          </div>
        </div>
      </div>

      <!-- 流式输出中 -->
      <div v-if="isStreaming" class="message-row message-assistant animate-slide-up">
        <div class="message-avatar">🤖</div>
        <div class="message-body">
          <div v-if="streamThinking" class="thinking-card expanded">
            <button class="thinking-toggle">
              <span class="thinking-icon">💭</span>
              <span>思考中...</span>
            </button>
            <div class="thinking-content selectable">{{ streamThinking }}</div>
          </div>
          <div v-if="streamContent" class="message-content selectable" v-html="renderMarkdown(streamContent)"></div>
          <div v-if="!streamContent && !streamThinking" class="typing-indicator">
            <span></span><span></span><span></span>
          </div>
        </div>
      </div>
    </div>

    <!-- 图片预览条 -->
    <div v-if="pendingImage" class="image-preview-bar animate-slide-up">
      <div class="image-preview-thumb">
        <img :src="'data:image/png;base64,' + pendingImage" alt="待发送图片" />
        <button class="image-remove btn-icon" @click="pendingImage = null">✕</button>
      </div>
      <span class="image-hint">图片已就绪，输入描述后发送</span>
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
        <span class="drop-icon">📁</span>
        <span>松开以分析文件</span>
      </div>
    </div>

    <!-- 输入区 -->
    <div class="input-area">
      <div class="quick-actions-bar" v-if="inputText.trim().length > 0">
        <button class="btn-parse-event" @click="parseTextAsEvent" :disabled="isParsing">
          {{ isParsing ? '⏳ 解析中...' : '📅 解析为日程' }}
        </button>
      </div>
      <div class="input-row">
        <button class="btn-icon" title="粘贴图片 (Ctrl+V)" @click="pasteImage">📎</button>
        <textarea
          ref="inputRef"
          v-model="inputText"
          class="chat-input"
          placeholder="输入消息... (Ctrl+V 粘贴图片)"
          rows="1"
          @keydown="handleKeydown"
          @input="autoResize"
          @paste="handlePaste"
        ></textarea>
        <button
          v-if="isStreaming"
          class="btn btn-ghost stop-btn"
          @click="stopGeneration"
        >
          ⏹ 停止
        </button>
        <button
          v-else
          class="btn btn-primary send-btn"
          :disabled="!canSend"
          @click="sendMessage"
        >
          发送
        </button>
      </div>
    </div>
  </div>
</template>

<script>
import { marked } from 'marked';
import hljs from 'highlight.js';
import { markedHighlight } from 'marked-highlight';

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
import { ref, watch, onMounted, onUnmounted, nextTick, defineProps, defineEmits } from 'vue';
import ConfirmCard from '../components/ConfirmCard.vue';

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
const isDragging = ref(false);
const isParsing = ref(false);
const messagesArea = ref(null);
const inputRef = ref(null);

const canSend = ref(true);

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
    messages.value.push({ role: 'assistant', content: `⚠️ 解析日程失败: ${err.message}` });
  } finally {
    isParsing.value = false;
    inputText.value = '';
    resetTextareaHeight();
  }
}

async function handleConfirmEvent(event, msgObj) {
  try {
    const res = await window.electronAPI.confirmEvent(event);
    if (res.ok) {
      msgObj.content = `✅ 已成功保存为${event.type === 'todo' ? '待办' : '日程'}：${event.title}`;
      msgObj.type = 'text'; // 将卡片转化为普通文本消息
    } else {
      msgObj.content = `⚠️ 保存失败: ${res.error}`;
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

onMounted(() => {
  cleanupStreamListener = window.electronAPI.onStreamChunk(handleStreamChunk);
  loadMessages();

  // 拖拽监听
  document.addEventListener('dragenter', onDragEnter);
});

onUnmounted(() => {
  if (cleanupStreamListener) cleanupStreamListener();
  document.removeEventListener('dragenter', onDragEnter);
});

// 切换对话时重新加载消息
watch(() => props.conversationId, () => {
  loadMessages();
});

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
    .filter(m => m.role !== 'system')
    .map(m => ({
      role: m.role,
      content: m.content,
    }));

  // 开始流式响应
  isStreaming.value = true;
  streamContent.value = '';
  streamThinking.value = '';

  try {
    let result;
    if (userMessage.image_base64) {
      result = await window.electronAPI.sendVision(apiMessages, userMessage.image_base64);
    } else {
      result = await window.electronAPI.sendChat(apiMessages);
    }

    if (result.error) {
      messages.value.push({
        role: 'assistant',
        content: `⚠️ ${result.error}`,
        _thinkingExpanded: false,
      });
    } else {
      // 流式完成 — 结果已通过 chunk 推送
      const assistantMsg = {
        role: 'assistant',
        content: streamContent.value || result.content,
        thinking: streamThinking.value || result.thinking || null,
        _thinkingExpanded: false,
      };
      messages.value.push(assistantMsg);

      // 持久化
      await window.electronAPI.addMessage(
        props.conversationId,
        'assistant',
        assistantMsg.content,
        null
      );
    }
  } catch (err) {
    messages.value.push({
      role: 'assistant',
      content: `⚠️ 发生错误: ${err.message}`,
      _thinkingExpanded: false,
    });
  } finally {
    isStreaming.value = false;
    streamContent.value = '';
    streamThinking.value = '';
    scrollToBottom();
  }
}

function handleStreamChunk(chunk) {
  if (chunk.type === 'text') {
    streamContent.value += chunk.content;
  } else if (chunk.type === 'thinking') {
    streamThinking.value += chunk.content;
  }
  scrollToBottom();
}

async function stopGeneration() {
  await window.electronAPI.stopGeneration();
  isStreaming.value = false;
}

// ── Markdown 渲染 ───────────────────────────────────
function renderMarkdown(text) {
  if (!text) return '';
  return marked.parse(text);
}

// ── 图片处理 ────────────────────────────────────────
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

async function handleDrop(event) {
  isDragging.value = false;
  const files = event.dataTransfer?.files;
  if (!files || files.length === 0) return;

  const file = files[0];

  // 图片文件 → Vision
  if (file.type.startsWith('image/')) {
    const reader = new FileReader();
    reader.onload = (e) => {
      const base64 = e.target.result.replace(/^data:image\/\w+;base64,/, '');
      pendingImage.value = base64;
    };
    reader.readAsDataURL(file);
    return;
  }

  // 其他文件 → FileReader
  try {
    const result = await window.electronAPI.readFile(file.path);
    if (result.error) {
      inputText.value = `⚠️ 文件读取失败: ${result.error}`;
    } else {
      inputText.value = `请分析以下文件内容 (${result.name}):\n\n${result.content}`;
    }
  } catch (err) {
    inputText.value = `⚠️ 文件处理失败: ${err.message}`;
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
</script>

<style scoped>
.chat-view {
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

/* ── 空状态 ─────────────────────────────────────────── */
.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-3);
}

.empty-icon {
  font-size: 3rem;
  margin-bottom: var(--space-2);
}

.empty-title {
  font-size: var(--text-2xl);
  font-weight: 600;
  background: var(--gradient-brand);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}

.empty-subtitle {
  color: var(--text-tertiary);
  font-size: var(--text-base);
}

.quick-actions {
  display: flex;
  gap: var(--space-3);
  margin-top: var(--space-6);
}

.quick-action {
  padding: var(--space-3) var(--space-5);
  font-size: var(--text-sm);
  color: var(--text-secondary);
  cursor: pointer;
  transition: all var(--duration-normal) var(--ease-out);
  border: 1px solid var(--border-subtle);
  background: var(--surface-card);
  font-family: var(--font-sans);
  border-radius: var(--radius-lg);
}

.quick-action:hover {
  border-color: var(--accent-primary);
  color: var(--text-primary);
  transform: translateY(-2px);
  box-shadow: var(--shadow-glow);
}

/* ── 消息行 ─────────────────────────────────────────── */
.message-row {
  display: flex;
  gap: var(--space-3);
  max-width: 800px;
  width: 100%;
  margin: 0 auto;
}

.message-user {
  flex-direction: row-reverse;
}

.message-avatar {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-sm);
  background: var(--surface-card);
  font-size: var(--text-lg);
  flex-shrink: 0;
}

.message-user .message-avatar {
  background: var(--gradient-subtle);
}

.message-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.message-content {
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-lg);
  line-height: var(--leading-relaxed);
  word-break: break-word;
}

.message-user .message-content {
  background: var(--gradient-subtle);
  border: 1px solid rgba(99, 102, 241, 0.15);
}

.message-assistant .message-content {
  background: var(--surface-card);
  border: 1px solid var(--border-subtle);
}

.message-content :deep(code) {
  font-family: var(--font-mono);
  font-size: 0.9em;
  padding: 2px 6px;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.08);
}

.message-content :deep(strong) {
  font-weight: 600;
  color: var(--text-primary);
}

/* ── 思维链折叠 ─────────────────────────────────────── */
.thinking-card {
  border-radius: var(--radius-md);
  border: 1px solid rgba(251, 191, 36, 0.15);
  background: rgba(251, 191, 36, 0.05);
  overflow: hidden;
}

.thinking-toggle {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: var(--space-2) var(--space-3);
  border: none;
  background: transparent;
  color: var(--color-warning);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  cursor: pointer;
  opacity: 0.8;
}

.thinking-toggle:hover {
  opacity: 1;
}

.thinking-content {
  padding: 0 var(--space-3) var(--space-3);
  font-size: var(--text-sm);
  color: var(--text-tertiary);
  line-height: var(--leading-relaxed);
  white-space: pre-wrap;
  max-height: 300px;
  overflow-y: auto;
}

/* ── 消息图片 ───────────────────────────────────────── */
.message-image {
  border-radius: var(--radius-md);
  overflow: hidden;
  max-width: 300px;
}

.message-image img {
  width: 100%;
  height: auto;
  display: block;
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

/* ── 拖拽遮罩 ───────────────────────────────────────── */
.drop-overlay {
  position: absolute;
  inset: 0;
  background: rgba(10, 10, 15, 0.85);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
  border: 3px dashed var(--accent-primary);
  border-radius: var(--radius-lg);
  margin: var(--space-4);
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

/* ── 输入区 ─────────────────────────────────────────── */
.quick-actions-bar {
  display: flex;
  justify-content: flex-end;
  margin-bottom: var(--space-2);
  max-width: 800px;
  margin-left: auto;
  margin-right: auto;
}

.btn-parse-event {
  background: var(--surface-glass);
  color: var(--accent-tertiary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  padding: 4px 12px;
  font-size: var(--text-sm);
  cursor: pointer;
  transition: all 0.2s;
}

.btn-parse-event:hover:not(:disabled) {
  background: var(--surface-hover);
  border-color: var(--accent-primary);
}

.btn-parse-event:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.input-area {
  padding: var(--space-4) var(--space-8) var(--space-6);
  border-top: 1px solid var(--border-subtle);
  background: var(--bg-primary);
}

.input-row {
  display: flex;
  align-items: flex-end;
  gap: var(--space-2);
  max-width: 800px;
  margin: 0 auto;
  background: var(--surface-input);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  padding: var(--space-2);
  transition: border-color var(--duration-fast) var(--ease-out);
}

.input-row:focus-within {
  border-color: var(--accent-primary);
  box-shadow: 0 0 0 3px var(--accent-glow);
}

.chat-input {
  flex: 1;
  border: none;
  background: transparent;
  color: var(--text-primary);
  font-family: var(--font-sans);
  font-size: var(--text-base);
  line-height: var(--leading-normal);
  resize: none;
  outline: none;
  padding: var(--space-2);
  max-height: 160px;
}

.chat-input::placeholder {
  color: var(--text-tertiary);
}

.send-btn {
  flex-shrink: 0;
  padding: var(--space-2) var(--space-5);
}

.send-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.stop-btn {
  flex-shrink: 0;
  color: var(--color-error);
  border-color: var(--color-error);
}
</style>
