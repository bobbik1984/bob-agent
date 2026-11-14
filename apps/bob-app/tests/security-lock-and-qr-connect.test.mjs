import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import QRCode from 'qrcode';
import { parse, compileScript, compileTemplate } from 'vue/compiler-sfc';
import { useMobileSecurityLock } from '../src/composables/useMobileSecurityLock.mjs';
import { decodeCleanQrRgba, extractPairingCode } from '../src/qr-image-decoder.mjs';
import { nodeAddress, isPrivateOrLoopbackHost } from '../src/node-connection.mjs';
import { createKernelBridge } from '../src/kernel-bridge.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(__dirname, '..');

function renderQrMatrixToRgba(text, { scale = 5, margin = 12, invert = false, errorCorrectionLevel = 'M' } = {}) {
  const qr = QRCode.create(text, { errorCorrectionLevel });
  const size = qr.modules.size;
  const width = size * scale + margin * 2;
  const height = width;
  const rgba = new Uint8ClampedArray(width * height * 4);
  const bg = invert ? 20 : 252;
  const fg = invert ? 245 : 14;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const mx = Math.floor((x - margin) / scale);
      const my = Math.floor((y - margin) / scale);
      const dark = mx >= 0 && mx < size && my >= 0 && my < size && qr.modules.get(my, mx);
      const c = dark ? fg : bg;
      const p = (y * width + x) * 4;
      rgba[p] = c;
      rgba[p + 1] = c;
      rgba[p + 2] = c;
      rgba[p + 3] = 255;
    }
  }
  return { rgba, width, height, size };
}

test('useMobileSecurityLock enforces first-launch choice, skip bypass, cold-launch unlock, and Web/PC bypass', async () => {
  const webLock = useMobileSecurityLock({
    securityLockStatus: async () => ({
      supported: false,
      enrolled: false,
      decided: true,
      enabled: false,
      unlocked: true,
    }),
  });
  await webLock.refresh();
  assert.equal(webLock.isBlocked.value, false);
  assert.equal(webLock.needsDecision.value, false);
  assert.equal(webLock.needsUnlock.value, false);

  const calls = [];
  let serverCfg = { supported: true, enrolled: true, decided: false, enabled: false, unlocked: true };
  const androidBridge = {
    securityLockStatus: async () => ({ ...serverCfg }),
    securityLockSet: async payload => {
      calls.push({ op: 'set', payload });
      if (!payload.enabled && payload.skipSetup) {
        serverCfg = { ...serverCfg, decided: true, enabled: false, unlocked: true };
      } else if (payload.enabled) {
        serverCfg = { ...serverCfg, decided: true, enabled: true, unlocked: true };
      } else {
        serverCfg = { ...serverCfg, decided: true, enabled: false, unlocked: true };
      }
      return { ...serverCfg };
    },
    securityLockUnlock: async payload => {
      calls.push({ op: 'unlock', payload });
      serverCfg = { ...serverCfg, unlocked: true };
      return { ...serverCfg };
    },
  };

  const firstLaunch = useMobileSecurityLock(androidBridge);
  await firstLaunch.refresh();
  assert.equal(firstLaunch.needsDecision.value, true);
  assert.equal(firstLaunch.isBlocked.value, true);

  await firstLaunch.chooseFirstLaunch(false);
  assert.equal(calls[0].payload.skipSetup, true);
  assert.equal(calls[0].payload.enabled, false);
  assert.equal(firstLaunch.state.value.decided, true);
  assert.equal(firstLaunch.state.value.enabled, false);
  assert.equal(firstLaunch.isBlocked.value, false);

  // Simulate enabling later in settings, then cold launch locked
  await firstLaunch.toggleInSettings(true, { title: 'Unlock', subtitle: 'Verify' });
  assert.equal(firstLaunch.state.value.enabled, true);

  serverCfg.unlocked = false;
  const coldLaunch = useMobileSecurityLock(androidBridge);
  await coldLaunch.refresh();
  assert.equal(coldLaunch.needsDecision.value, false);
  assert.equal(coldLaunch.needsUnlock.value, true);
  assert.equal(coldLaunch.isBlocked.value, true);

  await coldLaunch.unlock({ title: 'Unlock', subtitle: 'Verify' });
  assert.equal(coldLaunch.isBlocked.value, false);
  assert.equal(coldLaunch.state.value.unlocked, true);

  // Failed auth preserves locked gate and exposes errorCode
  const failingBridge = {
    ...androidBridge,
    securityLockUnlock: async () => {
      throw new Error('SECURITY_LOCK_CANCELLED');
    },
  };
  serverCfg.unlocked = false;
  const lockedFail = useMobileSecurityLock(failingBridge);
  await lockedFail.refresh();
  await lockedFail.unlock();
  assert.equal(lockedFail.errorCode.value, 'SECURITY_LOCK_CANCELLED');
  assert.equal(lockedFail.isBlocked.value, true);
});

test('decodeCleanQrRgba decodes compact and large pairing QR codes across normal and dark-terminal polarity', () => {
  const uris = [
    'bob://pair?u=https%3A%2F%2Fbob-online.bobbik.org&i=aB3kL9mN2p&c=SecRetTokEn1234567890A',
    'bob://pair?u=http%3A%2F%2F100.64.0.5%3A9527&i=xY9zW1kL4m&c=AnotherStrongToken1234',
    'bob://pair?u=https%3A%2F%2Fvps3.example.org&i=1234567890&c=0123456789ABCDEFGHIJKLMNOPQRSTUV',
  ];

  for (const uri of uris) {
    for (const ecc of ['L', 'M', 'Q', 'H']) {
      for (const invert of [false, true]) {
        const rendered = renderQrMatrixToRgba(uri, {
          scale: invert ? 6 : 4,
          margin: invert ? 18 : 8,
          invert,
          errorCorrectionLevel: ecc,
        });
        const decoded = decodeCleanQrRgba(rendered.rgba, rendered.width, rendered.height);
        assert.equal(decoded, uri, `failed decoding ECC=${ecc} invert=${invert} len=${uri.length}`);
      }
    }
  }
});

