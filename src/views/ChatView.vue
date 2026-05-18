<template>
  <div class="chat-view">
    <!-- 消息区域 -->
    <div class="messages-area" ref="messagesArea">
      <!-- 统一的页面标题 -->
      <div v-if="messages.length > 0" class="view-header" :style="{ opacity: logoOpacity }">
        <h2 class="view-title">
          <img :src="bobLogoUrl" class="title-bob-logo" alt="Bob" />
        </h2>
      </div>

      <!-- 空状态：背景 logo（绝对定位，不参与 flex 布局） -->
      <div v-if="messages.length === 0" class="empty-logo-wrapper">
        <div class="empty-bob-logo"></div>
      </div>

      <!-- 空状态：前景内容（晨间汇报等） -->
      <div v-if="messages.length === 0" class="empty-state animate-fade-in">
        <!-- 晨间汇报卡片 -->
        <MorningBriefing
          @chat="onBriefingChat"
          @dismiss="() => {}"
        />
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
          <img v-else :src="bobLogoUrl" class="bob-avatar-img" alt="Bob" />
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
          <!-- 模型标注 -->
          <div v-if="msg.role === 'assistant' && msg._modelLabel" class="model-label">
            {{ msg._modelLabel }}
          </div>
        </div>
      </div>

      <!-- 流式输出中 -->
      <div v-if="isStreaming" class="message-row message-assistant animate-slide-up">
        <div class="message-avatar avatar-bob"><img :src="bobLogoUrl" class="bob-avatar-img" alt="Bob" /></div>
        <div class="message-body">
          <!-- 等待响应指示器：回车后立即出现，思考期间持续显示，直到正文开始流入才消失 -->
          <div v-if="!streamContent && activeTools.length === 0" class="typing-indicator">
            <span class="dot"></span><span class="dot"></span><span class="dot"></span>
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
                <template v-if="tool._searchResults && tool._searchResults.length > 0">
                  <SearchCard
                    v-for="(sr, si) in tool._searchResults"
                    :key="si"
                    :title="sr.title"
                    :url="sr.url"
                    :snippet="sr.snippet"
                  />
                </template>
                <template v-else>
                  {{ tool.result }}
                </template>
              </div>
            </div>
          </div>
          <div v-if="streamContent" class="message-content selectable" v-html="renderMarkdown(streamContent)"></div>
          <!-- 流式模型标注 -->
          <div v-if="currentModelName" class="model-label">{{ currentModelName }}</div>
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
        <!-- 待发送文件预览 -->
        <div v-if="pendingFiles.length > 0" class="inline-files-preview">
          <div v-for="(f, index) in pendingFiles" :key="index" class="pending-file-chip">
            <FileText :size="12" />
            <span class="pending-file-name" :title="f.name">{{ f.name }}</span>
            <button class="file-remove-btn" @click="pendingFiles.splice(index, 1)"><X :size="10" /></button>
          </div>
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
              <div class="model-popup-cols">
                <!-- 左：供应商列表 -->
                <div class="model-popup-providers">
                  <button
                    v-for="p in modelProviderList"
                    :key="p.id"
                    class="model-provider-btn"
                    :class="{ active: switcherProvider === p.id }"
                    @mouseenter="switcherProvider = p.id"
                    @click="switcherProvider = p.id"
                  >
                    <img v-if="getModelLogo(p.id)" :src="getModelLogo(p.id)" class="model-logo-sm" />
                    <span class="model-provider-name" :title="p.name">{{ p.name }}</span>
                    <span class="provider-count">{{ p.count }}</span>
                  </button>
                </div>
                <!-- 右：当前供应商的模型 -->
                <div class="model-popup-models">
                  <button
                    v-for="m in switcherModels"
                    :key="m.id"
                    class="model-option"
                    :class="{ active: currentModelRaw === m.id }"
                    @click="switchModel(m.id)"
                  >
                    <span class="model-option-label">{{ m.displayName }}</span>
                  </button>
                  <div v-if="switcherModels.length === 0" class="model-option-empty">
                    {{ $t('chat.no_models') }}
                  </div>
                </div>
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
import SearchCard from '../components/SearchCard.vue';
import FolderDropCard from '../components/FolderDropCard.vue';
import KBEstimateCard from '../components/KBEstimateCard.vue';
import MorningBriefing from '../components/MorningBriefing.vue';

