import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parse, compileScript } from 'vue/compiler-sfc';
import { createKernelBridge } from '../src/kernel-bridge.mjs';
import { useJarvisHome } from '../src/composables/useJarvisHome.mjs';

const virtualBrowserModalSource = readFileSync(
  new URL('../src/components/VirtualBrowserModal.vue', import.meta.url),
  'utf8',
);
const { descriptor: vbmDescriptor } = parse(virtualBrowserModalSource, {
  filename: 'VirtualBrowserModal.vue',
});
const { mapClientPointToRemote } = await import(
  'data:text/javascript;charset=utf-8,' + encodeURIComponent(vbmDescriptor.script.content)
);
const siteSessionVaultSource = readFileSync(
  new URL('../src/components/SiteSessionVault.vue', import.meta.url),
  'utf8',
);
const primaryChatPaneSource = readFileSync(
  new URL('../src/components/PrimaryChatPane.vue', import.meta.url),
  'utf8',
);
const settingsViewSource = readFileSync(
  new URL('../src/views/SettingsView.vue', import.meta.url),
  'utf8',
);
const onboardingWizardSource = readFileSync(
  new URL('../src/components/OnboardingWizard.vue', import.meta.url),
  'utf8',
);
const manifestSource = readFileSync(
  new URL('../public/manifest.webmanifest', import.meta.url),
  'utf8',
);
const swSource = readFileSync(new URL('../public/sw.js', import.meta.url), 'utf8');
const indexHtmlSource = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const zhLocale = JSON.parse(
  readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'),
);
const enLocale = JSON.parse(
  readFileSync(new URL('../src/locales/en-US.json', import.meta.url), 'utf8'),
);

test('kernelBridge maps new browser sessions, takeover complete, proactivity, and push RPC methods', async () => {
  const recorded = [];
  const invoke = (command, args) => {
    recorded.push({ command, args });
    if (command === 'app_browser_sessions_list') {
      return Promise.resolve([{ domain: 'example.com', last_used: 1700000000000 }]);
    }
    if (command === 'app_browser_session_open') {
      return Promise.resolve({ takeover_id: 'takeover-login-1', url: args.target });
    }
    if (command === 'app_browser_session_clear') {
      return Promise.resolve({ ok: true });
    }
    if (command === 'app_browser_takeover_complete') {
      return Promise.resolve({ ok: true });
    }
    if (command === 'app_proactivity_get') {
      return Promise.resolve({ level: 0.35, quietHoursStart: '22:00', quietHoursEnd: '08:00' });
    }
    if (command === 'app_proactivity_set') {
      return Promise.resolve({ ok: true });
    }
    if (command === 'app_push_vapid_public_key') {
      return Promise.resolve({ key: 'BEl62iUY...' });
    }
    if (command === 'app_push_subscribe') {
      return Promise.resolve({ subscribed: true });
    }
    return Promise.resolve({ ok: true });
  };

  const bridge = createKernelBridge(invoke, () => true);

  // 1. Browser sessions list
  const list = await bridge.browserSessionsList();
  assert.equal(Array.isArray(list), true);
  assert.equal(list[0].domain, 'example.com');
  assert.deepEqual(recorded[0], { command: 'app_browser_sessions_list', args: {} });

  // 2. Browser session open
  const openRes = await bridge.browserSessionOpen('https://github.com/login');
  assert.equal(openRes.takeover_id, 'takeover-login-1');
  assert.deepEqual(recorded[1], {
    command: 'app_browser_session_open',
    args: { target: 'https://github.com/login' },
  });

  // 3. Browser session clear
  const clearRes = await bridge.browserSessionClear('github.com');
  assert.equal(clearRes.ok, true);
  assert.deepEqual(recorded[2], {
    command: 'app_browser_session_clear',
    args: { domain: 'github.com' },
  });

  // 4. Browser takeover complete
  const completeRes = await bridge.browserTakeoverComplete('takeover-login-1');
  assert.equal(completeRes.ok, true);
  assert.deepEqual(recorded[3], {
    command: 'app_browser_takeover_complete',
    args: { takeoverId: 'takeover-login-1' },
  });

  // 5. Proactivity get
  const proactGet = await bridge.proactivityGet();
  assert.equal(proactGet.level, 0.35);
  assert.deepEqual(recorded[4], { command: 'app_proactivity_get', args: {} });

  // 6. Proactivity set
  await bridge.proactivitySet({ level: 0.7, quietHoursStart: '22:00', quietHoursEnd: '08:00' });
  assert.deepEqual(recorded[5], {
    command: 'app_proactivity_set',
    args: { level: 0.7, quietHoursStart: '22:00', quietHoursEnd: '08:00' },
  });

  // 7. Push Vapid public key
  const vapid = await bridge.pushVapidPublicKey();
  assert.equal(vapid.key, 'BEl62iUY...');
  assert.deepEqual(recorded[6], { command: 'app_push_vapid_public_key', args: {} });

  // 8. Push subscribe
  const subRes = await bridge.pushSubscribe({ endpoint: 'https://example.com/push' });
  assert.equal(subRes.subscribed, true);
  assert.deepEqual(recorded[7], {
    command: 'app_push_subscribe',
    args: { subscription: { endpoint: 'https://example.com/push' } },
  });
});

