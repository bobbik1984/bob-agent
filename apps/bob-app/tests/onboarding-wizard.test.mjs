import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parse, compileScript } from 'vue/compiler-sfc';
import {
  BOB_NODE_TOKEN_STORAGE_KEY,
  fetchOnboardingStatus,
  claimOnboardingNode,
  configureOnboardingProvider,
  completeOnboardingPersona,
  getBobNodeSessionToken,
  setBobNodeSessionToken,
} from '../src/http-bridge.mjs';

const onboardingWizardSource = readFileSync(
  new URL('../src/components/OnboardingWizard.vue', import.meta.url),
  'utf8',
);
const appVueSource = readFileSync(
  new URL('../src/App.vue', import.meta.url),
  'utf8',
);
const httpBridgeSource = readFileSync(
  new URL('../src/http-bridge.mjs', import.meta.url),
  'utf8',
);
const zhLocale = JSON.parse(
  readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'),
);
const enLocale = JSON.parse(
  readFileSync(new URL('../src/locales/en-US.json', import.meta.url), 'utf8'),
);

const { descriptor: wizardDescriptor } = parse(onboardingWizardSource, {
  filename: 'OnboardingWizard.vue',
});
const {
  DEFAULT_PROVIDER_PRESETS,
  normalizeProviderPresets,
  normalizePairingCode,
  validateStep1Claim,
  resolveOnboardingClaimErrorKey,
  resolveAutoPairedModels,
} = await import(
  'data:text/javascript;charset=utf-8,' + encodeURIComponent(wizardDescriptor.script.content)
);

test('OnboardingWizard.vue and App.vue compile cleanly with Vue SFC compiler', () => {
  for (const [name, source] of [
    ['OnboardingWizard.vue', onboardingWizardSource],
    ['App.vue', appVueSource],
  ]) {
    const { descriptor } = parse(source, { filename: name });
    const compiled = compileScript(descriptor, { id: name, inlineTemplate: true });
    assert.ok(compiled.content, `${name} script setup compiles cleanly`);
  }
});

test('Step 1 helpers normalize 6-digit pairing codes, validate passwords, and map anti-front-running errors', () => {
  assert.equal(normalizePairingCode(' 842-917 '), '842917');
  assert.equal(normalizePairingCode('12 34 56 78'), '123456');

  assert.deepEqual(
    validateStep1Claim({
      pairingCode: '123',
      password: 'ValidAdminPassword123!',
      confirmPassword: 'ValidAdminPassword123!',
    }),
    { valid: false, errorKey: 'onboardingErrPairingFormat' },
  );

  assert.deepEqual(
    validateStep1Claim({
      pairingCode: '842917',
      password: 'short',
      confirmPassword: 'short',
    }),
    { valid: false, errorKey: 'onboardingErrPasswordShort' },
  );

  assert.deepEqual(
    validateStep1Claim({
      pairingCode: '842917',
      password: 'ValidAdminPassword123!',
      confirmPassword: 'DifferentPassword123!',
    }),
    { valid: false, errorKey: 'onboardingErrPasswordMismatch' },
  );

  assert.deepEqual(
    validateStep1Claim({
      pairingCode: '842-917',
      password: 'ValidAdminPassword123!',
      confirmPassword: 'ValidAdminPassword123!',
    }),
    { valid: true, errorKey: null },
  );

  assert.equal(
    resolveOnboardingClaimErrorKey({ code: 'PAIRING_CODE_INVALID' }),
    'onboardingErrPairingInvalid',
  );
  assert.equal(
    resolveOnboardingClaimErrorKey({ code: 'PAIRING_RATE_LIMITED' }),
    'onboardingErrRateLimited',
  );
  assert.equal(
    resolveOnboardingClaimErrorKey({ code: 'ONBOARDING_ALREADY_CLAIMED' }),
    'onboardingErrAlreadyClaimed',
  );
  assert.equal(
    resolveOnboardingClaimErrorKey({ code: 'PAIRING_CODE_EXPIRED' }),
    'onboardingErrPairingExpired',
  );
});