const bobLogoUrl = '/bob_logo.svg';

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
const pendingFiles = ref([]); // Array of { path, name, size }
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
const switcherProvider = ref('');
const sessionCost = ref(0);

// 从 availableModels 分组出供应商列表
const modelProviderList = computed(() => {
  const map = {};
  for (const m of availableModels.value) {
    const prov = m.provider || 'unknown';
    if (!map[prov]) map[prov] = { id: prov, name: m.providerName || prov, count: 0 };
    map[prov].count++;
  }
  return Object.values(map);
});

// 当前供应商下的模型列表
const switcherModels = computed(() => {
  if (!switcherProvider.value) return [];
  const models = availableModels.value.filter(m => (m.provider || 'unknown') === switcherProvider.value);
  // 在 UI 层面强制按字母排序，确保同系列模型（如 Qwen3.5、GLM-5 等）在下拉列表里排列在一起
  return models.sort((a, b) => {
    const nameA = a.displayName || a.id || '';
    const nameB = b.displayName || b.id || '';
    return nameA.localeCompare(nameB);
  });
});
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
  if (name.includes('deepseek')) return new URL('/logos/deepseek.png', import.meta.url).href;
  if (name.includes('gpt') || name.includes('openai')) return new URL('/logos/openai.png', import.meta.url).href;
  if (name.includes('gemini') || name.includes('google') || name.includes('gemma')) return new URL('/logos/google.png', import.meta.url).href;
  if (name.includes('qwen') || name.includes('dashscope')) return new URL('/logos/qwen.png', import.meta.url).href;
  if (name.includes('glm') || name.includes('zhipu')) return new URL('/logos/glm.svg', import.meta.url).href;
  if (name.includes('kimi') || name.includes('moonshot')) return new URL('/logos/kimi.png', import.meta.url).href;
  if (name.includes('doubao') || name.includes('seed')) return new URL('/logos/doubao.png', import.meta.url).href;
  if (name.includes('minimax')) return new URL('/logos/minimax.png', import.meta.url).href;
  if (name.includes('mimo')) return new URL('/logos/mimo.png', import.meta.url).href;
  if (name.includes('modelscope')) return new URL('/logos/modelscope.png', import.meta.url).href;
  if (name.includes('claude') || name.includes('anthropic')) return new URL('/logos/claude.png', import.meta.url).href;
  if (name.includes('grok') || name.includes('xai')) return new URL('/logos/grok.png', import.meta.url).href;
  if (name.includes('openrouter')) return new URL('/logos/openrouter.png', import.meta.url).href;
  return null;
}

const currentModelName = computed(() => {
  const found = availableModels.value.find(m => m.id === currentModelRaw.value);
  if (found) return found.displayName || found.label;
  // 尝试从 ID 中提取显示名 (如 'deepseek::deepseek-v4-flash' → 'deepseek-v4-flash')
  if (currentModelRaw.value && currentModelRaw.value.includes('::')) {
    return currentModelRaw.value.split('::')[1];
  }
  return currentModelRaw.value || '';
});

const currentModelLogo = computed(() => {
  return getModelLogo(currentModelRaw.value);
});

async function toggleModelSwitcher() {
  if (!showModelSwitcher.value) {
    try {
      const pool = await window.electronAPI.getModelPool();
      let keys = {};
      if (window.electronAPI.getApiKeys) {
        keys = await window.electronAPI.getApiKeys() || {};
      }
      
      availableModels.value = (pool || [])
        .filter(m => !!keys[m.provider]) // 只保留已配置 API Key 的模型
        .map(m => ({
          id: m.id,
          provider: m.provider,
          providerName: m.providerName,
          displayName: m.displayName,
        }));
      // 默认选中当前模型所在的供应商
      if (currentModelRaw.value && currentModelRaw.value.includes('::')) {
        switcherProvider.value = currentModelRaw.value.split('::')[0];
      } else if (modelProviderList.value.length > 0) {
        switcherProvider.value = modelProviderList.value[0].id;
      }
    } catch (e) {
      availableModels.value = [];
    }
  }
  showModelSwitcher.value = !showModelSwitcher.value;
}

