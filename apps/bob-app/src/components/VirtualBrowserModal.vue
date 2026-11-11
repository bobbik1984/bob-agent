<script>
export function mapClientPointToRemote({
  clientX,
  clientY,
  rect,
  remoteWidth,
  remoteHeight,
  zoom = 1,
  panX = 0,
  panY = 0,
}) {
  if (!rect || rect.width <= 0 || rect.height <= 0) {
    return { x: 0, y: 0 };
  }
  const z = zoom > 0 ? zoom : 1;
  const relX = (clientX - rect.left - panX) / z;
  const relY = (clientY - rect.top - panY) / z;
  const scaleX = remoteWidth / rect.width;
  const scaleY = remoteHeight / rect.height;
  const rawX = relX * scaleX;
  const rawY = relY * scaleY;
  const x = Math.max(0, Math.min(Math.round(rawX), remoteWidth));
  const y = Math.max(0, Math.min(Math.round(rawY), remoteHeight));
  return { x, y };
}
</script>

<script setup>
import { computed, inject, onMounted, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  takeover: {
    type: Object,
    default: null,
  },
});

const emit = defineEmits(['close', 'complete']);

const shell = inject('bobAppShell', {});
const jarvisHome = shell?.jarvisHome;
const { t } = useI18n();

const activeTakeover = computed(() => props.takeover || jarvisHome?.activeTakeover?.value || null);

// WebSocket State
const ws = ref(null);
const connected = ref(false);
const currentFrameSrc = ref('');
const remoteUrl = ref('');
const remoteWidth = ref(390);
const remoteHeight = ref(844);
const desktopMode = ref(false);
const sensitiveInputFocused = ref(false);

// Zoom and Stage Viewport
const zoomLevel = ref(1);
const panOffset = ref({ x: 0, y: 0 });
const stageRef = ref(null);
let isPointerDown = false;
let lastMoveTime = 0;

// Bottom Input Bar
const inputBarText = ref('');
const inputMode = ref('text'); // 'text' | 'password'
const isComposing = ref(false);

const displayDomain = computed(() => {
  const raw = remoteUrl.value || activeTakeover.value?.url || '';
  if (!raw) return 'Virtual Browser';
  try {
    const u = new URL(raw.startsWith('http') ? raw : `https://${raw}`);
    return u.hostname || raw;
  } catch {
    return raw;
  }
});

function buildWsUrl(takeoverId) {
  if (typeof window === 'undefined') return '';
  const loc = window.location;
  const proto = loc.protocol === 'https:' ? 'wss:' : 'ws:';
  const host = loc.host || 'localhost:1420';
  let url = `${proto}//${host}/api/ws/browser-takeover?session_id=${encodeURIComponent(takeoverId)}`;
  const token =
    (typeof window !== 'undefined' && window.__BOB_NODE_TOKEN__) ||
    (typeof localStorage !== 'undefined' &&
      (localStorage.getItem('bob_node_session_token') || localStorage.getItem('bob_node_token')));
  if (token) {
    url += `&token=${encodeURIComponent(token)}`;
  }
  return url;
}

function handleWsMessage(msg) {
  if (!msg || typeof msg !== 'object') return;
  const type = msg.type || msg.event;

  if (type === 'viewport_meta' || type === 'meta') {
    if (msg.url) remoteUrl.value = msg.url;
    if (msg.remote_width || msg.remoteWidth) {
      remoteWidth.value = msg.remote_width || msg.remoteWidth;
    }
    if (msg.remote_height || msg.remoteHeight) {
      remoteHeight.value = msg.remote_height || msg.remoteHeight;
    }
    if (msg.desktop_mode !== undefined || msg.desktopMode !== undefined) {
      desktopMode.value = Boolean(msg.desktop_mode ?? msg.desktopMode);
    }
    if (msg.sensitive_input_focused !== undefined || msg.sensitiveInputFocused !== undefined) {
      sensitiveInputFocused.value = Boolean(msg.sensitive_input_focused ?? msg.sensitiveInputFocused);
      if (sensitiveInputFocused.value) {
        inputMode.value = 'password';
      }
    }
  } else if (type === 'frame') {
    const fmt = msg.format || 'jpeg';
    if (msg.data_base64) {
      currentFrameSrc.value = `data:image/${fmt};base64,${msg.data_base64}`;
    }
  } else if (type === 'takeover_ended' || type === 'ended') {
    handBackToBob();
  }
}

