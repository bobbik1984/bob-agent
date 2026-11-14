<script setup>
import { computed, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { decodeQrFromImageSource, extractPairingCode } from '../qr-image-decoder.mjs';

defineProps({
  inline: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits(['detected', 'close']);
const { t } = useI18n();

const videoEl = ref(null);
const cameraActive = ref(false);
const busy = ref(false);
const errorKey = ref('');
const manualText = ref('');

let activeStream = null;
let scanTimer = null;
let scanningFrame = false;

const cameraSupported = computed(
  () => typeof navigator !== 'undefined' && Boolean(navigator.mediaDevices?.getUserMedia),
);

function stopCamera() {
  if (scanTimer) {
    clearTimeout(scanTimer);
    scanTimer = null;
  }
  scanningFrame = false;
  if (activeStream) {
    for (const track of activeStream.getTracks()) {
      track.stop();
    }
    activeStream = null;
  }
  if (videoEl.value) {
    videoEl.value.srcObject = null;
  }
  cameraActive.value = false;
}

function acceptDecoded(raw) {
  const clean = extractPairingCode(raw) || String(raw || '').trim();
  if (!clean) return false;
  stopCamera();
  errorKey.value = '';
  emit('detected', clean);
  return true;
}

function scheduleNextFrame() {
  if (!cameraActive.value) return;
  scanTimer = setTimeout(async () => {
    if (!cameraActive.value || !videoEl.value || scanningFrame) {
      scheduleNextFrame();
      return;
    }
    scanningFrame = true;
    try {
      if (videoEl.value.readyState >= 2) {
        const hit = await decodeQrFromImageSource(videoEl.value);
        if (hit && acceptDecoded(hit)) return;
      }
    } catch {
      /* Continue polling frames until stopped. */
    } finally {
      scanningFrame = false;
    }
    scheduleNextFrame();
  }, 320);
}

async function startCamera() {
  if (!cameraSupported.value || busy.value) return;
  errorKey.value = '';
  busy.value = true;
  try {
    activeStream = await navigator.mediaDevices.getUserMedia({
      video: { facingMode: 'environment' },
      audio: false,
    });
    cameraActive.value = true;
    await Promise.resolve();
    if (videoEl.value) {
      videoEl.value.srcObject = activeStream;
      await videoEl.value.play().catch(() => {});
    }
    scheduleNextFrame();
  } catch (err) {
    stopCamera();
    const name = String(err?.name || '');
    errorKey.value =
      name === 'NotAllowedError' || name === 'PermissionDeniedError'
        ? 'qrCameraDenied'
        : 'qrCameraUnavailable';
  } finally {
    busy.value = false;
  }
}

async function decodeImageBlob(blob) {
  if (!blob) return;
  busy.value = true;
  errorKey.value = '';
  try {
    if (typeof createImageBitmap === 'function') {
      const bmp = await createImageBitmap(blob);
      try {
        const raw = await decodeQrFromImageSource(bmp, { width: bmp.width, height: bmp.height });
        if (raw && acceptDecoded(raw)) return;
      } finally {
        bmp.close?.();
      }
    } else if (typeof Image !== 'undefined') {
      const url = URL.createObjectURL(blob);
      try {
        const img = await new Promise((resolve, reject) => {
          const el = new Image();
          el.onload = () => resolve(el);
          el.onerror = reject;
          el.src = url;
        });
        const raw = await decodeQrFromImageSource(img);
        if (raw && acceptDecoded(raw)) return;
      } finally {
        URL.revokeObjectURL(url);
      }
    }
    errorKey.value = 'qrDecodeImageFailed';
  } catch {
    errorKey.value = 'qrDecodeImageFailed';
  } finally {
    busy.value = false;
  }
}

function onFileChange(event) {
  const file = event?.target?.files?.[0];
  if (file) void decodeImageBlob(file);
  if (event?.target) event.target.value = '';
}

function onPaste(event) {
  const items = event?.clipboardData?.items;
  if (!items) return;
  for (const item of items) {
    if (item.type?.startsWith('image/')) {
      const blob = item.getAsFile();
      if (blob) {
        event.preventDefault();
        void decodeImageBlob(blob);
        return;
      }
    }
  }
}

function submitManual() {
  errorKey.value = '';
  const code = extractPairingCode(manualText.value) || manualText.value.trim();
  if (!code) {
    errorKey.value = 'qrInvalid';
    return;
  }
  acceptDecoded(code);
}

function handleClose() {
  stopCamera();
  emit('close');
}

onUnmounted(stopCamera);
</script>

<template>
  <div
    :class="inline ? 'qr-scanner-inline' : 'qr-scanner-backdrop'"
    :role="inline ? 'region' : 'dialog'"
    :aria-modal="inline ? undefined : 'true'"
    :aria-label="t('qrScan')"
    @paste="onPaste"
  >
    <div :class="inline ? 'qr-scanner-body' : 'qr-scanner-dialog'">
      <div v-if="!inline" class="qr-scanner-header">
        <h3>{{ t('qrScan') }}</h3>
        <button type="button" class="qr-scanner-close" :aria-label="t('qrClose')" @click="handleClose">
          ×
        </button>
      </div>

      <p class="view-card-desc">{{ t('qrScanWebHint') }}</p>

      <div v-if="cameraSupported" class="qr-scanner-camera-box">
        <video
          v-show="cameraActive"
          ref="videoEl"
          class="qr-scanner-video"
          autoplay
          playsinline
          muted
        ></video>
        <div class="qr-scanner-actions">
          <button
            v-if="!cameraActive"
            type="button"
            class="qr-scanner-primary-btn"
            :disabled="busy"
            @click="startCamera"
          >
            {{ t('qrStartCameraBtn') }}
          </button>
          <button
            v-else
            type="button"
            class="qr-scanner-secondary-btn"
            @click="stopCamera"
          >
            {{ t('qrStopCameraBtn') }}
          </button>
          <label class="qr-scanner-upload-btn">
            <input type="file" accept="image/*" class="qr-scanner-file-input" :disabled="busy" @change="onFileChange" />
            <span>{{ t('qrUploadImageBtn') }}</span>
          </label>
        </div>
      </div>

      <div v-else class="qr-scanner-actions">
        <label class="qr-scanner-upload-btn">
          <input type="file" accept="image/*" class="qr-scanner-file-input" :disabled="busy" @change="onFileChange" />
          <span>{{ t('qrUploadImageBtn') }}</span>
        </label>
      </div>

      <div class="qr-scanner-manual">
        <label class="qr-scanner-label">
          <span>{{ t('qrPasteUriLabel') }}</span>
          <textarea
            v-model="manualText"
            rows="2"
            class="qr-scanner-textarea"
            :placeholder="t('qrPasteUriPlaceholder')"
            :disabled="busy"
            @keydown.enter.exact.prevent="submitManual"
          ></textarea>
        </label>
        <button
          type="button"
          class="qr-scanner-secondary-btn"
          :disabled="busy || !manualText.trim()"
          @click="submitManual"
        >
          {{ t('qrUseUriBtn') }}
        </button>
      </div>

      <p v-if="errorKey" role="alert" class="qr-scanner-error">{{ t(errorKey) }}</p>
    </div>
  </div>
</template>

<style scoped>
.qr-scanner-backdrop {
  position: fixed;
  inset: 0;
  z-index: 1200;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1rem;
  background: color-mix(in srgb, var(--bg-root) 80%, Canvas);
}

.qr-scanner-dialog {
  width: 100%;
  max-width: 26rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 1.25rem;
  border-radius: 0.75rem;
  background: var(--bg-primary);
  border: 1px solid var(--border-color);
}

.qr-scanner-body {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.qr-scanner-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
}

.qr-scanner-header h3 {
  margin: 0;
  font-size: 1rem;
  color: var(--text-primary);
}

.qr-scanner-close {
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-primary);
  border-radius: 0.375rem;
  padding: 0.25rem 0.625rem;
  cursor: pointer;
}

.qr-scanner-camera-box {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.qr-scanner-video {
  width: 100%;
  max-height: 14rem;
  object-fit: cover;
  border-radius: 0.5rem;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
}

.qr-scanner-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
}

.qr-scanner-primary-btn,
.qr-scanner-secondary-btn,
.qr-scanner-upload-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0.5rem 0.875rem;
  border-radius: 0.5rem;
  font-size: 0.8125rem;
  font-weight: 600;
  cursor: pointer;
}

.qr-scanner-primary-btn {
  border: 1px solid var(--accent-primary);
  background: var(--accent-primary);
  color: var(--text-on-accent);
}

.qr-scanner-secondary-btn,
.qr-scanner-upload-btn {
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-primary);
}

.qr-scanner-file-input {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
  pointer-events: none;
}

.qr-scanner-manual {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.qr-scanner-label {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  font-size: 0.8125rem;
  color: var(--text-secondary);
}

.qr-scanner-textarea {
  width: 100%;
  box-sizing: border-box;
  padding: 0.5rem 0.625rem;
  border-radius: 0.5rem;
  border: 1px solid var(--border-color);
  background: var(--bg-secondary);
  color: var(--text-primary);
  font-size: 0.8125rem;
  resize: vertical;
}

.qr-scanner-error {
  margin: 0;
  font-size: 0.8125rem;
  color: var(--color-error);
}
</style>
