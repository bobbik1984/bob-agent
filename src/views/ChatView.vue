<template>
  <div class="chat-view">
    <!-- 消息区域 -->
    <div class="messages-area" ref="messagesArea">
      <!-- 空状态 -->
      <div v-if="messages.length === 0" class="empty-state animate-fade-in">
        <Sparkles :size="48" class="empty-icon" />
        <h2 class="empty-title">你好，有什么可以帮你的？</h2>
        <p class="empty-subtitle">对话、粘贴图片、拖入文件 — 我都能处理</p>
        <div class="quick-actions">
          <button class="quick-action card" @click="insertPrompt('帮我总结一下这段文字')">
            <FileText :size="16" /> 文字总结
          </button>
          <button class="quick-action card" @click="insertPrompt('帮我分析这张图片')">
            <Camera :size="16" /> 图片分析
          </button>
          <button class="quick-action card" @click="insertPrompt('下周三下午3点和李总开会')">
            <Calendar :size="16" /> 创建日程
          </button>
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
          <span v-else class="bob-avatar">B</span>
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
        <div class="message-avatar avatar-bob"><span class="bob-avatar">B</span></div>
        <div class="message-body">
          <div v-if="streamThinking" class="thinking-card expanded">
            <button class="thinking-toggle">
              <ChevronDown :size="14" class="thinking-arrow" />
              <span>Thinking...</span>
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
        <span>松开以分析文件</span>
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
          <span>{{ isParsing ? '解析中...' : '解析为日程' }}</span>
        </button>
      </div>
      <div class="input-row">
        <!-- 图片预览 -->
        <div v-if="pendingImage" class="inline-image-preview">
          <img :src="'data:image/png;base64,' + pendingImage" alt="待发送图片" />
          <button class="image-remove-inline btn-icon" @click="pendingImage = null"><X :size="10" /></button>
        </div>
        <!-- 文本输入 -->
        <textarea
          ref="inputRef"
          v-model="inputText"
          class="chat-input"
          placeholder="输入消息..."
          rows="3"
          @keydown="handleKeydown"
          @input="autoResize"
          @paste="handlePaste"
        ></textarea>
        <!-- 底部工具栏 -->
        <div class="input-toolbar">
          <button class="btn-icon attach-btn" title="附件 / 粘贴图片" @click="handleAttach">
            <Paperclip :size="16" />
          </button>
          <!-- 模型切换器 -->
          <div class="model-switcher-wrap" v-if="currentModelName">
            <button class="model-indicator" @click="toggleModelSwitcher">
              <img v-if="currentModelLogo" :src="currentModelLogo" class="model-logo-sm" @error="(e) => e.target.style.display = 'none'" />
              <span>{{ currentModelName }}</span>
              <ChevronUp :size="12" />
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
                暂无可用模型
              </div>
            </div>
          </div>
          
          <!-- 代理模式切换器 -->
          <div class="model-switcher-wrap" style="margin-left: 8px;">
            <button class="model-indicator" @click="showAgentModeSwitcher = !showAgentModeSwitcher">
              <Shield v-if="agentMode === 'insight'" :size="12" style="margin-right: 4px; color: var(--text-tertiary);" />
              <Zap v-else :size="12" style="margin-right: 4px; color: var(--accent-primary);" />
              <span>{{ agentMode === 'insight' ? '问答' : '干活' }}</span>
              <ChevronUp :size="12" />
            </button>
            <div v-if="showAgentModeSwitcher" class="model-popup">
              <button class="model-option" :class="{ active: agentMode === 'insight' }" @click="agentMode = 'insight'; showAgentModeSwitcher = false">
                <Shield :size="14" style="margin-right: 8px;" />
                <span class="model-option-label">问答 (只读防误触)</span>
              </button>
              <button class="model-option" :class="{ active: agentMode === 'yolo' }" @click="agentMode = 'yolo'; showAgentModeSwitcher = false">
                <Zap :size="14" style="margin-right: 8px;" />
                <span class="model-option-label">干活 (允许执行)</span>
              </button>
            </div>
          </div>

          <div class="toolbar-spacer"></div>
          
          <!-- 全局权限开关 -->
          <label class="global-access-toggle" :class="{ active: globalFileAccess }" title="开启后，允许AI读取或修改工作目录外的系统文件 (仅限当前对话有效)">
            <input type="checkbox" v-model="globalFileAccess" style="display: none;" />
            <Unlock v-if="globalFileAccess" :size="14" style="margin-right: 4px; color: var(--accent-primary);" />
            <Lock v-else :size="14" style="margin-right: 4px; opacity: 0.5;" />
            <span class="global-access-text">全局文件</span>
          </label>
          <div style="width: 12px;"></div>
          <!-- 计费指示器 -->
          <span class="cost-indicator" title="本次对话累计费用">
            ¥{{ sessionCost.toFixed(4) }}
          </span>
          <button
            v-if="isStreaming"
            class="action-btn stop-btn"
            @click="stopGeneration"
            title="停止生成"
          >
            <span class="icon-stop"></span>
          </button>
          <button
            v-else
            class="action-btn send-btn"
            :disabled="!canSend"
            @click="sendMessage"
            title="发送"
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
import { ref, computed, watch, onMounted, onUnmounted, nextTick, defineProps, defineEmits } from 'vue';
import { Sparkles, FileText, Camera, Calendar, User, ChevronRight, ChevronDown, ChevronUp, X, FileUp, Paperclip, Loader2, Shield, Zap, Lock, Unlock } from 'lucide-vue-next';
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
});