test('extractPairingCode extracts bob://pair URI from terminal banner text', () => {
  const banner = `
----------------------------------------------------------------
  Bob Node Ready: VPS (https://bob-online.bobbik.org) [role: node]
  Pairing URI (10m single-use): bob://pair?u=https%3A%2F%2Fbob-online.bobbik.org&i=aB3kL9mN2p&c=SecRetTokEn1234567890A
----------------------------------------------------------------
`;
  assert.equal(
    extractPairingCode(banner),
    'bob://pair?u=https%3A%2F%2Fbob-online.bobbik.org&i=aB3kL9mN2p&c=SecRetTokEn1234567890A',
  );
  assert.equal(extractPairingCode('nothing here'), null);
});

test('nodeAddress accepts LAN/Tailscale HTTP and public HTTPS while rejecting insecure public HTTP', () => {
  assert.equal(isPrivateOrLoopbackHost('100.64.0.5'), true);
  assert.equal(isPrivateOrLoopbackHost('100.127.255.254'), true);
  assert.equal(isPrivateOrLoopbackHost('100.128.0.1'), false);
  assert.equal(isPrivateOrLoopbackHost('192.168.1.20'), true);
  assert.equal(isPrivateOrLoopbackHost('172.16.0.9'), true);
  assert.equal(isPrivateOrLoopbackHost('10.2.3.4'), true);

  assert.equal(nodeAddress('100.64.0.5:9527'), 'http://100.64.0.5:9527');
  assert.equal(nodeAddress('192.168.1.20:53020'), 'http://192.168.1.20:53020');
  assert.equal(nodeAddress('https://100.64.0.5:9527'), 'https://100.64.0.5:9527');
  assert.equal(nodeAddress('bob-online.bobbik.org'), 'https://bob-online.bobbik.org');
  assert.throws(() => nodeAddress('http://worker.example'), /NODE_HTTPS_REQUIRED/);
  assert.throws(() => nodeAddress('http://100.128.0.1:9527'), /NODE_HTTPS_REQUIRED/);
});

test('kernelBridge exposes nodeDiscover, nodeConnect, and securityLock RPC helpers', async () => {
  const invoked = [];
  const bridge = createKernelBridge(
    async (cmd, args) => {
      invoked.push({ cmd, args });
      return { ok: true };
    },
    () => true,
  );
  await bridge.nodeDiscover('100.64.0.5:9527');
  await bridge.nodeConnect({ address: 'http://100.64.0.5:9527', password: 'secret' });
  await bridge.securityLockStatus();
  await bridge.securityLockSet({ enabled: true, skipSetup: false });
  await bridge.securityLockUnlock({ title: 'Unlock' });

  assert.deepEqual(
    invoked.map(x => x.cmd),
    [
      'app_node_discover',
      'app_node_connect',
      'app_security_lock_status',
      'app_security_lock_set',
      'app_security_lock_unlock',
    ],
  );
});

test('Vue components compile cleanly, satisfy size/theme/emoji invariants, and locales match 100%', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;
  const hexColorRegex = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;
  const rgbColorRegex = /\brgba?\s*\(/i;

  const vueRelPaths = [
    'src/components/MobileSecurityGate.vue',
    'src/components/QrScanModal.vue',
    'src/NodeConnect.vue',
    'src/HomeConnection.vue',
    'src/views/SettingsView.vue',
    'src/App.vue',
  ];

  for (const rel of vueRelPaths) {
    const fullPath = path.join(APP_ROOT, rel);
    const source = fs.readFileSync(fullPath, 'utf8');
    assert.equal(emojiRegex.test(source), false, `${rel} must not contain decorative emoji`);
    assert.equal(hexColorRegex.test(source), false, `${rel} must not contain hardcoded hex colors`);
    assert.equal(rgbColorRegex.test(source), false, `${rel} must not contain hardcoded rgb/rgba colors`);

    const { descriptor, errors } = parse(source, { filename: fullPath });
    assert.equal(errors.length, 0, `${rel} SFC parse errors: ${errors.map(e => e.message).join('; ')}`);
    const compiled = compileScript(descriptor, { id: rel });
    assert.ok(compiled.content.length > 0, `${rel} compileScript produced output`);
    if (descriptor.template) {
      const tpl = compileTemplate({
        source: descriptor.template.content,
        filename: fullPath,
        id: rel,
        compilerOptions: { bindingMetadata: compiled.bindings },
      });
      assert.equal(tpl.errors.length, 0, `${rel} template compile errors: ${tpl.errors.join('; ')}`);
    }
    if (rel === 'src/App.vue') {
      const lineCount = source.split(/\r?\n/).length;
      assert.ok(lineCount < 500, `App.vue must stay under 500 lines (got ${lineCount})`);
    }
  }

  const zh = JSON.parse(fs.readFileSync(path.join(APP_ROOT, 'src/locales/zh-CN.json'), 'utf8'));
  const en = JSON.parse(fs.readFileSync(path.join(APP_ROOT, 'src/locales/en-US.json'), 'utf8'));
  assert.deepEqual(Object.keys(zh).sort(), Object.keys(en).sort(), 'zh-CN.json and en-US.json must have identical keys');
});