function connectWs(takeoverId) {
  if (ws.value) {
    try {
      ws.value.close();
    } catch {}
    ws.value = null;
  }
  if (!takeoverId) return;

  const url = buildWsUrl(takeoverId);
  if (!url) return;

  try {
    const socket = new WebSocket(url);
    ws.value = socket;

    socket.onopen = () => {
      connected.value = true;
    };

    socket.onmessage = evt => {
      try {
        const msg = JSON.parse(evt.data);
        handleWsMessage(msg);
      } catch {}
    };

    socket.onclose = () => {
      connected.value = false;
    };

    socket.onerror = () => {
      connected.value = false;
    };
  } catch {}
}

function sendWs(payload) {
  if (ws.value && ws.value.readyState === WebSocket.OPEN) {
    ws.value.send(JSON.stringify(payload));
  }
}

function onPointerDown(e) {
  if (!stageRef.value) return;
  isPointerDown = true;
  const rect = stageRef.value.getBoundingClientRect();
  const pt = mapClientPointToRemote({
    clientX: e.clientX,
    clientY: e.clientY,
    rect,
    remoteWidth: remoteWidth.value,
    remoteHeight: remoteHeight.value,
    zoom: zoomLevel.value,
    panX: panOffset.value.x,
    panY: panOffset.value.y,
  });
  sendWs({ type: 'pointer_down', x: pt.x, y: pt.y, button: e.button || 0 });
}

function onPointerMove(e) {
  if (!isPointerDown || !stageRef.value) return;
  const now = Date.now();
  if (now - lastMoveTime < 24) return;
  lastMoveTime = now;

  const rect = stageRef.value.getBoundingClientRect();
  const pt = mapClientPointToRemote({
    clientX: e.clientX,
    clientY: e.clientY,
    rect,
    remoteWidth: remoteWidth.value,
    remoteHeight: remoteHeight.value,
    zoom: zoomLevel.value,
    panX: panOffset.value.x,
    panY: panOffset.value.y,
  });
  sendWs({ type: 'pointer_move', x: pt.x, y: pt.y });
}

function onPointerUp(e) {
  if (!isPointerDown || !stageRef.value) return;
  isPointerDown = false;
  const rect = stageRef.value.getBoundingClientRect();
  const pt = mapClientPointToRemote({
    clientX: e.clientX,
    clientY: e.clientY,
    rect,
    remoteWidth: remoteWidth.value,
    remoteHeight: remoteHeight.value,
    zoom: zoomLevel.value,
    panX: panOffset.value.x,
    panY: panOffset.value.y,
  });
  sendWs({ type: 'pointer_up', x: pt.x, y: pt.y, button: e.button || 0 });
}

function onWheel(e) {
  sendWs({
    type: 'scroll',
    delta_x: e.deltaX || 0,
    delta_y: e.deltaY || 0,
  });
}

function navigateBack() {
  sendWs({ type: 'navigate', action: 'back' });
}

function navigateReload() {
  sendWs({ type: 'navigate', action: 'reload' });
}

function toggleDesktopMode() {
  desktopMode.value = !desktopMode.value;
  if (desktopMode.value) {
    remoteWidth.value = 1280;
    remoteHeight.value = 800;
  } else {
    remoteWidth.value = 390;
    remoteHeight.value = 844;
  }
  sendWs({ type: 'toggle_desktop', desktop: desktopMode.value });
}

function cycleZoom() {
  if (zoomLevel.value === 1) zoomLevel.value = 1.5;
  else if (zoomLevel.value === 1.5) zoomLevel.value = 2;
  else zoomLevel.value = 1;
}

