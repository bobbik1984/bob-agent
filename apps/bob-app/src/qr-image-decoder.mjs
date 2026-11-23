// Zero-dependency QR image decoder (Versions 1-10) + pairing URI extractor.

const TOTAL_CW = [0, 26, 44, 70, 100, 134, 172, 196, 242, 292, 346];
const EC_BLOCKS = [
  null,
  [[7, 1], [10, 1], [13, 1], [17, 1]],
  [[10, 1], [16, 1], [22, 1], [28, 1]],
  [[15, 1], [26, 1], [18, 2], [22, 2]],
  [[20, 1], [18, 2], [26, 2], [16, 4]],
  [[26, 1], [24, 2], [18, 4], [22, 4]],
  [[18, 2], [16, 4], [24, 4], [28, 4]],
  [[20, 2], [18, 4], [18, 6], [26, 5]],
  [[24, 2], [22, 4], [22, 6], [26, 6]],
  [[30, 2], [22, 5], [20, 8], [24, 8]],
  [[18, 4], [26, 5], [24, 8], [28, 8]],
];
const ALIGN_CENTERS = [
  [],
  [],
  [6, 18],
  [6, 22],
  [6, 26],
  [6, 30],
  [6, 34],
  [6, 22, 38],
  [6, 24, 42],
  [6, 26, 46],
  [6, 28, 50],
];
const ALPHANUMERIC_CHARS = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:';

function buildFormatTable() {
  const out = [];
  for (let d = 0; d < 32; d += 1) {
    let rem = d << 10;
    for (let bit = 14; bit >= 10; bit -= 1) {
      if ((rem >>> bit) & 1) rem ^= 0x537 << (bit - 10);
    }
    out.push(((d << 10) | rem) ^ 0x5412);
  }
  return out;
}
const FORMAT_TABLE = buildFormatTable();

function popcount15(v) {
  let c = 0;
  for (let x = v & 0x7fff; x; x &= x - 1) c += 1;
  return c;
}

function decodeFormatWord(rawBits) {
  let bestIdx = -1;
  let bestDist = 4;
  for (let d = 0; d < 32; d += 1) {
    const dist = popcount15(rawBits ^ FORMAT_TABLE[d]);
    if (dist < bestDist) {
      bestDist = dist;
      bestIdx = d;
      if (dist === 0) break;
    }
  }
  if (bestIdx < 0) return null;
  const ecBits = (bestIdx >> 3) & 3;
  const ecIdx = [1, 0, 3, 2][ecBits];
  const mask = bestIdx & 7;
  return { ecIdx, mask };
}

function applyMaskBit(mask, r, c, bit) {
  let flip = false;
  switch (mask) {
    case 0: flip = (r + c) % 2 === 0; break;
    case 1: flip = r % 2 === 0; break;
    case 2: flip = c % 3 === 0; break;
    case 3: flip = (r + c) % 3 === 0; break;
    case 4: flip = (Math.floor(r / 2) + Math.floor(c / 3)) % 2 === 0; break;
    case 5: flip = ((r * c) % 2 + (r * c) % 3) === 0; break;
    case 6: flip = (((r * c) % 2 + (r * c) % 3) % 2) === 0; break;
    case 7: flip = (((r + c) % 2 + (r * c) % 3) % 2) === 0; break;
    default: break;
  }
  return flip ? bit ^ 1 : bit;
}

