import test from 'node:test';
import assert from 'node:assert/strict';
import {
  DEFAULT_CHUNK_BYTES,
  IMAGE_COMPRESS_THRESHOLD,
  computeChunkBytes,
  isCompressibleRasterImage,
  prepareAttachmentBinary,
  replaceImageExtension,
  uploadAttachmentChunks,
} from '../src/attachment-upload.mjs';

test('computeChunkBytes keeps 96KB chunks up to 12MB and stays <= 128 chunks above 12MB', () => {
  assert.equal(computeChunkBytes(250 * 1024), DEFAULT_CHUNK_BYTES);
  assert.equal(computeChunkBytes(5 * 1024 * 1024), DEFAULT_CHUNK_BYTES);
  const bigSize = 32 * 1024 * 1024;
  const bigChunk = computeChunkBytes(bigSize);
  assert.ok(bigChunk > DEFAULT_CHUNK_BYTES);
  assert.ok(Math.ceil(bigSize / bigChunk) <= 128);
});

test('isCompressibleRasterImage skips gif and svg while allowing png/jpeg/webp/bmp', () => {
  assert.equal(isCompressibleRasterImage('image/png', 'shot.png'), true);
  assert.equal(isCompressibleRasterImage('image/jpeg', 'photo.jpg'), true);
  assert.equal(isCompressibleRasterImage('image/gif', 'anim.gif'), false);
  assert.equal(isCompressibleRasterImage('image/svg+xml', 'icon.svg'), false);
  assert.equal(isCompressibleRasterImage('application/pdf', 'doc.pdf'), false);
});

test('prepareAttachmentBinary compresses large raster images and renames extension to .jpg', async () => {
  const raw = new Uint8Array(IMAGE_COMPRESS_THRESHOLD + 50_000).fill(7);
  const compressed = new Uint8Array(80_000).fill(3);
  const fakeFile = {
    name: 'cdb40ec1-3a49-456d-a800.png',
    type: 'image/png',
    size: raw.length,
    async arrayBuffer() {
      return raw.buffer;
    },
  };
  const out = await prepareAttachmentBinary(fakeFile, {
    async compressImage() {
      return { bytes: compressed, type: 'image/jpeg', ext: '.jpg' };
    },
  });
  assert.equal(out.compressed, true);
  assert.equal(out.size, compressed.length);
  assert.equal(out.type, 'image/jpeg');
  assert.equal(out.name, 'cdb40ec1-3a49-456d-a800.jpg');
  assert.equal(out.originalSize, raw.length);
});

test('prepareAttachmentBinary caches raw bytes untouched for small images and non-images', async () => {
  const raw = new Uint8Array(40_000).fill(9);
  const fakeFile = {
    name: 'small.png',
    type: 'image/png',
    size: raw.length,
    async arrayBuffer() {
      return raw.buffer;
    },
  };
  let compressorCalled = false;
  const out = await prepareAttachmentBinary(fakeFile, {
    async compressImage() {
      compressorCalled = true;
      return null;
    },
  });
  assert.equal(compressorCalled, false);
  assert.equal(out.compressed, false);
  assert.equal(out.size, raw.length);
  assert.equal(out.name, 'small.png');
  assert.equal(replaceImageExtension('clipboard_img', '.jpg'), 'clipboard_img.jpg');
});

test('uploadAttachmentChunks retries transient failures and supports resumable retry without re-uploading completed chunks', async () => {
  const payload = new Uint8Array(DEFAULT_CHUNK_BYTES * 2 + 12345).fill(42);
  const seenChunks = [];
  const uploadedChunks = new Set();
  let chunk1Attempts = 0;

  // First run: chunk 1 fails all 2 attempts -> caller keeps uploadedChunks (contains chunk 0)
  await assert.rejects(
    uploadAttachmentChunks({
      connectionId: 'bobbik-pc',
      bytes: payload,
      name: 'shot.jpg',
      type: 'image/jpeg',
      uploadId: 'up_resume_test_01',
      uploadedChunks,
      maxRetries: 2,
      retryDelays: [1, 1],
      sleep: async () => {},
      async chunkUpload(args) {
        seenChunks.push(args.chunkIndex);
        if (args.chunkIndex === 1) {
          chunk1Attempts++;
          throw new Error('DSH_UNAVAILABLE');
        }
        return { ok: true };
      },
      async finalize() {
        return { ok: true, upload_id: 'up_resume_test_01', sha256: 'abc123' };
      },
    }),
    /DSH_UNAVAILABLE/,
  );

  assert.equal(chunk1Attempts, 2);
  assert.deepEqual([...uploadedChunks], [0]);

  // Second run (user clicks Retry): chunk 0 is skipped; chunks 1 and 2 upload; finalize retries once on timeout and succeeds!
  const retrySeen = [];
  let finalizeAttempts = 0;
  const progressLog = [];
  const res = await uploadAttachmentChunks({
    connectionId: 'bobbik-pc',
    bytes: payload,
    name: 'shot.jpg',
    type: 'image/jpeg',
    uploadId: 'up_resume_test_01',
    uploadedChunks,
    maxRetries: 3,
    retryDelays: [1, 1],
    sleep: async () => {},
    onProgress: pct => progressLog.push(pct),
    async chunkUpload(args) {
      retrySeen.push(args.chunkIndex);
      return { ok: true };
    },
    async finalize() {
      finalizeAttempts++;
      if (finalizeAttempts === 1) throw new Error('HTTP_504');
      return { ok: true, upload_id: 'up_resume_test_01', sha256: 'abc123' };
    },
  });

  assert.deepEqual(retrySeen, [1, 2]);
  assert.equal(finalizeAttempts, 2);
  assert.equal(res.uploadId, 'up_resume_test_01');
  assert.equal(res.sha256, 'abc123');
  assert.equal(progressLog.at(-1), 100);
});

test('uploadAttachmentChunks stops immediately on non-retryable error or cancellation', async () => {
  const payload = new Uint8Array(DEFAULT_CHUNK_BYTES + 100).fill(5);
  let attempts = 0;
  await assert.rejects(
    uploadAttachmentChunks({
      connectionId: 'bobbik-pc',
      bytes: payload,
      name: 'shot.jpg',
      type: 'image/jpeg',
      uploadId: 'up_nonretry_01',
      maxRetries: 3,
      retryDelays: [1, 1],
      sleep: async () => {},
      async chunkUpload() {
        attempts++;
        throw new Error('NODE_AUTH_REQUIRED');
      },
      async finalize() {
        return { ok: true };
      },
    }),
    /NODE_AUTH_REQUIRED/,
  );
  assert.equal(attempts, 1);

  let cancelled = false;
  let cancelAttempts = 0;
  await assert.rejects(
    uploadAttachmentChunks({
      connectionId: 'bobbik-pc',
      bytes: payload,
      name: 'shot.jpg',
      type: 'image/jpeg',
      uploadId: 'up_cancel_01',
      isCancelled: () => cancelled,
      async chunkUpload() {
        cancelAttempts++;
        cancelled = true;
        return { ok: true };
      },
      async finalize() {
        return { ok: true };
      },
    }),
    /UPLOAD_CANCELLED/,
  );
  assert.equal(cancelAttempts, 1);
});