test('mapClientPointToRemote coordinate mapping math: 1x mobile, 2x zoomed desktop, clamping, and drag sequence', () => {
  // 1. Mobile 1x portrait (390 x 844)
  const mobileRect = { left: 0, top: 0, width: 390, height: 844 };
  const centerPt = mapClientPointToRemote({
    clientX: 195,
    clientY: 422,
    rect: mobileRect,
    remoteWidth: 390,
    remoteHeight: 844,
    zoom: 1,
    panX: 0,
    panY: 0,
  });
  assert.deepEqual(centerPt, { x: 195, y: 422 });

  // Mobile with offset rect (e.g. stage centered on tablet)
  const offsetRect = { left: 50, top: 100, width: 390, height: 844 };
  const offsetPt = mapClientPointToRemote({
    clientX: 245,
    clientY: 522,
    rect: offsetRect,
    remoteWidth: 390,
    remoteHeight: 844,
    zoom: 1,
    panX: 0,
    panY: 0,
  });
  assert.deepEqual(offsetPt, { x: 195, y: 422 });

  // 2. Desktop 2x zoom (1280 x 800) rendered in 640x400 container
  const desktopRect = { left: 0, top: 0, width: 640, height: 400 };
  const zoomedPt = mapClientPointToRemote({
    clientX: 320,
    clientY: 200,
    rect: desktopRect,
    remoteWidth: 1280,
    remoteHeight: 800,
    zoom: 2,
    panX: 0,
    panY: 0,
  });
  assert.deepEqual(zoomedPt, { x: 320, y: 200 });

  // 3. Out-of-bounds clamping
  const negativePt = mapClientPointToRemote({
    clientX: -100,
    clientY: -50,
    rect: mobileRect,
    remoteWidth: 390,
    remoteHeight: 844,
    zoom: 1,
    panX: 0,
    panY: 0,
  });
  assert.deepEqual(negativePt, { x: 0, y: 0 });

  const overflowPt = mapClientPointToRemote({
    clientX: 1200,
    clientY: 2000,
    rect: mobileRect,
    remoteWidth: 390,
    remoteHeight: 844,
    zoom: 1,
    panX: 0,
    panY: 0,
  });
  assert.deepEqual(overflowPt, { x: 390, y: 844 });

  // 4. Slider drag sequence (continuous drag from x=50 to x=250)
  const dragSequenceX = [50, 100, 150, 200, 250];
  const mappedPoints = dragSequenceX.map(cx =>
    mapClientPointToRemote({
      clientX: cx,
      clientY: 300,
      rect: { left: 0, top: 0, width: 500, height: 1000 },
      remoteWidth: 500,
      remoteHeight: 1000,
      zoom: 1,
      panX: 0,
      panY: 0,
    }),
  );

  for (let i = 0; i < mappedPoints.length; i++) {
    assert.equal(mappedPoints[i].x, dragSequenceX[i]);
    assert.equal(mappedPoints[i].y, 300);
    if (i > 0) {
      assert.ok(mappedPoints[i].x > mappedPoints[i - 1].x, 'Drag coordinate increases monotonically');
    }
  }
});

