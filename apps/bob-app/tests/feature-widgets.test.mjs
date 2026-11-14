import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parse, compileScript } from 'vue/compiler-sfc';
import { computed, nextTick, reactive, ref, watch } from 'vue';
import { createKernelBridge } from '../src/kernel-bridge.mjs';
import { useJarvisHome, MAX_FEATURE_STORAGE_BYTES } from '../src/composables/useJarvisHome.mjs';

const featureWidgetFrameSource = readFileSync(
  new URL('../src/components/FeatureWidgetFrame.vue', import.meta.url),
  'utf8',
);
const featureTrialBannerSource = readFileSync(
  new URL('../src/components/FeatureTrialBanner.vue', import.meta.url),
  'utf8',
);
const jarvisWidgetGridSource = readFileSync(
  new URL('../src/components/JarvisWidgetGrid.vue', import.meta.url),
  'utf8',
);
const primaryChatPaneSource = readFileSync(
  new URL('../src/components/PrimaryChatPane.vue', import.meta.url),
  'utf8',
);
const zhLocale = JSON.parse(
  readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'),
);
const enLocale = JSON.parse(
  readFileSync(new URL('../src/locales/en-US.json', import.meta.url), 'utf8'),
);

function getFeatureWidgetExports() {
  const { descriptor } = parse(featureWidgetFrameSource);
  const code = descriptor.script.content.replace(/^export /gm, '');
  const exp = new Function(
    code + '\nreturn { extractThemeVars, computeFeatureSrcdocKey, buildFeatureWidgetSrcdoc };',
  )();
  return exp;
}

const { buildFeatureWidgetSrcdoc, computeFeatureSrcdocKey, extractThemeVars } =
  getFeatureWidgetExports();

test('kernelBridge maps feature sandbox RPC bindings cleanly', async () => {
  const recorded = [];
  const bridge = createKernelBridge(
    (command, args) => {
      recorded.push({ command, args });
      return Promise.resolve({ ok: true, command, args });
    },
    () => true,
  );

  await bridge.featuresList();
  assert.equal(recorded[0].command, 'app_features_list');
  assert.deepEqual(recorded[0].args, {});

  await bridge.featureKeep('weather_widget');
  assert.equal(recorded[1].command, 'app_feature_keep');
  assert.deepEqual(recorded[1].args, { name: 'weather_widget' });

  await bridge.featureRollback('weather_widget', 2);
  assert.equal(recorded[2].command, 'app_feature_rollback');
  assert.deepEqual(recorded[2].args, { name: 'weather_widget', targetVersion: 2 });

  await bridge.featureRollback('weather_widget');
  assert.equal(recorded[3].command, 'app_feature_rollback');
  assert.deepEqual(recorded[3].args, { name: 'weather_widget', targetVersion: null });

  await bridge.featureToggle('weather_widget', false);
  assert.equal(recorded[4].command, 'app_feature_toggle');
  assert.deepEqual(recorded[4].args, { name: 'weather_widget', enabled: false });

  await bridge.featureCall('weather_widget', 'get_forecast', { city: 'Beijing' });
  assert.equal(recorded[5].command, 'app_feature_call');
  assert.deepEqual(recorded[5].args, {
    name: 'weather_widget',
    route: 'get_forecast',
    payload: { city: 'Beijing' },
  });

  await bridge.featureSaveStorage('weather_widget', { lastCity: 'Beijing' });
  assert.equal(recorded[6].command, 'app_feature_storage_save');
  assert.deepEqual(recorded[6].args, {
    name: 'weather_widget',
    storage: { lastCity: 'Beijing' },
  });
});