onUnmounted(() => {
  if (cleanupStreamListener) cleanupStreamListener();
  document.removeEventListener('dragenter', onDragEnter);
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
        content: `⚠️ ${result.error}`,
        _thinkingExpanded: false,
      });
    } else {
      let finalContent = streamContent.value || result.content || '';
      let finalThinking = streamThinking.value || result.thinking || null;
      let eventObj = null;

      // 自动日程意图检测
      const eventRegex = /<calendar_event>\s*({[\s\S]*?})\s*<\/calendar_event>/i;
      const match = finalContent.match(eventRegex);
      if (match) {
        try {
          eventObj = JSON.parse(match[1]);
          finalContent = finalContent.replace(eventRegex, '').trim();
        } catch (e) {
          console.error("Failed to parse automatic calendar event:", e);
        }
      }

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

      if (eventObj) {
        messages.value.push({ role: 'assistant', type: 'confirm-card', event: eventObj });
      }
    }
  } catch (err) {
    console.error('[sendMessage] exception:', err);
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
  // 隐藏流式输出过程中的日历块
  const cleaned = text.replace(/<calendar_event>[\s\S]*?(?:<\/calendar_event>|$)/gi, '');
  return marked.parse(cleaned);
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
  margin-bottom: var(--space-2);
  color: var(--text-tertiary);
  opacity: 0.5;
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
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-4);
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

/* ── 消息行（聊天气泡布局）─────────────────────────── */
.message-row {
  display: flex;
  gap: var(--space-2);
  max-width: 800px;
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
  background: rgba(255, 255, 255, 0.08);
  color: var(--text-secondary);
}

.avatar-bob {
  background: rgba(255, 255, 255, 0.12);
  color: var(--text-primary);
}

.bob-avatar {
  font-family: var(--font-sans);
  font-weight: 700;
  font-size: 14px;
  letter-spacing: -0.5px;
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
  background: rgba(255, 255, 255, 0.08);
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
  align-items: center;
  margin-bottom: var(--space-2);
  max-width: 800px;
  margin-left: auto;
  margin-right: auto;
}

.actions-spacer {
  flex: 1;
}

.model-switcher-wrap {
  position: relative;
}

.model-indicator {
  display: flex;
  align-items: center;
  gap: 5px;
  color: var(--text-tertiary);
  font-size: var(--text-xs);
  background: none;
  border: none;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: var(--radius-sm);
  transition: all var(--duration-fast);
  font-family: var(--font-sans);
}

.model-indicator:hover {
  color: var(--text-secondary);
  background: var(--surface-glass);
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
  background: #1c1c1c;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
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
  max-width: 800px;
  margin: 0 auto;
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
  gap: var(--space-2);
  padding-top: var(--space-1);
}

.toolbar-spacer {
  flex: 1;
}

.attach-btn {
  color: var(--text-tertiary);
  flex-shrink: 0;
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
  width: 22px;
  height: 22px;
  border-radius: var(--radius-sm);
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
  border-width: 4px 0 4px 7px;
  border-color: transparent transparent transparent var(--text-primary);
  margin-left: 2px;
}

.send-btn:disabled .icon-send {
  border-left-color: var(--text-tertiary);
}

/* 红色实心方块 ■ */
.icon-stop {
  display: block;
  width: 6px;
  height: 6px;
  border-radius: 1px;
  background: var(--color-error);
}

.stop-btn {
  border: 1px solid var(--color-error);
}

.stop-btn:hover {
  border: 1px solid var(--color-error);
  background: rgba(248, 113, 113, 0.1);
}

/* ── 计费指示器 ───────────────────────────────────── */
.cost-indicator {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  white-space: nowrap;
  padding: 0 var(--space-2);
}

/* ── 全局权限开关 ─────────────────────────────────── */
.global-access-toggle {
  display: flex;
  align-items: center;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: var(--radius-sm);
  transition: all var(--duration-fast);
  opacity: 0.6;
}

.global-access-toggle:hover {
  background: rgba(255, 255, 255, 0.05);
  opacity: 1;
}

.global-access-toggle.active {
  opacity: 1;
}

.global-access-text {
  font-size: 11px;
  color: inherit;
  font-weight: 500;
}
.global-access-toggle.active .global-access-text {
  color: var(--accent-primary);
}
</style>
