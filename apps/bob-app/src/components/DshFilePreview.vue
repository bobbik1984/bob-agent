<script setup>
import {ref, onMounted, onUnmounted, provide, inject} from 'vue';
import {useI18n} from 'vue-i18n';
import {kernelBridge} from '../native-bridge.mjs';
import MessageContent from './MessageContent.vue';
import {previewErrorKey} from '../preview-error.mjs';
import {artifactHtmlDocument, artifactHtmlSandbox} from '../artifact-html.mjs';

const props = defineProps({address: Object});
const emit = defineEmits(['close', 'quote']);

function quoteForFeedback() {
  const path = props.address?.path || '';
  const name = path.split('/').pop() || 'image';
  emit('quote', { path, name });
}
const {t} = useI18n();

provide('dshOpenFile', null);
const getFile = inject('dshPreviewGet', a => kernelBridge.dshPreview(a.connectionId, a.sessionId, a.path, null, a.messageSequence));

// State
const dialog = ref(null), busy = ref(true), error = ref(''), file = ref(null), url = ref(''), text = ref('');
const zoomed = ref(false);
let alive = true, pushedStates = 0, ignoringPop = false, closingFromPop = false;
const origin = globalThis.document?.activeElement || null;

const documentTypes = {
  pdf: 'application/pdf',
  docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  doc: 'application/msword',
  xls: 'application/vnd.ms-excel',
  ppt: 'application/vnd.ms-powerpoint',
  odt: 'application/vnd.oasis.opendocument.text',
  ods: 'application/vnd.oasis.opendocument.spreadsheet',
  odp: 'application/vnd.oasis.opendocument.presentation'
};
const imageTypes = {png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', webp: 'image/webp', svg: 'image/svg+xml'};

// Image Inspector State
const imgBgMode = ref('checkerboard'); // 'checkerboard' | 'dark' | 'light'
const zoomLevel = ref('fit'); // 'fit' | '100' | '200'
const imgNatural = ref({width: 0, height: 0});

// HTML Prototype Viewport State
const viewportMode = ref('responsive'); // 'responsive' | 'mobile_720' | 'mobile_1080' | 'tablet'
const isLandscape = ref(false);
const htmlReloadKey = ref(0);

function getViewportDimensions() {
  let w = 0, h = 0;
  if (viewportMode.value === 'mobile_720') {
    w = 720; h = 1280;
  } else if (viewportMode.value === 'mobile_1080') {
    w = 1080; h = 1920;
  } else if (viewportMode.value === 'tablet') {
    w = 1280; h = 800;
  }
  if (!w || !h) return null;
  return isLandscape.value ? {width: h, height: w} : {width: w, height: h};
}

function toggleOrientation() {
  isLandscape.value = !isLandscape.value;
}

function reloadHtml() {
  htmlReloadKey.value++;
}

function onImgLoad(e) {
  if (e.target) {
    imgNatural.value = {
      width: e.target.naturalWidth || 0,
      height: e.target.naturalHeight || 0
    };
  }
}

function setZoom(level) {
  zoomLevel.value = level;
  if (level !== 'fit') {
    if (!zoomed.value) {
      zoomed.value = true;
      pushPreviewState('zoom');
    }
  } else {
    zoomed.value = false;
  }
}

function toggleZoom() {
  if (!zoomed.value) {
    zoomed.value = true;
    zoomLevel.value = '100';
    pushPreviewState('zoom');
  } else {
    zoomed.value = false;
    zoomLevel.value = 'fit';
    handleBack();
  }
}

function pushPreviewState(layer) {
  try {
    if (globalThis.history?.pushState) {
      globalThis.history.pushState({bobPreview: layer, path: props.address?.path || ''}, '');
      pushedStates++;
    }
  } catch {}
}

function handleBack() {
  if (zoomed.value) {
    zoomed.value = false;
    zoomLevel.value = 'fit';
    if (pushedStates > 1 && globalThis.history?.back) {
      pushedStates--;
      ignoringPop = true;
      try { globalThis.history.back(); } catch { ignoringPop = false; }
    }
    return;
  }
  closeAll();
}

function closeAll() {
  zoomed.value = false;
  zoomLevel.value = 'fit';
  if (!closingFromPop && pushedStates > 0 && globalThis.history?.go) {
    const count = pushedStates;
    pushedStates = 0;
    ignoringPop = true;
    try { globalThis.history.go(-count); } catch { ignoringPop = false; }
  }
  emit('close');
}

function onPopState() {
  if (ignoringPop) { ignoringPop = false; return; }
  if (pushedStates > 0) pushedStates--;
  if (zoomed.value) {
    zoomed.value = false;
    zoomLevel.value = 'fit';
    return;
  }
  closingFromPop = true;
  emit('close');
}

function onBackdropClick(e) {
  if (e.target === dialog.value) {
    const rect = dialog.value.getBoundingClientRect();
    const inside = rect.top <= e.clientY && e.clientY <= rect.top + rect.height && rect.left <= e.clientX && e.clientX <= rect.left + rect.width;
    if (!inside) closeAll();
  }
}

onMounted(async () => {
  pushPreviewState('file');
  globalThis.window?.addEventListener?.('popstate', onPopState);
  globalThis.document?.body?.classList?.add?.('dsh-modal-blur-active');
  dialog.value?.showModal?.();
  try {
    const a = props.address;
    const applyFile = f => {
      if (!alive) return;
      if (f.path !== a.path || f.session_id !== a.sessionId || f.connection_id !== a.connectionId) {
        throw Error('DSH_IDENTITY_MISMATCH');
      }
      const bytes = Uint8Array.from(atob(f.data), c => c.charCodeAt(0));
      if (bytes.length !== f.bytes) throw Error('DSH_FILE_CHANGED');
      if (url.value) URL.revokeObjectURL(url.value);
      file.value = f;
      url.value = URL.createObjectURL(new Blob([bytes], {type: imageTypes[f.extension] || documentTypes[f.extension] || 'text/plain;charset=utf-8'}));
      if (!imageTypes[f.extension] && !documentTypes[f.extension]) {
        text.value = new TextDecoder('utf-8', {fatal: true}).decode(bytes);
      }
    };
    const f = await getFile(a, updated => {
      try { applyFile(updated); } catch {/* Keep verified preview on failure */}
    });
    applyFile(f);
  } catch (e) {
    error.value = String(e.message || e).match(/DSH_[A-Z_]+/)?.[0] || 'DSH_PREVIEW_UNAVAILABLE';
  } finally {
    busy.value = false;
  }
});

onUnmounted(() => {
  alive = false;
  globalThis.window?.removeEventListener?.('popstate', onPopState);
  globalThis.document?.body?.classList?.remove?.('dsh-modal-blur-active');
  if (url.value) URL.revokeObjectURL(url.value);
  origin?.focus?.({preventScroll: true});
});
</script>

<template>
<dialog ref="dialog" class="dsh-preview" @cancel.prevent="handleBack" @click="onBackdropClick">
  <header class="dsh-preview-header">
    <button type="button" class="dsh-preview-back" @click="handleBack">← {{ t(zoomed ? 'filePreviewExitZoom' : 'filePreviewBack') }}</button>
    <strong class="dsh-preview-title">{{ file?.resolved_path || address.path }}</strong>
    <button type="button" class="dsh-preview-close" @click="closeAll">{{ t('filePreviewClose') }}</button>
  </header>

  <!-- HTML Prototype Viewport Toolbar -->
  <div v-if="file?.extension === 'html' && !error" class="dsh-inspector-toolbar" role="toolbar" aria-label="Viewport toolbar">
    <div class="toolbar-group">
      <button type="button" class="toolbar-btn" :class="{active: viewportMode === 'responsive'}" @click="viewportMode = 'responsive'">{{ t('viewportResponsive') }}</button>
      <button type="button" class="toolbar-btn" :class="{active: viewportMode === 'mobile_720'}" @click="viewportMode = 'mobile_720'">720p 手机</button>
      <button type="button" class="toolbar-btn" :class="{active: viewportMode === 'mobile_1080'}" @click="viewportMode = 'mobile_1080'">1080p 手机</button>
      <button type="button" class="toolbar-btn" :class="{active: viewportMode === 'tablet'}" @click="viewportMode = 'tablet'">{{ t('viewportTablet') }}</button>
    </div>
    <div v-if="getViewportDimensions()" class="toolbar-group">
      <button type="button" class="toolbar-btn" :title="t('viewportRotate')" @click="toggleOrientation">
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="2" width="14" height="20" rx="2"/><path d="M12 18h.01"/><path d="M20 7l-2-2-2 2"/></svg>
        <span>{{ isLandscape ? '横屏' : '竖屏' }}</span>
      </button>
      <span class="toolbar-spec-pill">{{ getViewportDimensions()?.width }} × {{ getViewportDimensions()?.height }}</span>
    </div>
    <div class="toolbar-group">
      <button type="button" class="toolbar-btn reload-btn" :title="t('viewportReload')" @click="reloadHtml">
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67"/></svg>
        <span>{{ t('viewportReload') }}</span>
      </button>
    </div>
  </div>

  <!-- Image High-Fidelity Inspector Toolbar -->
  <div v-if="imageTypes[file?.extension] && !error" class="dsh-inspector-toolbar" role="toolbar" aria-label="Image Inspector">
    <div class="toolbar-group">
      <span class="toolbar-label">{{ t('bgCheckerboard') }}:</span>
      <button type="button" class="toolbar-btn" :class="{active: imgBgMode === 'checkerboard'}" @click="imgBgMode = 'checkerboard'">{{ t('bgCheckerboard') }}</button>
      <button type="button" class="toolbar-btn" :class="{active: imgBgMode === 'dark'}" @click="imgBgMode = 'dark'">{{ t('bgDark') }}</button>
      <button type="button" class="toolbar-btn" :class="{active: imgBgMode === 'light'}" @click="imgBgMode = 'light'">{{ t('bgLight') }}</button>
    </div>
    <div class="toolbar-group">
      <span class="toolbar-label">缩放:</span>
      <button type="button" class="toolbar-btn" :class="{active: zoomLevel === 'fit'}" @click="setZoom('fit')">{{ t('zoomFit') }}</button>
      <button type="button" class="toolbar-btn" :class="{active: zoomLevel === '100'}" @click="setZoom('100')">{{ t('zoom100') }}</button>
      <button type="button" class="toolbar-btn" :class="{active: zoomLevel === '200'}" @click="setZoom('200')">{{ t('zoom200') }}</button>
    </div>
    <div v-if="imgNatural.width > 0" class="toolbar-group">
      <span class="toolbar-spec-pill">{{ imgNatural.width }} × {{ imgNatural.height }} px</span>
    </div>
    <div class="toolbar-group">
      <button type="button" class="toolbar-btn quote-feedback-btn" :title="t('quoteFeedbackBtn')" @click="quoteForFeedback">
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
        <span>{{ t('quoteFeedbackBtn') }}</span>
      </button>
    </div>
  </div>

  <p class="preview-status-line">{{ t(file?.from_cache ? 'filePreviewCached' : 'filePreviewCurrent') }}</p>
  <p v-if="file?.resources_unavailable?.length" role="status" class="preview-warning">{{ t('filePreviewResourcesPartial', {count: file.resources_unavailable.length}) }}</p>
  <p v-if="busy" role="status">{{ t('filePreviewLoading') }}</p>
  <p v-if="error" role="alert" class="preview-error">{{ t(previewErrorKey(error)) }} · {{ error }}</p>

  <template v-if="file && !error">
    <!-- Legacy compatibility button (hidden if inspector is active, but present for backward-compatibility) -->
    <button v-if="imageTypes[file.extension]" type="button" class="legacy-zoom-btn" style="display:none;" @click="toggleZoom">{{ t(zoomed ? 'filePreviewFit' : 'filePreviewActual') }}</button>

    <div class="dsh-preview-body" :class="{zoomed: zoomed, [`canvas-${imgBgMode}`]: imageTypes[file.extension]}">
      <!-- Image View -->
      <div v-if="imageTypes[file.extension]" class="image-stage" :class="[`zoom-${zoomLevel}`]">
        <img :src="url" :alt="address.path" @load="onImgLoad"/>
      </div>

      <!-- Document PDF Pages -->
      <section v-else-if="file.document" class="document-pages">
        <figure v-for="page in file.document.pages" :key="page.number">
          <figcaption>{{ t('filePreviewPage', {page: page.number, total: file.document.page_count}) }}</figcaption>
          <img :src="'data:image/jpeg;base64,' + page.data" :alt="t('filePreviewPage', {page: page.number, total: file.document.page_count})" loading="lazy"/>
        </figure>
      </section>

      <!-- Markdown Text -->
      <MessageContent v-else-if="file.extension === 'md'" :content="text"/>

      <!-- Sandboxed HTML Prototype Viewport -->
      <div v-else-if="file.extension === 'html'" class="html-viewport-stage" :class="[viewportMode, {landscape: isLandscape}]">
        <div class="html-device-frame" :style="getViewportDimensions() ? {width: `${getViewportDimensions().width}px`, height: `${getViewportDimensions().height}px`} : {}">
          <iframe
            :key="htmlReloadKey"
            class="dsh-html-preview"
            :title="address.path"
            :sandbox="artifactHtmlSandbox"
            referrerpolicy="no-referrer"
            :srcdoc="artifactHtmlDocument(text, file.resources || [])"
          />
        </div>
      </div>

      <!-- Plain Text -->
      <pre v-else>{{ text }}</pre>
    </div>

    <footer class="dsh-preview-footer">
      <button type="button" class="footer-quote-btn" @click="quoteForFeedback">{{ t('quoteFeedbackBtn') }}</button>
      <a :href="url" :download="address.path.split('/').pop()">{{ t('filePreviewDownload') }}</a>
      <span v-if="imgNatural.width > 0 && imageTypes[file.extension]" class="footer-spec">{{ imgNatural.width }} × {{ imgNatural.height }} px · </span>
      <small :title="file.version">{{ Math.ceil(file.bytes / 1024) }} KB</small>
    </footer>
  </template>
</dialog>
</template>

<style scoped>
.document-pages figure{margin:0 0 16px}
.document-pages figcaption{color:var(--text-secondary);padding:8px 0}
.document-pages img{display:block;width:100%;height:auto}

/* Toolbar */
.quote-feedback-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  background: var(--accent-primary);
  color: var(--bg-primary);
  border-color: var(--accent-primary);
  font-weight: 600;
}
.quote-feedback-btn:hover {
  filter: brightness(1.1);
}
.footer-quote-btn {
  background: transparent;
  border: 0;
  color: var(--accent-primary);
  cursor: pointer;
  padding: 0;
  text-decoration: underline;
  font-size: var(--text-xs);
}
.footer-quote-btn:hover {
  filter: brightness(1.15);
}
.dsh-inspector-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  padding: 8px 12px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  margin-block: 8px;
}
.toolbar-group {
  display: flex;
  align-items: center;
  gap: 4px;
}
.toolbar-label {
  font-size: 11px;
  color: var(--text-secondary);
  margin-right: 2px;
}
.toolbar-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  font-size: 11px;
  font-family: inherit;
  border-radius: 6px;
  border: 1px solid var(--border-color);
  background: var(--bg-primary);
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s ease;
}
.toolbar-btn:hover {
  background: var(--bg-tertiary, var(--bg-secondary));
  color: var(--text-primary);
}
.toolbar-btn.active {
  background: color-mix(in srgb, var(--accent-primary) 18%, var(--bg-primary));
  color: var(--accent-primary);
  border-color: var(--accent-primary);
  font-weight: 600;
}
.toolbar-btn.reload-btn {
  color: var(--accent-primary);
}
.toolbar-spec-pill {
  font-size: 11px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  padding: 3px 6px;
  border-radius: 4px;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
  color: var(--text-secondary);
}