function makeReservedGrid(version) {
  const size = 17 + 4 * version;
  const res = new Uint8Array(size * size);
  const mark = (r, c) => {
    if (r >= 0 && r < size && c >= 0 && c < size) res[r * size + c] = 1;
  };
  for (let r = 0; r <= 8; r += 1) {
    for (let c = 0; c <= 8; c += 1) mark(r, c);
  }
  for (let r = 0; r <= 8; r += 1) {
    for (let c = size - 8; c < size; c += 1) mark(r, c);
  }
  for (let r = size - 8; r < size; r += 1) {
    for (let c = 0; c <= 8; c += 1) mark(r, c);
  }
  for (let i = 0; i < size; i += 1) {
    mark(6, i);
    mark(i, 6);
  }
  const centers = ALIGN_CENTERS[version] || [];
  for (const ar of centers) {
    for (const ac of centers) {
      if ((ar === 6 && ac === 6) || (ar === 6 && ac === size - 7) || (ar === size - 7 && ac === 6)) continue;
      for (let dr = -2; dr <= 2; dr += 1) {
        for (let dc = -2; dc <= 2; dc += 1) mark(ar + dr, ac + dc);
      }
    }
  }
  if (version >= 7) {
    for (let r = 0; r < 6; r += 1) {
      for (let c = size - 11; c <= size - 9; c += 1) {
        mark(r, c);
        mark(c, r);
      }
    }
  }
  return res;
}

function finderScore(matrix, size) {
  const origins = [[0, 0], [0, size - 7], [size - 7, 0]];
  let matches = 0;
  for (const [or0, oc0] of origins) {
    for (let dr = 0; dr < 7; dr += 1) {
      for (let dc = 0; dc < 7; dc += 1) {
        const expected =
          dr === 0 || dr === 6 || dc === 0 || dc === 6 || (dr >= 2 && dr <= 4 && dc >= 2 && dc <= 4)
            ? 1
            : 0;
        if (matrix[(or0 + dr) * size + (oc0 + dc)] === expected) matches += 1;
      }
    }
  }
  return matches;
}

function readFormatWords(matrix, size) {
  const coords1 = [
    [8, 0], [8, 1], [8, 2], [8, 3], [8, 4], [8, 5], [8, 7], [8, 8],
    [7, 8], [5, 8], [4, 8], [3, 8], [2, 8], [1, 8], [0, 8],
  ];
  let w1 = 0;
  for (const [r, c] of coords1) w1 = (w1 << 1) | matrix[r * size + c];
  const d1 = decodeFormatWord(w1);
  if (d1) return d1;

  const coords2 = [
    [size - 1, 8], [size - 2, 8], [size - 3, 8], [size - 4, 8], [size - 5, 8], [size - 6, 8], [size - 7, 8],
    [8, size - 8], [8, size - 7], [8, size - 6], [8, size - 5], [8, size - 4], [8, size - 3], [8, size - 2], [8, size - 1],
  ];
  let w2 = 0;
  for (const [r, c] of coords2) w2 = (w2 << 1) | matrix[r * size + c];
  return decodeFormatWord(w2);
}