test('Security Iron Rule: FeatureWidgetFrame enforces sandbox="allow-scripts" and NEVER includes allow-same-origin', () => {
  // Check exact attribute presence
  assert.match(
    featureWidgetFrameSource,
    /sandbox="allow-scripts"/,
    'FeatureWidgetFrame must enforce sandbox="allow-scripts"',
  );
  assert.match(
    featureWidgetFrameSource,
    /referrerpolicy="no-referrer"/,
    'FeatureWidgetFrame must enforce referrerpolicy="no-referrer"',
  );

  // Security critical: MUST NOT contain allow-same-origin anywhere
  assert.equal(
    featureWidgetFrameSource.includes('allow-same-origin'),
    false,
    'FeatureWidgetFrame must NEVER contain allow-same-origin (null origin sandbox isolation)',
  );
});

test('Zero-flicker buildFeatureWidgetSrcdoc safely embeds initial state, storage, @scope, and escapes </script>', () => {
  const feature = {
    name: 'counter_pkg',
    title: 'Counter Demo',
    widget_html: '<div class="counter"><button id="btn">Click</button></div>',
    cached_state: { count: 42, injection: '</script><script>alert(1)</script>' },
    storage: { theme_pref: 'dark', note: 'test</script>' },
  };

  const srcdoc = buildFeatureWidgetSrcdoc(feature, {
    '--bg-primary': 'oklch(20% 0 0)',
  });

  // 1. Must contain DOCTYPE and HTML skeleton
  assert.match(srcdoc, /<!DOCTYPE html>/i);
  assert.match(srcdoc, /<div class="bob-widget-scope">/);
  assert.match(srcdoc, /@scope \(\.bob-widget-scope\)/);

  // 2. Must safely escape closing script tags inside embedded JSON
  assert.equal(
    srcdoc.includes('</script><script>alert(1)'),
    false,
    'Unescaped </script> must never exist in srcdoc script payload',
  );
  assert.match(srcdoc, /<\\\/script/);

  // 3. Must initialize window.bob bridge with pre-populated state and storage
  assert.match(srcdoc, /const bobBridge = \{/);
  assert.match(srcdoc, /state:\s*\{"count":42/);
  assert.match(srcdoc, /storage:\s*\{"theme_pref":"dark"/);

  // 4. Must define bridge methods: call, save, onState, onTheme
  assert.match(srcdoc, /call:\s*function\(route,\s*payload\)/);
  assert.match(srcdoc, /save:\s*function\(nextStorage\)/);
  assert.match(srcdoc, /onState:\s*function\(fn\)/);
  assert.match(srcdoc, /onTheme:\s*function\(fn\)/);

  // 5. Must embed raw widget HTML
  assert.match(srcdoc, /<button id="btn">Click<\/button>/);
});

test('useJarvisHome saveFeatureStorage enforces 8192-byte UTF-8 storage limit', async () => {
  let savedStorage = null;
  const mockBridge = {
    featureSaveStorage: async (name, storage) => {
      savedStorage = storage;
      return { ok: true };
    },
  };

  const home = useJarvisHome({ kernelBridge: mockBridge });
  assert.equal(MAX_FEATURE_STORAGE_BYTES, 8192);

  // 1. Payload within 8192 bytes succeeds
  const validStorage = { data: 'a'.repeat(4000) };
  await home.saveFeatureStorage('feat1', validStorage);
  assert.deepEqual(savedStorage, validStorage);

  // 2. ASCII payload exceeding 8192 bytes throws FEATURE_STORAGE_LIMIT_EXCEEDED
  const oversizedAscii = { data: 'x'.repeat(8200) };
  await assert.rejects(
    async () => await home.saveFeatureStorage('feat1', oversizedAscii),
    /FEATURE_STORAGE_LIMIT_EXCEEDED/,
  );

  // 3. Multi-byte UTF-8 payload exceeding 8192 bytes (e.g., 3000 Chinese characters = 9000 bytes)
  const oversizedUtf8 = { note: '智能小组件测试'.repeat(600) };
  await assert.rejects(
    async () => await home.saveFeatureStorage('feat1', oversizedUtf8),
    /FEATURE_STORAGE_LIMIT_EXCEEDED/,
  );
});

test('useJarvisHome feature lifecycle: refreshFeatures, trialFeatures, keepFeature, rollbackFeature, toggleFeature', async () => {
  let features = [
    {
      name: 'trial_widget',
      title: 'Trial Widget',
      version: 1,
      status: 'trial',
      trial_expires_at: Date.now() + 180000,
      has_widget: true,
      widget_html: '<p>Trial</p>',
      cached_state: { step: 1 },
      storage: {},
    },
    {
      name: 'active_widget',
      title: 'Active Widget',
      version: 2,
      status: 'active',
      has_widget: true,
      widget_html: '<p>Active</p>',
      cached_state: { step: 2 },
      storage: {},
    },
    {
      name: 'no_ui_feature',
      title: 'Headless Feature',
      version: 1,
      status: 'active',
      has_widget: false,
    },
  ];

  let kept = null;
  let rolledBack = null;
  let toggled = null;
  let called = null;

  const mockBridge = {
    featuresList: async () => features,
    featureKeep: async name => {
      kept = name;
      const f = features.find(item => item.name === name);
      if (f) {
        f.status = 'active';
        f.trial_expires_at = null;
      }
      return { ok: true };
    },
    featureRollback: async (name, targetVersion) => {
      rolledBack = { name, targetVersion };
      features = features.filter(item => item.name !== name);
      return { ok: true };
    },
    featureToggle: async (name, enabled) => {
      toggled = { name, enabled };
      const f = features.find(item => item.name === name);
      if (f) f.enabled = enabled;
      return { ok: true };
    },
    featureCall: async (name, route, payload) => {
      called = { name, route, payload };
      return { ok: true, cached_state: { step: 99 } };
    },
  };

  const home = useJarvisHome({ kernelBridge: mockBridge });

  // 1. Initial refresh
  await home.refreshFeatures();
  assert.equal(home.installedFeatures.value.length, 3);
  // customFeatureWidgets only includes active/trial with widget_html
  assert.equal(home.customFeatureWidgets.value.length, 2);
  // trialFeatures only includes status === 'trial'
  assert.equal(home.trialFeatures.value.length, 1);
  assert.equal(home.trialFeatures.value[0].name, 'trial_widget');

  // 2. callFeatureRoute updates cached_state
  const res = await home.callFeatureRoute('trial_widget', 'advance', { delta: 1 });
  assert.deepEqual(called, { name: 'trial_widget', route: 'advance', payload: { delta: 1 } });
  assert.equal(res.cached_state.step, 99);
  assert.equal(home.installedFeatures.value[0].cached_state.step, 99);

  // 3. keepFeature transitions status to 'active'
  await home.keepFeature('trial_widget');
  assert.equal(kept, 'trial_widget');
  assert.equal(home.trialFeatures.value.length, 0);
  assert.equal(home.installedFeatures.value.find(f => f.name === 'trial_widget')?.status, 'active');

  // 4. toggleFeature disables widget from customFeatureWidgets
  await home.toggleFeature('active_widget', false);
  assert.deepEqual(toggled, { name: 'active_widget', enabled: false });
  assert.equal(
    home.customFeatureWidgets.value.some(f => f.name === 'active_widget'),
    false,
  );

  // 5. rollbackFeature removes feature completely
  await home.rollbackFeature('trial_widget');
  assert.equal(rolledBack.name, 'trial_widget');
  assert.equal(
    home.installedFeatures.value.some(f => f.name === 'trial_widget'),
    false,
  );
  assert.equal(
    home.customFeatureWidgets.value.some(f => f.name === 'trial_widget'),
    false,
  );
});

test('FeatureWidgetFrame.vue and FeatureTrialBanner.vue compile cleanly with Vue SFC compiler', () => {
  for (const [name, source] of [
    ['FeatureWidgetFrame.vue', featureWidgetFrameSource],
    ['FeatureTrialBanner.vue', featureTrialBannerSource],
    ['JarvisWidgetGrid.vue', jarvisWidgetGridSource],
    ['PrimaryChatPane.vue', primaryChatPaneSource],
  ]) {
    const { descriptor } = parse(source, { filename: name });
    const compiled = compileScript(descriptor, { id: name, inlineTemplate: true });
    assert.ok(compiled.content, `${name} script setup compiles successfully`);
  }
});

test('Zero hardcoded hex/rgb colors and zero decorative emojis in all feature widget components', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;
  const hexColorRegex = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;
  const rgbColorRegex = /\brgba?\s*\(/i;

  for (const [name, source] of [
    ['FeatureWidgetFrame.vue', featureWidgetFrameSource],
    ['FeatureTrialBanner.vue', featureTrialBannerSource],
    ['JarvisWidgetGrid.vue', jarvisWidgetGridSource],
    ['PrimaryChatPane.vue', primaryChatPaneSource],
  ]) {
    assert.equal(emojiRegex.test(source), false, `${name} must not contain decorative emoji`);
    assert.equal(hexColorRegex.test(source), false, `${name} must not contain hardcoded hex color`);
    assert.equal(rgbColorRegex.test(source), false, `${name} must not contain hardcoded rgb/rgba color`);
  }
});

test('All trial & feature widget i18n keys exist symmetrically in zh-CN.json and en-US.json with zero decorative emojis', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;

  const requiredKeys = [
    'featureTrialBannerTitle',
    'featureTrialRemaining',
    'featureTrialKeep',
    'featureTrialUndo',
    'featureCustomBadge',
    'featureRollbackBtn',
    'featureTrialAutoRollback',
    'featureInstalledTitle',
  ];

  for (const key of requiredKeys) {
    assert.ok(
      typeof zhLocale[key] === 'string' && zhLocale[key].length > 0,
      `zh-CN.json missing key: ${key}`,
    );
    assert.ok(
      typeof enLocale[key] === 'string' && enLocale[key].length > 0,
      `en-US.json missing key: ${key}`,
    );

    assert.equal(
      emojiRegex.test(zhLocale[key]),
      false,
      `zh-CN.json[${key}] must not contain decorative emoji`,
    );
    assert.equal(
      emojiRegex.test(enLocale[key]),
      false,
      `en-US.json[${key}] must not contain decorative emoji`,
    );
  }
});

test('Zero-flicker FeatureWidgetFrame srcdoc stability: mutating cached_state or storage does NOT recreate srcdoc, while v1 -> v2 upgrade DOES', async () => {
  // Static check that FeatureWidgetFrame binds composedSrcdoc via ref + watch([srcdocKey, reloadNonce])
  assert.match(featureWidgetFrameSource, /const srcdocKey = computed\(/);
  assert.match(featureWidgetFrameSource, /const composedSrcdoc = ref\(''\)/);
  assert.match(featureWidgetFrameSource, /watch\(\s*\[srcdocKey,\s*reloadNonce\]/);
  assert.match(featureWidgetFrameSource, /type:\s*'bob\.state'/);
  assert.match(featureWidgetFrameSource, /type:\s*'bob\.storage'/);

  // Reactive verification matching FeatureWidgetFrame setup pipeline
  const feature = reactive({
    name: 'daily_focus_matrix',
    version: 1,
    widget_html: '<section class="matrix">v1</section>',
    cached_state: { completed: 1 },
    storage: { filter: 'urgent' },
  });
  const currentThemeVars = ref({ '--bg-primary': 'Canvas' });
  const reloadNonce = ref(0);
  const srcdocKey = computed(() => computeFeatureSrcdocKey(feature));
  const composedSrcdoc = ref('');
  let buildCount = 0;

  watch(
    [srcdocKey, reloadNonce],
    () => {
      buildCount += 1;
      composedSrcdoc.value = buildFeatureWidgetSrcdoc(feature, currentThemeVars.value);
    },
    { immediate: true },
  );

  assert.equal(buildCount, 1);
  const initialDoc = composedSrcdoc.value;
  assert.match(initialDoc, /<section class="matrix">v1<\/section>/);

  // 1. Mutating cached_state must NOT regenerate srcdoc
  feature.cached_state = { completed: 5 };
  await nextTick();
  assert.equal(buildCount, 1);
  assert.equal(composedSrcdoc.value, initialDoc);

  // 2. Mutating storage must NOT regenerate srcdoc
  feature.storage = { filter: 'scheduled', pinned: ['q3-review'] };
  await nextTick();
  assert.equal(buildCount, 1);
  assert.equal(composedSrcdoc.value, initialDoc);

  // 3. Mutating theme vars must NOT regenerate srcdoc
  currentThemeVars.value = { '--bg-primary': 'CanvasText' };
  await nextTick();
  assert.equal(buildCount, 1);
  assert.equal(composedSrcdoc.value, initialDoc);

  // 4. Upgrading version v1 -> v2 MUST regenerate srcdoc
  feature.version = 2;
  feature.widget_html = '<section class="matrix">v2-upgraded</section>';
  await nextTick();
  assert.equal(buildCount, 2);
  assert.notEqual(composedSrcdoc.value, initialDoc);
  assert.match(composedSrcdoc.value, /<section class="matrix">v2-upgraded<\/section>/);

  // 5. Explicit Retry (reloadNonce increment) regenerates srcdoc on demand
  reloadNonce.value += 1;
  await nextTick();
  assert.equal(buildCount, 3);
});

test('Iframe runtime error capture (bob.error) in buildFeatureWidgetSrcdoc & reportFeatureError auto-rollback in useJarvisHome', async () => {
  const srcdoc = buildFeatureWidgetSrcdoc({
    name: 'broken_widget',
    version: 2,
    widget_html: '<script>throw new Error("boom");</script>',
  });

  // Verify error & unhandledrejection listeners + bob.storage listener exist inside bootstrap script
  assert.match(srcdoc, /window\.addEventListener\('error'/);
  assert.match(srcdoc, /window\.addEventListener\('unhandledrejection'/);
  assert.match(srcdoc, /type:\s*'bob\.error'/);
  assert.match(srcdoc, /data\.type\s*===\s*'bob\.storage'/);

  // Verify FeatureWidgetFrame handles bob.error and renders Retry & Rollback recovery actions
  assert.match(featureWidgetFrameSource, /data\.type\s*===\s*'bob\.error'/);
  assert.match(featureWidgetFrameSource, /reportFeatureError/);
  assert.match(featureWidgetFrameSource, /handleRetryWidget/);
  assert.match(featureWidgetFrameSource, /handleRollbackWidget/);

  // Simulate v2 trial widget throwing runtime error -> backend auto-rolls back to stable v1
  let backendFeatures = [
    {
      name: 'quote_margin_calc',
      title: 'Quote Margin Calculator',
      version: 2,
      status: 'trial',
      has_widget: true,
      widget_html: '<div>v2 broken</div>',
    },
  ];
  const errorReports = [];

  const home = useJarvisHome({
    kernelBridge: {
      featuresList: async () => backendFeatures,
      featureReportError: async (name, error) => {
        errorReports.push({ name, error });
        // Backend auto-rolls back broken v2 trial to healthy v1 active
        backendFeatures = [
          {
            name: 'quote_margin_calc',
            title: 'Quote Margin Calculator',
            version: 1,
            status: 'active',
            has_widget: true,
            widget_html: '<div>v1 stable</div>',
          },
        ];
        return { ok: true, rolled_back: true, active_version: 1 };
      },
    },
  });

  await home.refreshFeatures();
  assert.equal(home.customFeatureWidgets.value[0].version, 2);

  const reportRes = await home.reportFeatureError(
    'quote_margin_calc',
    'TypeError: Cannot read properties of null',
  );
  assert.equal(reportRes.rolled_back, true);
  assert.deepEqual(errorReports, [
    { name: 'quote_margin_calc', error: 'TypeError: Cannot read properties of null' },
  ]);
  assert.equal(home.customFeatureWidgets.value.length, 1);
  assert.equal(home.customFeatureWidgets.value[0].version, 1);
  assert.equal(home.customFeatureWidgets.value[0].status, 'active');
});

test('Office widgets (daily_focus_matrix & quote_margin_calc) render cleanly with semantic CSS variables and zero hardcoded colors', () => {
  const officeWidgets = [
    {
      name: 'daily_focus_matrix',
      title: 'Daily Focus Matrix',
      version: 1,
      status: 'active',
      cached_state: {
        quadrants: {
          do_first: ['Finalize Q4 client proposal'],
          schedule: ['Architecture review'],
          delegate: ['Expense receipts'],
          eliminate: [],
        },
      },
      storage: { activeQuadrant: 'do_first' },
      widget_html: `
        <style>
          .focus-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; padding: 10px; background: var(--surface); color: var(--text-primary); }
          .focus-cell { border: 1px solid var(--line); border-radius: 6px; padding: 8px; background: var(--bg-secondary); }
          .focus-badge { color: var(--accent-primary); font-weight: 600; }
        </style>
        <div class="focus-grid">
          <div class="focus-cell"><span class="focus-badge">Do First</span><p id="q1"></p></div>
          <div class="focus-cell"><span class="focus-badge">Schedule</span><p id="q2"></p></div>
        </div>
      `,
    },
    {
      name: 'quote_margin_calc',
      title: 'Quote & Margin Calculator',
      version: 1,
      status: 'active',
      cached_state: { currency: 'USD', targetMarginPct: 32 },
      storage: { unitCost: 140, quantity: 25 },
      widget_html: `
        <style>
          .calc-card { display: flex; flex-direction: column; gap: 8px; padding: 10px; background: var(--surface); color: var(--text-primary); }
          .calc-row { display: flex; justify-content: space-between; border-bottom: 1px solid var(--line); padding-bottom: 4px; }
          .calc-profit { color: var(--color-success); font-weight: 700; }
        </style>
        <div class="calc-card">
          <div class="calc-row"><span>Unit Quote</span><strong>$205.88</strong></div>
          <div class="calc-row"><span>Gross Margin</span><span class="calc-profit">32.0%</span></div>
        </div>
      `,
    },
  ];

  const emojiRegex = /[\u{1F000}-\u{1FAFF}\u{2600}-\u{27BF}]/u;
  const hexColorRegex = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;
  const rgbColorRegex = /\brgba?\s*\(/i;

  for (const widget of officeWidgets) {
    assert.equal(emojiRegex.test(widget.widget_html), false);
    assert.equal(hexColorRegex.test(widget.widget_html), false);
    assert.equal(rgbColorRegex.test(widget.widget_html), false);

    const doc = buildFeatureWidgetSrcdoc(widget, {
      '--bg-primary': 'Canvas',
      '--surface': 'Canvas',
      '--bg-secondary': 'color-mix(in srgb, CanvasText 5%, Canvas)',
      '--line': 'color-mix(in srgb, CanvasText 12%, Canvas)',
      '--text-primary': 'CanvasText',
      '--accent-primary': 'Highlight',
      '--color-success': 'Highlight',
    });
    assert.match(doc, new RegExp(`const PKG_NAME = "${widget.name}"`));
    assert.match(doc, /var\(--surface\)/);
    assert.match(doc, /var\(--line\)/);
    assert.equal(hexColorRegex.test(doc), false);
    assert.equal(rgbColorRegex.test(doc), false);
  }
});