async function switchModel(modelId) {
  // 使用 ModelHub assign 替代旧的 config:set
  await window.electronAPI.assignModelRole(modelId, 'main');
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

// ── 点击外部关闭模型弹窗 ──────────────────────────
function onClickOutside(e) {
  // 如果点击不在任何 model-switcher-wrap 内部，关闭所有弹窗
  if (!e.target.closest('.model-switcher-wrap')) {
    showModelSwitcher.value = false;
    showAgentModeSwitcher.value = false;
  }
}

// ── 流式监听 ─────────────────────────────────────────
let cleanupStreamListener = null;
let tauriDragUnlistens = [];
let kbUnlistens = [];
// Phase 2: remote:new-message 监听（微信 Bridge 完成回复后触发）
let remoteMessageUnlisten = null;

onMounted(async () => {
  cleanupStreamListener = window.electronAPI.onStreamChunk(handleStreamChunk);

  // 监听来自 wechat-bot-bridge 的远程新消息通知，刷新当前会话消息列表
  if (window.electronAPI.onRemoteNewMessage) {
    remoteMessageUnlisten = await window.electronAPI.onRemoteNewMessage((event) => {
      const convId = event?.payload?.conversation_id || event?.conversation_id;
      if (convId && convId === props.conversationId) {
        loadMessages();
      }
    });
  }

  loadMessages();

  // 预加载模型列表 + 当前活跃模型
  try {
    const pool = await window.electronAPI.getModelPool();
    availableModels.value = (pool || []).map(m => ({
      id: m.id,
      provider: m.provider,
      providerName: m.providerName,
      displayName: m.displayName,
    }));
    const active = await window.electronAPI.getActiveModels();
    currentModelRaw.value = active?.main || '';
  } catch (e) { /* ignore */ }

  // 拖拽监听 (DOM 回退)
  document.addEventListener('dragenter', onDragEnter);

  // Tauri 原生拖拽监听
  if (window.electronAPI.onDragEnter) {
    let currentPreScanPath = null;
    
    window.electronAPI.onDragEnter(async (e) => { 
      isDragging.value = true;
      if (e.payload && e.payload.paths && e.payload.paths.length > 0) {
        const filePath = e.payload.paths[0];
        if (currentPreScanPath === filePath) return;
        currentPreScanPath = filePath;
        try {
          const meta = await window.electronAPI.getFileMeta(filePath);
          if (meta && meta.isDir) {
            // 后台预处理文件夹
            window.electronAPI.scanFolder(filePath).then(scanResult => {
               if (scanResult && !scanResult.error) {
                 window.__preScannedFolder = { path: filePath, name: meta.name, scanResult };
               }
            });
          }
        } catch(err) {}
      }
    }).then(u => tauriDragUnlistens.push(u));
    
    window.electronAPI.onDragLeave(async () => { isDragging.value = false; }).then(u => tauriDragUnlistens.push(u));
    
    window.electronAPI.onDragDrop(async (e) => {
      isDragging.value = false;
      if (e.payload && e.payload.paths && e.payload.paths.length > 0) {
        await handleTauriDrop(e.payload.paths);
      }
    }).then(u => tauriDragUnlistens.push(u));
  }

  // 点击外部关闭弹窗
  document.addEventListener('click', onClickOutside);

  // Logo 滚动视差
  if (messagesArea.value) {
    messagesArea.value.addEventListener('scroll', onMessagesScroll);
    messagesArea.value.addEventListener('click', onMessageLinkClick);
  }
});

onUnmounted(() => {
  if (cleanupStreamListener) cleanupStreamListener();
  if (remoteMessageUnlisten) remoteMessageUnlisten();
  document.removeEventListener('dragenter', onDragEnter);
  document.removeEventListener('click', onClickOutside);
  if (messagesArea.value) {
    messagesArea.value.removeEventListener('scroll', onMessagesScroll);
    messagesArea.value.removeEventListener('click', onMessageLinkClick);
  }
  tauriDragUnlistens.forEach(u => typeof u === 'function' && u());
  tauriDragUnlistens = [];
  kbUnlistens.forEach(u => typeof u === 'function' && u());
  kbUnlistens = [];
});

// 切换对话时重新加载消息
watch(() => props.conversationId, async () => {
  if (props.conversationId) {
    const conv = await window.electronAPI.getConversation(props.conversationId);
    sessionCost.value = conv?.cost || 0;
  } else {
    sessionCost.value = 0;
  }
  loadMessages();
  globalFileAccess.value = false;
  currentModelRaw.value = (await window.electronAPI.getActiveModels())?.main || '';
}, { immediate: true });

// ── 消息加载 ─────────────────────────────────────────
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

// ── 晨间汇报交互 ──────────────────────────────────────
function onBriefingChat(briefingContent) {
  inputText.value = `关于你刚才的晨间回顾，我想继续聊聊：\n\n${briefingContent}`;
  nextTick(() => sendMessage());
}

// ── 发送消息 ─────────────────────────────────────────
async function sendMessage() {
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
      content: m.content || '',
    }));

  // 在发给大模型的最终载荷里，偷偷塞入系统指令（不污染前端 UI 和数据库）
  if (filesToRead.length > 0) {
    const lastApiMsg = apiMessages[apiMessages.length - 1];
    lastApiMsg.content += `\n\n（系统内部提示：如果用户要求分析或总结上述附件，请调用 read_file 工具阅读；如果用户要求“整理进知识库”，请绝对不要尝试自己阅读，直接调用 build_knowledge_base 工具将其发往后台 Clerk 引擎）`;
  }

  await nextTick();
  scrollToBottom();

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
      
      // 持久化保存到数据库
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

      // ── Outbox: 检测 bob-config 代码块 (T-812) ──────────
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
        // 从显示内容中移除 bob-config 块 (用户不需要看到原始 JSON)
        finalContent = finalContent.replace(configBlockRegex, '').trim();
      }

      // 始终显示回复，即使内容为空也给出提示
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
  if (chunk.type === 'clear') {
    // 后端检测到 DSML 泄漏，清除已渲染的脏内容准备重试
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
      // 解析 web_search 结果为结构化卡片数据
      if (chunk.name === 'web_search' && chunk.result) {
        try {
          const parsed = JSON.parse(chunk.result);
          if (parsed.results && Array.isArray(parsed.results)) {
            tool._searchResults = parsed.results;
            tool._expanded = true; // 自动展开搜索结果
          }
        } catch (e) { /* 解析失败则显示原始文本 */ }
      }
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

// ── Tauri 原生拖拽处理 ──
async function handleTauriDrop(paths) {
  if (!paths || paths.length === 0) return;
  
  // 支持多文件拖拽
  for (const filePath of paths) {
    // 忽略图片，因为 DOM 层的 HTML5 Drag-and-Drop 会生成 File 对象并通过 handleDrop 转成 Base64
    // 为了防止冲突，我们在 Tauri 原生层静默忽略图片
    if (filePath.match(/\.(png|jpg|jpeg|gif|webp)$/i)) {
      continue;
    }

    try {
      const meta = await window.electronAPI.getFileMeta(filePath);
      if (meta && meta.isDir) {
        // 使用后台预处理结果（如果就绪）
        let scanResult;
        if (window.__preScannedFolder && window.__preScannedFolder.path === filePath) {
          scanResult = window.__preScannedFolder.scanResult;
          window.__preScannedFolder = null; // 清除缓存
        } else {
          scanResult = await window.electronAPI.scanFolder(filePath);
        }
        
        if (scanResult && !scanResult.error) {
           pendingFolderInfo.value = {
             path: filePath,
             name: meta.name,
             scanResult
           };
           scrollToBottom();
        } else {
           inputText.value = `文件夹扫描失败: ${scanResult?.message || '未知错误'}`;
        }
      } else {
        // 放入待发送区
        if (!pendingFiles.value.some(f => f.path === filePath)) {
          pendingFiles.value.push({
            path: filePath,
            name: meta ? meta.name : filePath.split(/[/\\]/).pop(),
            size: meta ? meta.size : 0
          });
        }
      }
    } catch (err) {
      console.warn(`原生拖拽处理错误: ${err.message}`);
    }
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
  const userContent = `我已经将文件夹「${folder.name}」拖入。`;
  messages.value.push({ role: 'user', content: userContent });
  await window.electronAPI.addMessage(props.conversationId, 'user', userContent, null);
  
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
    const successContent = `✅ 已将「${folder.name}」收藏到目录列表。`;
    const index = messages.value.findIndex(m => m.id === systemMsgId);
    if (index !== -1) {
       messages.value[index].content = successContent;
    }
    await window.electronAPI.addMessage(props.conversationId, 'assistant', successContent, null);
    
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
    const failContent = `❌ 文件夹收藏失败: ${err.message}`;
    const index = messages.value.findIndex(m => m.id === systemMsgId);
    if (index !== -1) {
       messages.value[index].content = failContent;
    }
    await window.electronAPI.addMessage(props.conversationId, 'assistant', failContent, null);
  }
}

function cancelKBEstimate() {
  pendingKBEstimate.value = null;
}

function startKBBuild(folderPath, plan) {
  pendingKBEstimate.value = null;

  // 在聊天流中插入进度消息（不阻塞主聊天）
  const progressMsgId = Date.now().toString();
  messages.value.push({
    id: progressMsgId,
    role: 'assistant',
    content: '📚 正在启动知识库构建...'
  });
  scrollToBottom();

  // 监听 Rust 后台的进度事件
  const unlistenProgress = window.electronAPI.onKBProgress?.((payload) => {
    const idx = messages.value.findIndex(m => m.id === progressMsgId);
    if (idx !== -1) {
      messages.value[idx].content = `📚 ${payload.message} (${payload.current}/${payload.total})`;
    }
    scrollToBottom();
  });
  if (unlistenProgress) kbUnlistens.push(unlistenProgress);

  const unlistenComplete = window.electronAPI.onKBComplete?.((payload) => {
    const idx = messages.value.findIndex(m => m.id === progressMsgId);
    if (idx !== -1) {
      if (payload.failed > 0) {
        messages.value[idx].content = `⚠️ 知识库构建完成：${payload.success}/${payload.total} 成功，${payload.failed} 个文件处理失败。你现在可以向我提问关于「${payload.folder}」的内容。`;
      } else {
        messages.value[idx].content = `✅ 知识库构建完成！已成功处理 ${payload.total} 个文件。你现在可以向我提问关于「${payload.folder}」的内容。`;
      }
      // 持久化最终结果消息
      window.electronAPI.addMessage(props.conversationId, 'assistant', messages.value[idx].content, null);
    }
    scrollToBottom();
    // 清理监听器
    if (unlistenProgress) unlistenProgress();
    if (unlistenComplete) unlistenComplete();
  });
  if (unlistenComplete) kbUnlistens.push(unlistenComplete);

  // 异步调用 Rust 后端（不 await，不阻塞）
  window.electronAPI.buildKB?.(folderPath, plan).then((result) => {
    if (result?.error) {
      const idx = messages.value.findIndex(m => m.id === progressMsgId);
      if (idx !== -1) {
        messages.value[idx].content = `❌ ${result.message}`;
      }
      if (unlistenProgress) unlistenProgress();
      if (unlistenComplete) unlistenComplete();
    }
  }).catch((err) => {
    const idx = messages.value.findIndex(m => m.id === progressMsgId);
    if (idx !== -1) {
      messages.value[idx].content = `❌ 知识库构建失败: ${err.message || err}`;
    }
    if (unlistenProgress) unlistenProgress();
    if (unlistenComplete) unlistenComplete();
  });
}

// ── 暴露给父组件 ────────────────────────────────────────
async function refreshModel() {
  try {
    const active = await window.electronAPI.getActiveModels();
    if (active && active.main) {
      currentModelRaw.value = active.main;
    }
  } catch (e) { /* ignore */ }
}

defineExpose({
  refreshModel,
  scrollToBottom,  // 从设置/日程切回时自动滚到底部
});
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
  min-height: 0;  /* flex 收缩链：允许消息区域缩到内容以下 */
  overflow-y: auto;
  padding: var(--space-6) var(--space-8);
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  position: relative;  /* 为绝对定位的 logo 背景层提供锚点 */
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

/* 背景 logo：脱离文档流，锚定在 messages-area 底部 */
.empty-logo-wrapper {
  position: absolute;
  bottom: 0;
  left: var(--space-8);
  right: var(--space-8);
  display: flex;
  justify-content: center;
  pointer-events: none;
  z-index: 0;
}

.empty-logo-wrapper .empty-bob-logo {
  max-width: 1000px;
  width: 100%;
}

/* 前景内容层（晨间汇报等） */
.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-end;
  padding-bottom: 8px;
  width: 100%;
  z-index: 1;  /* 在 logo 上方 */
  min-height: 0;  /* 允许 flex 子项收缩到内容以下，防止溢出 */
  overflow: hidden;  /* 裁剪超出容器的内容 */
}