function saveScreenshot() {
  if (!currentFrameSrc.value || typeof document === 'undefined') return;
  const a = document.createElement('a');
  a.href = currentFrameSrc.value;
  a.download = `bob-browser-qr-${Date.now()}.jpg`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

function onInputCompositionStart() {
  isComposing.value = true;
}

function onInputCompositionEnd() {
  isComposing.value = false;
}

function sendText() {
  const text = inputBarText.value;
  if (!text) return;
  sendWs({ type: 'insert_text', text });
  inputBarText.value = '';
}

function onInputKeydown(e) {
  if (e.key === 'Enter') {
    if (isComposing.value) {
      return;
    }
    e.preventDefault();
    sendText();
  }
}

function sendKey(keyName) {
  sendWs({ type: 'key_press', key: keyName });
}

function handBackToBob() {
  sendWs({ type: 'complete' });
  if (ws.value) {
    try {
      ws.value.close();
    } catch {}
    ws.value = null;
  }
  emit('complete', activeTakeover.value?.id);
  if (activeTakeover.value?.id) {
    jarvisHome?.completeTakeover?.(activeTakeover.value.id);
  } else {
    jarvisHome?.closeTakeover?.();
  }
}

function cancelTakeover() {
  if (ws.value) {
    try {
      ws.value.close();
    } catch {}
    ws.value = null;
  }
  emit('close');
  jarvisHome?.closeTakeover?.();
}

watch(
  () => activeTakeover.value?.id,
  newId => {
    if (newId) {
      remoteUrl.value = activeTakeover.value?.url || '';
      if (activeTakeover.value?.width) remoteWidth.value = activeTakeover.value.width;
      if (activeTakeover.value?.height) remoteHeight.value = activeTakeover.value.height;
      if (activeTakeover.value?.desktopMode !== undefined) {
        desktopMode.value = activeTakeover.value.desktopMode;
      }
      zoomLevel.value = 1;
      sensitiveInputFocused.value = false;
      inputMode.value = 'text';
      connectWs(newId);
    } else {
      if (ws.value) {
        try {
          ws.value.close();
        } catch {}
        ws.value = null;
      }
      currentFrameSrc.value = '';
    }
  },
  { immediate: true },
);

onUnmounted(() => {
  if (ws.value) {
    try {
      ws.value.close();
    } catch {}
    ws.value = null;
  }
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="activeTakeover"
      class="virtual-browser-backdrop"
      role="dialog"
      aria-modal="true"
      :aria-label="t('browserTakeoverTitle')"
      @keydown.esc="cancelTakeover"
    >
      <div
        class="virtual-browser-container"
        :class="{ 'is-desktop-mode': desktopMode }"
      >
        <!-- Top Toolbar -->
        <header class="browser-toolbar">
          <div class="toolbar-nav-group">
            <button
              type="button"
              class="toolbar-btn"
              :title="t('browserBack')"
              :aria-label="t('browserBack')"
              @click="navigateBack"
            >
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
                <path d="m15 18-6-6 6-6" />
              </svg>
            </button>
            <button
              type="button"
              class="toolbar-btn"
              :title="t('browserReload')"
              :aria-label="t('browserReload')"
              @click="navigateReload"
            >
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
                <path d="M3 3v5h5" />
                <path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" />
                <path d="M16 21h5v-5" />
              </svg>
            </button>
          </div>

          <!-- URL & Domain Pill -->
          <div class="url-domain-pill" :title="remoteUrl || activeTakeover.url">
            <svg class="pill-lock-icon" viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
              <path d="M7 11V7a5 5 0 0 1 10 0v4" />
            </svg>
            <span class="pill-domain-text">{{ displayDomain }}</span>
          </div>

          <div class="toolbar-actions-group">
            <!-- Desktop / Mobile Toggle -->
            <button
              type="button"
              class="toolbar-btn"
              :class="{ active: desktopMode }"
              :title="t(desktopMode ? 'browserDesktopMode' : 'browserMobileMode')"
              :aria-label="t(desktopMode ? 'browserDesktopMode' : 'browserMobileMode')"
              @click="toggleDesktopMode"
            >
              <svg v-if="desktopMode" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="2" y="3" width="20" height="14" rx="2" />
                <line x1="8" y1="21" x2="16" y2="21" />
                <line x1="12" y1="17" x2="12" y2="21" />
              </svg>
              <svg v-else viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="5" y="2" width="14" height="20" rx="2" ry="2" />
                <line x1="12" y1="18" x2="12.01" y2="18" />
              </svg>
            </button>

            <!-- Zoom Cycle Toggle -->
            <button
              type="button"
              class="toolbar-btn zoom-btn"
              :title="t('browserZoom')"
              @click="cycleZoom"
            >
              <span>{{ zoomLevel }}x</span>
            </button>

            <!-- Save QR / Screenshot -->
            <button
              type="button"
              class="toolbar-btn"
              :disabled="!currentFrameSrc"
              :title="t('browserSaveScreenshot')"
              :aria-label="t('browserSaveScreenshot')"
              @click="saveScreenshot"
            >
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="3" y="3" width="7" height="7" />
                <rect x="14" y="3" width="7" height="7" />
                <rect x="14" y="14" width="7" height="7" />
                <rect x="3" y="14" width="7" height="7" />
              </svg>
            </button>

            <!-- Close Modal -->
            <button
              type="button"
              class="toolbar-btn close-btn"
              :aria-label="t('browserCancelTakeover')"
              @click="cancelTakeover"
            >
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6 6 18M6 6l12 12" />
              </svg>
            </button>
          </div>
        </header>

        <!-- Privacy Shield Badge (Zero-LLM Direct Encrypted Input) -->
        <div v-if="sensitiveInputFocused" class="privacy-notice-banner" role="status">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
          </svg>
          <span>{{ t('browserZeroLlmBadge') }}</span>
        </div>

        <!-- Interactive Remote Stage -->
        <main
          ref="stageRef"
          class="browser-stage-viewport"
          tabindex="0"
          @pointerdown="onPointerDown"
          @pointermove="onPointerMove"
          @pointerup="onPointerUp"
          @pointercancel="onPointerUp"
          @wheel.prevent="onWheel"
        >
          <div
            class="stage-content-wrapper"
            :style="{
              transform: `scale(${zoomLevel}) translate(${panOffset.x}px, ${panOffset.y}px)`,
              transformOrigin: 'top left',
            }"
          >
            <img
              v-if="currentFrameSrc"
              :src="currentFrameSrc"
              alt="Remote Browser Frame"
              class="remote-frame-img"
              draggable="false"
            />
            <div v-else class="stage-placeholder">
              <svg class="spinner-svg" viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="10" stroke-opacity="0.25" />
                <path d="M12 2a10 10 0 0 1 10 10" />
              </svg>
              <span>{{ t('browserConnecting') }}</span>
            </div>
          </div>
        </main>

        <!-- Bottom Native Input Bar (Chinese IME & Password/OTP Safe) -->
        <section class="browser-input-bar">
          <div class="input-row">
            <button
              type="button"
              class="input-mode-btn"
              :class="{ masked: inputMode === 'password' }"
              :title="t(inputMode === 'password' ? 'browserMaskedMode' : 'browserTextMode')"
              @click="inputMode = inputMode === 'password' ? 'text' : 'password'"
            >
              <svg v-if="inputMode === 'password'" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                <path d="M7 11V7a5 5 0 0 1 10 0v4" />
              </svg>
              <svg v-else viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="4 7 4 4 20 4 20 7" />
                <line x1="9" y1="20" x2="15" y2="20" />
                <line x1="12" y1="4" x2="12" y2="20" />
              </svg>
              <span>{{ t(inputMode === 'password' ? 'browserMaskedMode' : 'browserTextMode') }}</span>
            </button>

            <input
              v-model="inputBarText"
              :type="inputMode === 'password' ? 'password' : 'text'"
              :autocomplete="inputMode === 'password' ? 'off' : 'on'"
              class="input-native-control"
              :placeholder="t(inputMode === 'password' ? 'browserPasswordPlaceholder' : 'browserInputPlaceholder')"
              @compositionstart="onInputCompositionStart"
              @compositionend="onInputCompositionEnd"
              @keydown="onInputKeydown"
            />

            <button
              type="button"
              class="send-text-btn"
              :disabled="!inputBarText.trim()"
              @click="sendText"
            >
              {{ t('browserSendText') }}
            </button>
          </div>

          <div class="quick-keys-strip">
            <button type="button" class="key-pill-btn" @click="sendKey('Enter')">
              {{ t('browserKeyEnter') }}
            </button>
            <button type="button" class="key-pill-btn" @click="sendKey('Tab')">
              {{ t('browserKeyTab') }}
            </button>
            <button type="button" class="key-pill-btn" @click="sendKey('Backspace')">
              {{ t('browserKeyBackspace') }}
            </button>
          </div>
        </section>

        <!-- Footer Hand back to Bob CTA -->
        <footer class="browser-footer">
          <button
            type="button"
            class="footer-btn footer-btn-secondary"
            @click="cancelTakeover"
          >
            {{ t('browserCancelTakeover') }}
          </button>
          <button
            type="button"
            class="footer-btn footer-btn-primary"
            @click="handBackToBob"
          >
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="20 6 9 17 4 12" />
            </svg>
            <span>{{ t('browserHandBackToBob') }}</span>
          </button>
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.virtual-browser-backdrop {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in srgb, var(--bg-root) 82%, transparent);
  backdrop-filter: blur(8px);
  padding: var(--gap-sm);
  scrollbar-gutter: stable;
}

.virtual-browser-container {
  display: flex;
  flex-direction: column;
  width: 100%;
  max-width: 440px;
  height: 92vh;
  max-height: 900px;
  background: var(--bg-primary);
  border: 1px solid var(--line);
  border-radius: var(--radius-card);
  box-shadow: var(--shadow);
  overflow: hidden;
  color: var(--text-primary);
  transition: max-width 0.25s ease;
}

.virtual-browser-container.is-desktop-mode {
  max-width: 980px;
}

.browser-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-xs);
  padding: 0.5rem 0.75rem;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--line);
  flex-shrink: 0;
}