test('Step 2 provider presets auto-pair Clerk (fast reply) and Maker (deep worker) models aligned with model_providers.json', () => {
  const registry = JSON.parse(
    readFileSync(
      new URL('../src-tauri/resources/model_providers.json', import.meta.url),
      'utf8',
    ),
  );
  const providersById = new Map(registry.providers.map(p => [p.id, p]));

  // Verify every fallback preset matches model_providers.json (zero stale hardcoded model IDs)
  for (const preset of DEFAULT_PROVIDER_PRESETS) {
    const prov = providersById.get(preset.id);
    assert.ok(prov, `Provider ${preset.id} must exist in model_providers.json`);
    const modelIds = new Set((prov.models || []).map(m => m.id));
    assert.ok(
      modelIds.has(preset.clerkModel),
      `Clerk model ${preset.clerkModel} for ${preset.id} must exist in model_providers.json`,
    );
    assert.ok(
      modelIds.has(preset.makerModel),
      `Maker model ${preset.makerModel} for ${preset.id} must exist in model_providers.json`,
    );
  }

  const merged = normalizeProviderPresets([
    {
      provider_id: 'deepseek',
      display_name: 'DeepSeek Official',
      clerk_model: 'deepseek-v4.1-flash-1017',
      maker_model: 'deepseek-v4-pro',
      recommended: true,
    },
  ]);

  assert.ok(merged.length >= DEFAULT_PROVIDER_PRESETS.length);
  const featured = merged.filter(p => p.featured).map(p => p.id);
  assert.deepEqual(featured, ['deepseek', 'gemini', 'openai', 'anthropic', 'openrouter']);

  const dsPair = resolveAutoPairedModels(merged, 'deepseek');
  assert.equal(dsPair.clerkModel, 'deepseek-v4.1-flash-1017');
  assert.equal(dsPair.makerModel, 'deepseek-v4-pro');

  const geminiPair = resolveAutoPairedModels(merged, 'gemini');
  assert.equal(geminiPair.clerkModel, 'models/gemini-2.5-flash');
  assert.equal(geminiPair.makerModel, 'models/gemini-2.5-pro');

  const qwenPair = resolveAutoPairedModels(merged, 'qwen');
  assert.equal(qwenPair.clerkModel, 'qwen3.8-flash');
  assert.equal(qwenPair.makerModel, 'qwen3.8-max');
});