function decodeCodewords(version, ecIdx, codewords) {
  const totalCw = TOTAL_CW[version];
  const spec = EC_BLOCKS[version]?.[ecIdx];
  if (!totalCw || !spec || codewords.length < totalCw) return null;
  const [ecPerBlock, numBlocks] = spec;
  const totalData = totalCw - ecPerBlock * numBlocks;
  const g2Blocks = totalData % numBlocks;
  const g1Blocks = numBlocks - g2Blocks;
  const g1Len = Math.floor(totalData / numBlocks);

  const blocks = Array.from({ length: numBlocks }, () => []);
  let idx = 0;
  for (let col = 0; col < g1Len; col += 1) {
    for (let b = 0; b < numBlocks; b += 1) blocks[b].push(codewords[idx++]);
  }
  if (g2Blocks > 0) {
    for (let b = g1Blocks; b < numBlocks; b += 1) blocks[b].push(codewords[idx++]);
  }
  const data = Uint8Array.from(blocks.flat());

  let bitPos = 0;
  const totalBits = data.length * 8;
  const readBits = count => {
    if (bitPos + count > totalBits) return -1;
    let v = 0;
    for (let i = 0; i < count; i += 1) {
      const byteIdx = bitPos >> 3;
      const shift = 7 - (bitPos & 7);
      v = (v << 1) | ((data[byteIdx] >> shift) & 1);
      bitPos += 1;
    }
    return v;
  };

  const outChunks = [];
  while (bitPos + 4 <= totalBits) {
    const mode = readBits(4);
    if (mode <= 0) break;
    if (mode === 0b0111) {
      if (readBits(8) < 0) return null;
      continue;
    }
    if (mode === 0b0100) {
      const lenBits = version <= 9 ? 8 : 16;
      const len = readBits(lenBits);
      if (len < 0) return null;
      const bytes = new Uint8Array(len);
      for (let i = 0; i < len; i += 1) {
        const b = readBits(8);
        if (b < 0) return null;
        bytes[i] = b;
      }
      outChunks.push(new TextDecoder('utf-8', { fatal: false }).decode(bytes));
      continue;
    }
    if (mode === 0b0010) {
      const lenBits = version <= 9 ? 9 : 11;
      let count = readBits(lenBits);
      if (count < 0) return null;
      let s = '';
      while (count >= 2) {
        const pair = readBits(11);
        if (pair < 0) return null;
        s += ALPHANUMERIC_CHARS[Math.floor(pair / 45)] + ALPHANUMERIC_CHARS[pair % 45];
        count -= 2;
      }
      if (count === 1) {
        const ch = readBits(6);
        if (ch < 0) return null;
        s += ALPHANUMERIC_CHARS[ch];
      }
      outChunks.push(s);
      continue;
    }
    if (mode === 0b0001) {
      const lenBits = version <= 9 ? 10 : 12;
      let count = readBits(lenBits);
      if (count < 0) return null;
      let s = '';
      while (count >= 3) {
        const v = readBits(10);
        if (v < 0) return null;
        s += String(v).padStart(3, '0');
        count -= 3;
      }
      if (count === 2) {
        const v = readBits(7);
        if (v < 0) return null;
        s += String(v).padStart(2, '0');
      } else if (count === 1) {
        const v = readBits(4);
        if (v < 0) return null;
        s += String(v);
      }
      outChunks.push(s);
      continue;
    }
    return null;
  }

  const text = outChunks.join('');
  return text.length > 0 ? text : null;
}

function decodeSampledMatrix(matrix, version) {
  const size = 17 + 4 * version;
  if (finderScore(matrix, size) < 140) return null;
  const fmt = readFormatWords(matrix, size);
  if (!fmt) return null;
  const reserved = makeReservedGrid(version);
  const codewords = [];
  let currentByte = 0;
  let bitCount = 0;
  let upward = true;

  for (let rightCol = size - 1; rightCol > 0; rightCol -= 2) {
    if (rightCol === 6) rightCol -= 1;
    for (let step = 0; step < size; step += 1) {
      const r = upward ? size - 1 - step : step;
      for (let dc = 0; dc < 2; dc += 1) {
        const c = rightCol - dc;
        if (reserved[r * size + c]) continue;
        const bit = applyMaskBit(fmt.mask, r, c, matrix[r * size + c]);
        currentByte = (currentByte << 1) | bit;
        bitCount += 1;
        if (bitCount === 8) {
          codewords.push(currentByte);
          currentByte = 0;
          bitCount = 0;
        }
      }
    }
    upward = !upward;
  }

  return decodeCodewords(version, fmt.ecIdx, codewords);
}

function sampleBox(bin, width, height, ox, oy, spanW, spanH, version) {
  const size = 17 + 4 * version;
  const matrix = new Uint8Array(size * size);
  const modW = spanW / size;
  const modH = spanH / size;
  for (let r = 0; r < size; r += 1) {
    const py = Math.min(height - 1, Math.max(0, Math.round(oy + (r + 0.5) * modH)));
    for (let c = 0; c < size; c += 1) {
      const px = Math.min(width - 1, Math.max(0, Math.round(ox + (c + 0.5) * modW)));
      matrix[r * size + c] = bin[py * width + px];
    }
  }
  return matrix;
}

