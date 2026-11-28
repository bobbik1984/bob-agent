export const CHUNK_UPLOAD_THRESHOLD = 64 * 1024;
export const DEFAULT_CHUNK_BYTES = 96 * 1024;
export const MAX_COMPAT_CHUNKS = 128;
export const MAX_ATTACHMENT_BYTES = 50 * 1024 * 1024;
export const IMAGE_COMPRESS_THRESHOLD = 180 * 1024;
export const IMAGE_MAX_EDGE = 2048;

const NON_RETRYABLE_ERRORS = new Set([
  'UPLOAD_CANCELLED',
  'ATTACHMENT_TOO_LARGE',
  'ATTACHMENT_UPLOAD_ID_INVALID',
  'ATTACHMENT_CHUNK_INVALID',
  'ATTACHMENT_CHUNK_EMPTY',
  'ATTACHMENT_CHUNK_TOO_LARGE',
  'ATTACHMENT_CHECKSUM_MISMATCH',
  'NODE_AUTH_REQUIRED',
  'NODE_SCOPE_DENIED',
  'DSH_CONNECTION_NOT_FOUND',
  'DSH_ADDRESS_INVALID',
  'NATIVE_HOST_REQUIRED',
]);

export function bytesToBase64(bytes) {
  let bin = '';
  const step = 0x8000;
  for (let i = 0; i < bytes.length; i += step) {
    bin += String.fromCharCode.apply(null, bytes.subarray(i, i + step));
  }
  return btoa(bin);
}

export function base64ToBytes(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) {
    out[i] = bin.charCodeAt(i);
  }
  return out;
}

export function computeChunkBytes(totalSize) {
  const size = Math.max(1, Number(totalSize) || 0);
  // Keep chunks at 96 KB for files <= 12 MB (<= 128 chunks) so cross-border / Tailscale
  // links finish each chunk well inside RPC timeouts; above 12 MB scale up so totalChunks
  // stays <= 128 for backward compatibility with older bob-node versions.
  return Math.max(DEFAULT_CHUNK_BYTES, Math.ceil(size / MAX_COMPAT_CHUNKS));
}

export function isCompressibleRasterImage(type, name = '') {
  const mime = String(type || '').toLowerCase().trim();
  const lowerName = String(name || '').toLowerCase();
  if (mime === 'image/gif' || mime === 'image/svg+xml' || /\.(gif|svg)$/i.test(lowerName)) {
    return false;
  }
  if (mime.startsWith('image/')) return true;
  return /\.(png|jpe?g|webp|bmp)$/i.test(lowerName);
}

export function replaceImageExtension(name, ext = '.jpg') {
  const raw = String(name || '').trim() || `image_${Date.now()}${ext}`;
  if (/\.(png|jpe?g|webp|bmp)$/i.test(raw)) {
    return raw.replace(/\.(png|jpe?g|webp|bmp)$/i, ext);
  }
  return `${raw}${ext}`;
}

async function compressImageInBrowser(blob, origBytes) {
  if (
    typeof createImageBitmap !== 'function' ||
    typeof document === 'undefined' ||
    typeof document.createElement !== 'function'
  ) {
    return null;
  }
  let bmp = null;
  try {
    bmp = await createImageBitmap(blob);
    const srcW = bmp.width || 1;
    const srcH = bmp.height || 1;
    const maxSide = Math.max(srcW, srcH);
    const scale = maxSide > IMAGE_MAX_EDGE ? IMAGE_MAX_EDGE / maxSide : 1;
    const w = Math.max(1, Math.round(srcW * scale));
    const h = Math.max(1, Math.round(srcH * scale));
    const canvas = document.createElement('canvas');
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    // Fill opaque backdrop before JPEG encoding so transparent regions never turn pure black.
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, w, h);
    ctx.drawImage(bmp, 0, 0, w, h);

    const encodeJpeg = quality =>
      new Promise(resolve => {
        if (typeof canvas.toBlob === 'function') {
          canvas.toBlob(
            b => {
              if (!b) return resolve(null);
              b.arrayBuffer().then(ab => resolve(new Uint8Array(ab)), () => resolve(null));
            },
            'image/jpeg',
            quality,
          );
          return;
        }
        try {
          const dataUrl = canvas.toDataURL('image/jpeg', quality);
          const idx = dataUrl.indexOf(',');
          resolve(idx > 0 ? base64ToBytes(dataUrl.slice(idx + 1)) : null);
        } catch {
          resolve(null);
        }
      });

    let candidate = await encodeJpeg(0.85);
    if (candidate && candidate.length > 600 * 1024) {
      const tighter = await encodeJpeg(0.74);
      if (tighter && tighter.length > 0 && tighter.length < candidate.length) {
        candidate = tighter;
      }
    }
    if (candidate && candidate.length > 0 && candidate.length < origBytes.length * 0.88) {
      return { bytes: candidate, type: 'image/jpeg', ext: '.jpg' };
    }
    return null;
  } catch {
    return null;
  } finally {
    try {
      bmp?.close?.();
    } catch {}
  }
}