test('3-Step Onboarding HTTP flow: status -> claim (token + rate-limit/front-run guards) -> provider -> persona', async () => {
  const origWindow = globalThis.window;
  const origFetch = globalThis.fetch;
  const origLocalStorage = globalThis.localStorage;
  const store = new Map();

  globalThis.localStorage = {
    getItem: k => (store.has(k) ? store.get(k) : null),
    setItem: (k, v) => store.set(k, String(v)),
    removeItem: k => store.delete(k),
  };

  try {
    // 0. Non-node runtime falls back cleanly without throwing
    globalThis.window = undefined;
    const nonNodeStatus = await fetchOnboardingStatus();
    assert.deepEqual(nonNodeStatus, { needsOnboarding: false, presets: [] });

    // Enable bob-node runtime and ensure window.prompt is NEVER invoked
    let promptCalled = false;
    globalThis.window = {
      __BOB_NODE_RUNTIME__: true,
      prompt: () => {
        promptCalled = true;
        return null;
      },
    };
    setBobNodeSessionToken(null);

    // 1. 404 status gracefully falls back
    globalThis.fetch = async () => ({ ok: false, status: 404, json: async () => null });
    const missingEndpointStatus = await fetchOnboardingStatus();
    assert.deepEqual(missingEndpointStatus, { needsOnboarding: false, presets: [] });

    // 2. Active onboarding status
    globalThis.fetch = async url => {
      assert.equal(url, '/api/node/v1/onboarding/status');
      return {
        ok: true,
        status: 200,
        json: async () => ({
          needsOnboarding: true,
          passwordConfigured: false,
          providerConfigured: false,
          pairingCodeRequired: true,
          pairingExpiresAt: 1790000000000,
          lockedRetryAfterSec: 0,
          presets: [{ id: 'deepseek', clerkModel: 'deepseek-chat', makerModel: 'deepseek-reasoner' }],
        }),
      };
    };
    const status = await fetchOnboardingStatus();
    assert.equal(status.needsOnboarding, true);
    assert.equal(status.pairingCodeRequired, true);
    assert.equal(status.presets.length, 1);
    assert.equal(globalThis.window.__BOB_ONBOARDING_ACTIVE__, true);
    assert.equal(promptCalled, false);

    // 3. Step 1 failure cases: invalid pairing code, rate limit cooldown, already claimed
    globalThis.fetch = async () => ({
      ok: false,
      status: 403,
      json: async () => ({ ok: false, error: 'PAIRING_CODE_INVALID', message: 'Wrong 6-digit code' }),
    });
    await assert.rejects(
      () => claimOnboardingNode({ pairingCode: '000000', password: 'StrongPassword123!' }),
      err => {
        assert.equal(err.code, 'PAIRING_CODE_INVALID');
        assert.equal(err.status, 403);
        return true;
      },
    );

    globalThis.fetch = async () => ({
      ok: false,
      status: 429,
      json: async () => ({
        ok: false,
        error: 'PAIRING_RATE_LIMITED',
        retryAfterSec: 45,
      }),
    });
    await assert.rejects(
      () => claimOnboardingNode({ pairingCode: '111111', password: 'StrongPassword123!' }),
      err => {
        assert.equal(err.code, 'PAIRING_RATE_LIMITED');
        assert.equal(err.retryAfterSec, 45);
        return true;
      },
    );

    globalThis.fetch = async () => ({
      ok: false,
      status: 403,
      json: async () => ({ ok: false, error: 'ONBOARDING_ALREADY_CLAIMED' }),
    });
    await assert.rejects(
      () => claimOnboardingNode({ pairingCode: '222222', password: 'StrongPassword123!' }),
      err => {
        assert.equal(err.code, 'ONBOARDING_ALREADY_CLAIMED');
        return true;
      },
    );

    // 4. Step 1 success: stores session token in bob_node_session_token
    globalThis.fetch = async (url, opts) => {
      assert.equal(url, '/api/node/v1/onboarding/claim');
      const req = JSON.parse(opts.body);
      assert.equal(req.pairingCode, '842917');
      assert.equal(req.password, 'StrongPassword123!');
      return {
        ok: true,
        status: 200,
        json: async () => ({
          ok: true,
          accessToken: 'session-tok-claimed-99',
          expiresAt: 1790086400000,
        }),
      };
    };
    const claimRes = await claimOnboardingNode({
      pairingCode: ' 842917 ',
      password: 'StrongPassword123!',
      clientName: 'Test Browser',
    });
    assert.equal(claimRes.ok, true);
    assert.equal(store.get(BOB_NODE_TOKEN_STORAGE_KEY), 'session-tok-claimed-99');
    assert.equal(getBobNodeSessionToken(), 'session-tok-claimed-99');

    // 5. Step 2 provider configuration sends Bearer token and supports skipConnectivityTest fallback
    let capturedProviderHeaders = null;
    let capturedProviderBody = null;
    globalThis.fetch = async (url, opts) => {
      assert.equal(url, '/api/node/v1/onboarding/provider');
      capturedProviderHeaders = opts.headers;
      capturedProviderBody = JSON.parse(opts.body);
      return {
        ok: true,
        status: 200,
        json: async () => ({ ok: true, connectivityVerified: true }),
      };
    };
    const provRes = await configureOnboardingProvider({
      providerId: 'deepseek',
      apiKey: 'sk-test-deepseek-key',
      baseUrl: 'https://api.deepseek.com/v1',
      clerkModel: 'deepseek-chat',
      makerModel: 'deepseek-reasoner',
      skipConnectivityTest: false,
    });
    assert.equal(provRes.ok, true);
    assert.equal(capturedProviderHeaders.Authorization, 'Bearer session-tok-claimed-99');
    assert.deepEqual(capturedProviderBody, {
      providerId: 'deepseek',
      apiKey: 'sk-test-deepseek-key',
      baseUrl: 'https://api.deepseek.com/v1',
      clerkModel: 'deepseek-chat',
      makerModel: 'deepseek-reasoner',
      skipConnectivityTest: false,
    });

    // 6. Step 3 persona save & skip
    const personaCalls = [];
    globalThis.fetch = async (url, opts) => {
      assert.equal(url, '/api/node/v1/onboarding/persona');
      assert.equal(opts.headers.Authorization, 'Bearer session-tok-claimed-99');
      personaCalls.push(JSON.parse(opts.body));
      return {
        ok: true,
        status: 200,
        json: async () => ({ ok: true }),
      };
    };
    await completeOnboardingPersona({ assistantName: 'Jarvis' });
    await completeOnboardingPersona({ assistantName: '' });
    assert.deepEqual(personaCalls, [{ assistantName: 'Jarvis' }, { assistantName: '' }]);
    assert.equal(globalThis.window.__BOB_ONBOARDING_ACTIVE__, false);
    assert.equal(promptCalled, false);
  } finally {
    globalThis.window = origWindow;
    globalThis.fetch = origFetch;
    globalThis.localStorage = origLocalStorage;
  }
});