function findFinderCenters(bin, width, height) {
  const centers = [];
  const stepY = height > 420 ? 2 : 1;
  const matches11311 = (c0, c1, c2, c3, c4) => {
    const tot = c0 + c1 + c2 + c3 + c4;
    if (tot < 7) return false;
    const mod = tot / 7;
    const tol = mod * 0.65;
    return (
      Math.abs(c0 - mod) < tol &&
      Math.abs(c1 - mod) < tol &&
      Math.abs(c2 - 3 * mod) < 3 * tol * 0.85 &&
      Math.abs(c3 - mod) < tol &&
      Math.abs(c4 - mod) < tol
    );
  };
  const crossCheckV = (cx, cy, estMod) => {
    let top = cy;
    while (top >= 0 && bin[top * width + cx] === 1) top -= 1;
    let bot = cy;
    while (bot < height && bin[bot * width + cx] === 1) bot += 1;
    const c2 = bot - top - 1;
    if (c2 < 1.5 * estMod || c2 > 4.5 * estMod) return null;
    let y = top;
    while (y >= 0 && bin[y * width + cx] === 0) y -= 1;
    const c1 = top - y;
    let y0 = y;
    while (y0 >= 0 && bin[y0 * width + cx] === 1) y0 -= 1;
    const c0 = y - y0;
    let y3 = bot;
    while (y3 < height && bin[y3 * width + cx] === 0) y3 += 1;
    const c3 = y3 - bot;
    let y4 = y3;
    while (y4 < height && bin[y4 * width + cx] === 1) y4 += 1;
    const c4 = y4 - y3;
    if (!matches11311(c0, c1, c2, c3, c4)) return null;
    return (top + 1 + bot - 1) / 2;
  };

  for (let y = 0; y < height; y += stepY) {
    const runs = [];
    let cur = bin[y * width];
    let len = 1;
    let start = 0;
    for (let x = 1; x < width; x += 1) {
      const v = bin[y * width + x];
      if (v === cur) {
        len += 1;
      } else {
        runs.push([cur, len, start]);
        cur = v;
        len = 1;
        start = x;
      }
    }
    runs.push([cur, len, start]);
    for (let i = 0; i + 4 < runs.length; i += 1) {
      if (runs[i][0] !== 1) continue;
      const [c0, c1, c2, c3, c4] = [runs[i][1], runs[i + 1][1], runs[i + 2][1], runs[i + 3][1], runs[i + 4][1]];
      if (!matches11311(c0, c1, c2, c3, c4)) continue;
      const cx = Math.round(runs[i + 2][2] + c2 / 2);
      const estMod = (c0 + c1 + c2 + c3 + c4) / 7;
      const vy = crossCheckV(cx, y, estMod);
      if (vy === null) continue;
      let merged = false;
      for (const existing of centers) {
        if (Math.hypot(existing.x - cx, existing.y - vy) <= Math.max(4, estMod * 2)) {
          existing.x = (existing.x * existing.n + cx) / (existing.n + 1);
          existing.y = (existing.y * existing.n + vy) / (existing.n + 1);
          existing.mod = (existing.mod * existing.n + estMod) / (existing.n + 1);
          existing.n += 1;
          merged = true;
          break;
        }
      }
      if (!merged) centers.push({ x: cx, y: vy, mod: estMod, n: 1 });
    }
  }
  return centers;
}