.toolbar-nav-group,
.toolbar-actions-group {
  display: flex;
  align-items: center;
  gap: var(--gap-3xs);
}

.toolbar-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 2rem;
  height: 2rem;
  padding: 0;
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--radius-md);
  color: var(--text-secondary);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.toolbar-btn:hover:not(:disabled) {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.toolbar-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.toolbar-btn.active {
  background: var(--accent-soft);
  color: var(--accent-primary);
  border-color: color-mix(in srgb, var(--accent-primary) 30%, transparent);
}

.zoom-btn {
  font-size: var(--text-2xs);
  font-weight: 600;
  width: auto;
  padding: 0 0.4rem;
}

.url-domain-pill {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  max-width: 45%;
  padding: 0.2rem 0.65rem;
  background: var(--bg-tertiary);
  border: 1px solid var(--line);
  border-radius: var(--radius-pill);
  font-size: var(--text-xs);
  color: var(--text-primary);
  overflow: hidden;
}

.pill-lock-icon {
  color: var(--color-success);
  flex-shrink: 0;
}

.pill-domain-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.privacy-notice-banner {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
  padding: 0.35rem 0.75rem;
  background: var(--warning-soft);
  color: var(--warning-text);
  border-bottom: 1px solid color-mix(in srgb, var(--color-warning) 30%, transparent);
  font-size: var(--text-xs);
  font-weight: 500;
  flex-shrink: 0;
}

.browser-stage-viewport {
  flex: 1;
  position: relative;
  background: var(--bg-root);
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  touch-action: none;
  user-select: none;
  outline: none;
}

.stage-content-wrapper {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
}

.remote-frame-img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
}