test('Static audit on OnboardingWizard.vue, App.vue, http-bridge.mjs, and 100% symmetric zh-CN/en-US locales', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}\u{2600}-\u{27BF}]/u;
  const hexColorRegex = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;
  const rgbColorRegex = /\brgba?\s*\(/i;
  const hslColorRegex = /\bhsla?\s*\(/i;
  const oklchColorRegex = /\boklch\s*\(/i;

  for (const [name, source] of [
    ['OnboardingWizard.vue', onboardingWizardSource],
    ['App.vue', appVueSource],
  ]) {
    assert.equal(emojiRegex.test(source), false, `${name} must contain zero decorative emoji`);
    assert.equal(hexColorRegex.test(source), false, `${name} must contain zero #hex colors`);
    assert.equal(rgbColorRegex.test(source), false, `${name} must contain zero rgb/rgba colors`);
    assert.equal(hslColorRegex.test(source), false, `${name} must contain zero hsl/hsla colors`);
    assert.equal(oklchColorRegex.test(source), false, `${name} must contain zero oklch colors`);
  }

  // Ensure http-bridge.mjs does not invoke window.prompt
  assert.doesNotMatch(httpBridgeSource, /window\.prompt\s*\(/);

  // Responsive 390px mobile and 1280px desktop breakpoints in OnboardingWizard.vue
  assert.match(onboardingWizardSource, /@media\s*\(\s*max-width:\s*390px\s*\)/);
  assert.match(onboardingWizardSource, /@media\s*\(\s*min-width:\s*1280px\s*\)/);
  assert.match(onboardingWizardSource, /position:\s*sticky/);

  // 100% symmetric keys across zh-CN.json and en-US.json
  const zhKeys = Object.keys(zhLocale).sort();
  const enKeys = Object.keys(enLocale).sort();
  assert.deepEqual(zhKeys, enKeys, 'zh-CN.json and en-US.json must have 100% identical key sets');

  const onboardingKeys = zhKeys.filter(k => k.startsWith('onboarding'));
  assert.ok(onboardingKeys.length >= 30, 'All onboarding i18n keys are registered');
  for (const key of onboardingKeys) {
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