.empty-bob-logo {
  width: 100%;
  /* 保持 bob_bob.svg 的宽高比 152.9:99.9 */
  aspect-ratio: 152.9 / 99.9;
  opacity: 0.05;
  background-color: var(--logo-color);
  -webkit-mask-image: url(/bob_bob.svg);
  mask-image: url(/bob_bob.svg);
  -webkit-mask-size: contain;
  mask-size: contain;
  -webkit-mask-repeat: no-repeat;
  mask-repeat: no-repeat;
  -webkit-mask-position: center;
  mask-position: center;
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
  font-size: 0.95em;
  padding: 0 2px;
  border-radius: 2px;
  background: transparent;
  color: var(--text-primary);
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
  background: var(--accent-primary);
  animation: dot-blink 1s ease-in-out infinite;
}

.dot-done {
  background: var(--accent-primary);
}

@keyframes dot-blink {
  0%, 100% { opacity: 0.3; }
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
  width: max-content;
  max-width: 450px;
  min-width: 300px;
  background: var(--bg-primary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
  z-index: 200;
  overflow: hidden;
}

.model-popup-cols {
  display: flex;
  max-height: 320px;
}

/* 左侧：供应商列表 */
.model-popup-providers {
  width: max-content;
  min-width: 120px;
  max-width: 160px;
  flex-shrink: 0;
  border-right: 1px solid var(--border-subtle);
  overflow-y: auto;
  overflow-x: hidden;
  padding: var(--space-1);
  background: var(--bg-secondary);
}

.model-provider-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 5px 8px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-family: var(--font-sans);
  font-size: var(--text-xs);
  text-align: left;
  cursor: pointer;
  transition: all var(--duration-fast);
  overflow: hidden;
}