function tryBinary(bin, width, height) {
  let minX = width;
  let minY = height;
  let maxX = -1;
  let maxY = -1;
  for (let y = 0; y < height; y += 1) {
    const rowOff = y * width;
    for (let x = 0; x < width; x += 1) {
      if (bin[rowOff + x]) {
        if (x < minX) minX = x;
        if (x > maxX) maxX = x;
        if (y < minY) minY = y;
        if (y > maxY) maxY = y;
      }
    }
  }
  if (maxX <= minX || maxY <= minY) return null;

  const finders = findFinderCenters(bin, width, height);
  if (finders.length >= 3) {
    const sorted = [...finders].sort((a, b) => b.n - a.n).slice(0, 3);
    const tl = [...sorted].sort((a, b) => (a.x + a.y) - (b.x + b.y))[0];
    const rest = sorted.filter(f => f !== tl);
    const [tr, bl] = rest[0].x > rest[1].x ? [rest[0], rest[1]] : [rest[1], rest[0]];
    const dx = tr.x - tl.x;
    const dy = bl.y - tl.y;
    if (dx > 8 && dy > 8) {
      for (let v = 1; v <= 10; v += 1) {
        const size = 17 + 4 * v;
        const modW = dx / (size - 7);
        const modH = dy / (size - 7);
        const ox = tl.x - 3.5 * modW;
        const oy = tl.y - 3.5 * modH;
        const matrix = sampleBox(bin, width, height, ox, oy, modW * size, modH * size, v);
        const decoded = decodeSampledMatrix(matrix, v);
        if (decoded) return decoded;
      }
    }
  }

  const spanW = maxX - minX + 1;
  const spanH = maxY - minY + 1;
  for (let v = 1; v <= 10; v += 1) {
    const matrix = sampleBox(bin, width, height, minX, minY, spanW, spanH, v);
    const decoded = decodeSampledMatrix(matrix, v);
    if (decoded) return decoded;
  }
  return null;
}

export function extractPairingCode(raw) {
  const text = String(raw || '').trim();
  if (!text) return null;
  const m = text.match(/(bob:\/\/pair\?[^\s"'<>]+)/i);
  return m ? m[1] : null;
}

export function decodeCleanQrRgba(rgba, width, height) {
  if (!rgba || width < 21 || height < 21 || rgba.length < width * height * 4) return null;
  const total = width * height;
  const lum = new Uint8Array(total);
  let minLum = 255;
  let maxLum = 0;
  for (let i = 0; i < total; i += 1) {
    const p = i * 4;
    const y = (77 * rgba[p] + 150 * rgba[p + 1] + 29 * rgba[p + 2]) >> 8;
    lum[i] = y;
    if (y < minLum) minLum = y;
    if (y > maxLum) maxLum = y;
  }
  if (maxLum - minLum < 16) return null;
  const threshold = (minLum + maxLum) >> 1;

  for (const invert of [false, true]) {
    const bin = new Uint8Array(total);
    for (let i = 0; i < total; i += 1) {
      bin[i] = invert ? (lum[i] > threshold ? 1 : 0) : (lum[i] < threshold ? 1 : 0);
    }
    const decoded = tryBinary(bin, width, height);
    if (decoded) return decoded;
  }
  return null;
}

let reuseCanvas = null;

export async function decodeQrFromImageSource(source, dims = {}) {
  if (!source) return null;
  if (typeof globalThis.BarcodeDetector === 'function') {
    try {
      const detector = new globalThis.BarcodeDetector({ formats: ['qr_code'] });
      const hits = await detector.detect(source);
      const raw = hits?.[0]?.rawValue;
      if (raw) return String(raw);
    } catch {
      /* Fall back to pure-JS decoder. */
    }
  }
  if (typeof document === 'undefined') return null;
  const srcW = Number(dims.width || source.videoWidth || source.naturalWidth || source.width || 0);
  const srcH = Number(dims.height || source.videoHeight || source.naturalHeight || source.height || 0);
  if (srcW < 21 || srcH < 21) return null;
  const scale = Math.max(srcW, srcH) > 720 ? 720 / Math.max(srcW, srcH) : 1;
  const w = Math.max(21, Math.round(srcW * scale));
  const h = Math.max(21, Math.round(srcH * scale));
  const canvas = reuseCanvas || (reuseCanvas = document.createElement('canvas'));
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) return null;
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(source, 0, 0, w, h);
  const imageData = ctx.getImageData(0, 0, w, h);
  return decodeCleanQrRgba(imageData.data, w, h);
}