.stage-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--gap-sm);
  color: var(--text-secondary);
  font-size: var(--text-sm);
}

.spinner-svg {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.browser-input-bar {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  padding: 0.5rem 0.75rem;
  background: var(--bg-secondary);
  border-top: 1px solid var(--line);
  flex-shrink: 0;
}

.input-row {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
}

.input-mode-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.35rem 0.55rem;
  background: var(--bg-tertiary);
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
  color: var(--text-secondary);
  font-size: var(--text-2xs);
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
}

.input-mode-btn.masked {
  background: var(--warning-soft);
  color: var(--warning-text);
  border-color: color-mix(in srgb, var(--color-warning) 30%, transparent);
}

.input-native-control {
  flex: 1;
  min-width: 0;
  padding: 0.4rem 0.65rem;
  background: var(--bg-primary);
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  font-size: var(--text-sm);
  outline: none;
  transition: border-color 0.15s ease;
}

.input-native-control:focus {
  border-color: var(--accent-primary);
}

.send-text-btn {
  padding: 0.4rem 0.75rem;
  background: var(--accent-primary);
  border: 1px solid var(--accent-primary);
  border-radius: var(--radius-md);
  color: var(--text-on-accent);
  font-size: var(--text-xs);
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
}

.send-text-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.quick-keys-strip {
  display: flex;
  align-items: center;
  gap: 0.35rem;
}

.key-pill-btn {
  padding: 0.15rem 0.5rem;
  background: var(--bg-tertiary);
  border: 1px solid var(--line);
  border-radius: var(--radius-pill);
  color: var(--text-secondary);
  font-size: var(--text-2xs);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}

.key-pill-btn:hover {
  background: var(--bg-primary);
  color: var(--text-primary);
}

.browser-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--gap-sm);
  padding: 0.6rem 0.75rem;
  background: var(--bg-primary);
  border-top: 1px solid var(--line);
  flex-shrink: 0;
}

.footer-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--gap-3xs);
  padding: 0.45rem 1rem;
  border-radius: var(--radius-btn);
  font-size: var(--text-sm);
  font-weight: 600;
  cursor: pointer;
  transition: opacity 0.15s ease, background 0.15s ease;
}

.footer-btn-secondary {
  background: var(--bg-secondary);
  border: 1px solid var(--line);
  color: var(--text-primary);
}

.footer-btn-secondary:hover {
  background: var(--bg-tertiary);
}

.footer-btn-primary {
  background: var(--accent-primary);
  border: 1px solid var(--accent-primary);
  color: var(--text-on-accent);
}

.footer-btn-primary:hover {
  opacity: 0.92;
}
</style>