.model-provider-name {
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.model-provider-btn:hover {
  background: var(--surface-glass);
  color: var(--text-primary);
}

.model-provider-btn.active {
  background: var(--bg-tertiary);
  color: var(--text-primary);
  font-weight: 500;
}

.provider-count {
  margin-left: auto;
  font-size: 10px;
  color: var(--text-tertiary);
}

/* 右侧：模型列表 */
.model-popup-models {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: var(--space-1);
  min-width: 0;
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
  font-size: var(--text-xs);
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
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
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

.inline-files-preview {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 4px 0 6px 0;
}

.pending-file-chip {
  display: flex;
  align-items: center;
  gap: 6px;
  background: var(--surface-card);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
  padding: 4px 8px 4px 6px;
  font-size: 11px;
  color: var(--text-secondary);
}

.pending-file-name {
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-remove-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 2px;
  border-radius: var(--radius-xs);
}

.file-remove-btn:hover {
  background: var(--surface-hover);
  color: var(--text-primary);
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

/* ── 模型标注 ─────────────────────────────────────── */
.model-label {
  font-size: 11px;
  color: var(--text-muted);
  opacity: 0.5;
  margin-top: 2px;
  font-family: var(--font-mono);
  user-select: none;
  letter-spacing: 0.02em;
}

/* ── 思考中动画 ───────────────────────────────────── */
.typing-indicator {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: var(--space-2) 0;
  height: 24px;
}

.typing-indicator .dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-muted);
  opacity: 0.4;
  animation: typing-bounce 1.4s ease-in-out infinite;
}

.typing-indicator .dot:nth-child(1) { animation-delay: 0s; }
.typing-indicator .dot:nth-child(2) { animation-delay: 0.2s; }
.typing-indicator .dot:nth-child(3) { animation-delay: 0.4s; }

@keyframes typing-bounce {
  0%, 60%, 100% {
    transform: translateY(0);
    opacity: 0.4;
  }
  30% {
    transform: translateY(-6px);
    opacity: 1;
  }
}
</style>