/* Background stages for Image Inspector */
.canvas-checkerboard {
  background-color: #141820;
  background-image: 
    linear-gradient(45deg, #0e1218 25%, transparent 25%),
    linear-gradient(-45deg, #0e1218 25%, transparent 25%),
    linear-gradient(45deg, transparent 75%, #0e1218 75%),
    linear-gradient(-45deg, transparent 75%, #0e1218 75%);
  background-size: 20px 20px;
  background-position: 0 0, 0 10px, 10px -10px, -10px 0px;
}
.canvas-dark {
  background-color: #05070a;
}
.canvas-light {
  background-color: #ffffff;
}

/* Image Stage */
.image-stage {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 240px;
  padding: 16px;
  border-radius: 8px;
  overflow: auto;
}
.image-stage.zoom-fit img {
  max-width: 100%;
  max-height: 68dvh;
  object-fit: contain;
  box-shadow: 0 4px 20px rgba(0,0,0,0.3);
}
.image-stage.zoom-100 img {
  max-width: none;
  max-height: none;
  box-shadow: 0 8px 30px rgba(0,0,0,0.4);
}
.image-stage.zoom-200 img {
  max-width: none;
  max-height: none;
  transform: scale(2);
  transform-origin: center center;
  margin: 15%;
  image-rendering: pixelated;
}

/* HTML Viewport Stage */
.html-viewport-stage {
  display: flex;
  justify-content: center;
  align-items: flex-start;
  width: 100%;
  overflow: auto;
  padding-block: 8px;
}
.html-viewport-stage.responsive {
  padding: 0;
}
.html-viewport-stage.responsive .html-device-frame {
  width: 100%;
  height: 68dvh;
  border: none;
  box-shadow: none;
}
.html-device-frame {
  border: 2px solid var(--border-color);
  border-radius: 12px;
  background: var(--bg-primary);
  box-shadow: 0 12px 40px rgba(0,0,0,0.45);
  overflow: hidden;
  max-width: 100%;
  transition: width 0.25s ease, height 0.25s ease;
}
.dsh-html-preview {
  display: block;
  width: 100%;
  height: 100%;
  min-height: 65dvh;
  border: none;
  background: var(--bg-primary);
}

.dsh-preview::backdrop {
  background: color-mix(in srgb, var(--bg-root, #0b0f17) 78%, transparent);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
}
.dsh-preview {
  width: min(1180px, 96vw);
  height: 92dvh;
  max-height: 96dvh;
  box-sizing: border-box;
  padding: max(16px, calc(env(safe-area-inset-top, 0px) + 12px)) 16px max(16px, calc(env(safe-area-inset-bottom, 0px) + 12px));
  border: 1px solid var(--border-color);
  border-radius: 12px;
  background: var(--bg-primary);
  color: var(--text-primary);
  box-shadow: 0 24px 64px rgba(0,0,0,0.65), 0 0 0 1px var(--border-color);
  overflow: auto;
  scrollbar-gutter: stable;
}

header, footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
header {
  position: sticky;
  top: 0;
  z-index: 2;
  background: var(--bg-primary);
  padding-block: 8px;
}
header strong {
  min-width: 0;
  flex: 1;
  text-align: center;
  overflow-wrap: anywhere;
}
header button {
  flex-shrink: 0;
  padding: 6px 12px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-primary);
  cursor: pointer;
}
.preview-status-line, small {
  color: var(--text-secondary);
}
.preview-warning {
  color: var(--color-warning, #f59e0b);
}
.preview-error {
  color: var(--color-error, #ef4444);
}
footer {
  margin-top: 16px;
}
footer a {
  color: var(--accent-primary);
  white-space: nowrap;
}
.footer-spec {
  color: var(--text-secondary);
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
}

.dsh-preview-body {
  overflow: auto;
  margin-top: 8px;
  border-radius: 8px;
}
.dsh-preview-body.zoomed {
  max-height: 75dvh;
}
pre {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

@media(max-width: 600px) {
  .dsh-preview {
    width: 100%;
    height: 100dvh;
    max-width: 100%;
    max-height: 100dvh;
    margin: 0;
    border-radius: 0;
  }
}
</style>