export async function prepareAttachmentBinary(file, opts = {}) {
  const origName = file?.name || `attachment_${Date.now()}`;
  const origType = file?.type || '';
  if (!file || typeof file.arrayBuffer !== 'function') {
    return {
      bytes: new Uint8Array(0),
      size: file?.size || 0,
      name: origName,
      type: origType,
      compressed: false,
      originalSize: file?.size || 0,
    };
  }
  const ab = await file.arrayBuffer();
  const rawBytes = new Uint8Array(ab);
  const originalSize = rawBytes.length;

  if (originalSize > IMAGE_COMPRESS_THRESHOLD && isCompressibleRasterImage(origType, origName)) {
    const compressFn = opts.compressImage || compressImageInBrowser;
    const blob =
      typeof Blob !== 'undefined'
        ? new Blob([rawBytes], { type: origType || 'image/png' })
        : file;
    const compressed = await compressFn(blob, rawBytes, { name: origName, type: origType });
    if (compressed?.bytes?.length > 0 && compressed.bytes.length < originalSize) {
      return {
        bytes: compressed.bytes,
        size: compressed.bytes.length,
        name: replaceImageExtension(origName, compressed.ext || '.jpg'),
        type: compressed.type || 'image/jpeg',
        compressed: true,
        originalSize,
      };
    }
  }

  return {
    bytes: rawBytes,
    size: rawBytes.length,
    name: origName,
    type: origType,
    compressed: false,
    originalSize,
  };
}

function defaultSleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function isRetryableError(err) {
  const code = String(err?.message || err || '').trim();
  if (!code) return true;
  if (NON_RETRYABLE_ERRORS.has(code)) return false;
  if (code === 'HTTP_400' || code === 'HTTP_401' || code === 'HTTP_403' || code === 'HTTP_413') {
    return false;
  }
  return true;
}

async function callWithRetry(fn, { maxRetries = 3, retryDelays = [500, 1200, 2500], sleep = defaultSleep, isCancelled }) {
  let lastErr = null;
  for (let attempt = 0; attempt < maxRetries; attempt++) {
    if (isCancelled?.()) throw new Error('UPLOAD_CANCELLED');
    try {
      return await fn(attempt);
    } catch (err) {
      lastErr = err;
      if (isCancelled?.()) throw new Error('UPLOAD_CANCELLED');
      if (!isRetryableError(err) || attempt + 1 >= maxRetries) {
        throw err;
      }
      const delay = retryDelays[Math.min(attempt, retryDelays.length - 1)] ?? 1000;
      await sleep(delay);
    }
  }
  throw lastErr || new Error('UPLOAD_FAILED');
}

export async function uploadAttachmentChunks({
  connectionId,
  bytes,
  name,
  type,
  uploadId,
  uploadedChunks = new Set(),
  chunkUpload,
  finalize,
  onProgress,
  isCancelled,
  maxRetries = 3,
  retryDelays = [500, 1200, 2500],
  sleep = defaultSleep,
}) {
  if (!bytes || bytes.length <= 0 || bytes.length > MAX_ATTACHMENT_BYTES) {
    throw new Error('ATTACHMENT_TOO_LARGE');
  }
  if (!connectionId || typeof chunkUpload !== 'function' || typeof finalize !== 'function') {
    throw new Error('UPLOAD_UNAVAILABLE');
  }
  const chunkBytes = computeChunkBytes(bytes.length);
  const totalChunks = Math.max(1, Math.ceil(bytes.length / chunkBytes));
  const id = uploadId || `up_${crypto.randomUUID().replace(/-/g, '')}`;

  const reportProgress = () => {
    const pct = Math.min(95, Math.round((uploadedChunks.size / totalChunks) * 95));
    onProgress?.(pct);
  };
  reportProgress();

  for (let idx = 0; idx < totalChunks; idx++) {
    if (isCancelled?.()) throw new Error('UPLOAD_CANCELLED');
    if (uploadedChunks.has(idx)) continue;
    const start = idx * chunkBytes;
    const slice = bytes.subarray(start, Math.min(bytes.length, start + chunkBytes));
    const dataBase64 = bytesToBase64(slice);
    await callWithRetry(
      () =>
        chunkUpload({
          connectionId,
          uploadId: id,
          chunkIndex: idx,
          totalChunks,
          totalSize: bytes.length,
          name,
          type,
          dataBase64,
        }),
      { maxRetries, retryDelays, sleep, isCancelled },
    );
    uploadedChunks.add(idx);
    reportProgress();
  }

  const fin = await callWithRetry(
    () =>
      finalize({
        connectionId,
        uploadId: id,
        totalChunks,
        name,
        type,
      }),
    { maxRetries, retryDelays, sleep, isCancelled },
  );
  onProgress?.(100);
  return {
    uploadId: fin?.upload_id || id,
    sha256: fin?.sha256 || '',
    totalChunks,
    chunkBytes,
  };
}