test('VirtualBrowserModal privacy, IME composition guard, and zero chat history leakage', () => {
  // Check SFC script and template compilation
  const { descriptor } = parse(virtualBrowserModalSource, { filename: 'VirtualBrowserModal.vue' });
  const compiled = compileScript(descriptor, { id: 'VirtualBrowserModal.vue', inlineTemplate: true });
  assert.ok(compiled.content, 'VirtualBrowserModal.vue script setup compiles successfully');

  // Verify IME composition start/end handlers and isComposing check
  assert.match(virtualBrowserModalSource, /@compositionstart="onInputCompositionStart"/);
  assert.match(virtualBrowserModalSource, /@compositionend="onInputCompositionEnd"/);
  assert.match(virtualBrowserModalSource, /isComposing\.value/);
  assert.match(virtualBrowserModalSource, /if\s*\(\s*isComposing\.value\s*\)\s*\{?\s*return;?/);

  // Verify sensitive password masking mode
  assert.match(virtualBrowserModalSource, /inputMode(\.value)?\s*===\s*'password'/);
  assert.match(virtualBrowserModalSource, /sensitiveInputFocused/);
  assert.match(virtualBrowserModalSource, /browserZeroLlmBadge/);
  assert.match(virtualBrowserModalSource, /autocomplete="off"|:autocomplete="[^"]*off[^"]*"/);

  // Verify text is sent via WebSocket directly and cleared, never leaked to chat messages
  assert.match(virtualBrowserModalSource, /sendWs\(\s*\{\s*type:\s*'insert_text',\s*text\s*\}\s*\)/);
  assert.match(virtualBrowserModalSource, /inputBarText\.value\s*=\s*''/);
  assert.doesNotMatch(virtualBrowserModalSource, /jarvisChat/);
  assert.doesNotMatch(virtualBrowserModalSource, /shell\.messages/);
});

test('useJarvisHome activeTakeover lifecycle, siteSessions, and 4-level proactivity', async () => {
  const calls = [];
  const fakeBridge = {
    browserSessionsList: () => {
      calls.push('browserSessionsList');
      return Promise.resolve([{ domain: 'taobao.com', last_used: 1700000000000 }]);
    },
    browserSessionOpen: target => {
      calls.push(['browserSessionOpen', target]);
      return Promise.resolve({ takeover_id: 'to-site-1', url: target });
    },
    browserSessionClear: domain => {
      calls.push(['browserSessionClear', domain]);
      return Promise.resolve({ ok: true });
    },
    browserTakeoverComplete: id => {
      calls.push(['browserTakeoverComplete', id]);
      return Promise.resolve({ ok: true });
    },
    proactivityGet: () => {
      calls.push('proactivityGet');
      return Promise.resolve({ level: 0.35, quietHoursStart: '22:00', quietHoursEnd: '08:00' });
    },
    proactivitySet: args => {
      calls.push(['proactivitySet', args]);
      return Promise.resolve({ ok: true });
    },
  };

  let reloaded = false;
  const ctx = {
    kernelBridge: fakeBridge,
    reload: () => {
      reloaded = true;
      return Promise.resolve();
    },
  };

  const jarvis = useJarvisHome(ctx);

  // 1. Initial State
  assert.equal(jarvis.activeTakeover.value, null);
  assert.deepEqual(jarvis.siteSessions.value, []);
  assert.equal(jarvis.proactivityState.value.level, 0.35);

  // 2. Open takeover via card object
  jarvis.openTakeover({
    id: 'takeover-card-1',
    title: 'Verification Needed',
    summary: 'Please slide CAPTCHA',
    url: 'https://security.example.com',
  });
  assert.ok(jarvis.activeTakeover.value);
  assert.equal(jarvis.activeTakeover.value.id, 'takeover-card-1');
  assert.equal(jarvis.activeTakeover.value.title, 'Verification Needed');
  assert.equal(jarvis.activeTakeover.value.url, 'https://security.example.com');

  // 3. Complete takeover
  await jarvis.completeTakeover('takeover-card-1');
  assert.equal(jarvis.activeTakeover.value, null);
  assert.equal(reloaded, true);
  assert.ok(calls.some(c => Array.isArray(c) && c[0] === 'browserTakeoverComplete' && c[1] === 'takeover-card-1'));

  // 4. Open site for login
  await jarvis.openSiteForLogin('https://taobao.com');
  assert.ok(jarvis.activeTakeover.value);
  assert.equal(jarvis.activeTakeover.value.id, 'to-site-1');
  assert.equal(jarvis.activeTakeover.value.url, 'https://taobao.com');

  // 5. Refresh & Clear site session
  await jarvis.refreshSiteSessions();
  assert.equal(jarvis.siteSessions.value.length, 1);
  assert.equal(jarvis.siteSessions.value[0].domain, 'taobao.com');

  await jarvis.clearSiteSession('taobao.com');
  assert.ok(calls.some(c => Array.isArray(c) && c[0] === 'browserSessionClear' && c[1] === 'taobao.com'));

  // 6. Proactivity State & Tiers
  await jarvis.refreshProactivity();
  assert.equal(jarvis.proactivityState.value.level, 0.35);

  await jarvis.saveProactivity(0.7);
  assert.equal(jarvis.proactivityState.value.level, 0.7);
  assert.ok(
    calls.some(
      c =>
        Array.isArray(c) &&
        c[0] === 'proactivitySet' &&
        c[1].level === 0.7 &&
        c[1].quietHoursStart === '22:00',
    ),
  );
});

test('Static design constraints: zero hardcoded hex/rgb colors, zero decorative emoji, valid manifest, and sw.js', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;
  const hexColorRegex = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;
  const rgbColorRegex = /\brgba?\s*\(/i;

  const vueFiles = [
    ['VirtualBrowserModal.vue', virtualBrowserModalSource],
    ['SiteSessionVault.vue', siteSessionVaultSource],
    ['PrimaryChatPane.vue', primaryChatPaneSource],
    ['SettingsView.vue', settingsViewSource],
    ['OnboardingWizard.vue', onboardingWizardSource],
  ];

  for (const [name, source] of vueFiles) {
    assert.equal(emojiRegex.test(source), false, `${name} must not contain decorative emoji`);
    assert.equal(hexColorRegex.test(source), false, `${name} must not contain hardcoded hex colors`);
    assert.equal(rgbColorRegex.test(source), false, `${name} must not contain hardcoded rgb/rgba colors`);
  }

  // Valid webmanifest JSON & properties
  const manifest = JSON.parse(manifestSource);
  assert.equal(manifest.name, 'Bob');
  assert.equal(manifest.short_name, 'Bob');
  assert.equal(manifest.display, 'standalone');
  assert.ok(Array.isArray(manifest.icons) && manifest.icons.length > 0);

  // Service Worker handlers
  assert.match(swSource, /addEventListener\(\s*['"]push['"]/);
  assert.match(swSource, /addEventListener\(\s*['"]notificationclick['"]/);
  assert.match(swSource, /showNotification/);

  // index.html link to manifest and iOS PWA meta tags
  assert.match(indexHtmlSource, /<link rel="manifest" href="\/manifest\.webmanifest">/);
  assert.match(indexHtmlSource, /<meta name="apple-mobile-web-app-capable" content="yes">/);

  // Symmetrical i18n key coverage
  const requiredNewKeys = [
    'browserTakeoverTitle',
    'browserTakeoverSubtitle',
    'browserZeroLlmBadge',
    'browserDesktopMode',
    'browserMobileMode',
    'browserZoom',
    'browserSaveScreenshot',
    'browserBack',
    'browserReload',
    'browserHandBackToBob',
    'browserCancelTakeover',
    'browserInputPlaceholder',
    'browserPasswordPlaceholder',
    'browserSendText',
    'browserTextMode',
    'browserMaskedMode',
    'browserKeyEnter',
    'browserKeyTab',
    'browserKeyBackspace',
    'browserConnecting',
    'browserSessionVaultTitle',
    'browserSessionVaultDesc',
    'browserPreLoginSiteBtn',
    'browserPreLoginPlaceholder',
    'browserClearSession',
    'browserClearAllSessions',
    'browserClearAllConfirm',
    'browserNoSavedSessions',
    'browserLastUsed',
    'browserCookiesStored',
    'proactivitySettingsTitle',
    'proactivitySettingsDesc',
    'proactivitySilent',
    'proactivitySilentDesc',
    'proactivityMorningBrief',
    'proactivityMorningBriefDesc',
    'proactivityTimely',
    'proactivityTimelyDesc',
    'proactivityCare',
    'proactivityCareDesc',
    'proactivityQuietHoursNote',
    'spendCapTitle',
    'spendCapDesc',
    'spendCapMonthly',
    'spendCapCurrentSpend',
    'spendCapSaveBtn',
    'spendCapSaved',
  ];

  for (const key of requiredNewKeys) {
    assert.ok(typeof zhLocale[key] === 'string' && zhLocale[key].length > 0, `zh-CN.json missing key: ${key}`);
    assert.ok(typeof enLocale[key] === 'string' && enLocale[key].length > 0, `en-US.json missing key: ${key}`);
    assert.equal(emojiRegex.test(zhLocale[key]), false, `zh-CN.json[${key}] must not contain decorative emoji`);
    assert.equal(emojiRegex.test(enLocale[key]), false, `en-US.json[${key}] must not contain decorative emoji`);
  }
});
