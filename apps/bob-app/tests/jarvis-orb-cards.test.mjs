import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parse, compileScript } from 'vue/compiler-sfc';

const jarvisOrbSource = readFileSync(new URL('../src/components/JarvisOrb.vue', import.meta.url), 'utf8');
const actionCardSource = readFileSync(new URL('../src/components/ActionCard.vue', import.meta.url), 'utf8');
const styleCssSource = readFileSync(new URL('../src/style.css', import.meta.url), 'utf8');
const zhLocale = JSON.parse(readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'));
const enLocale = JSON.parse(readFileSync(new URL('../src/locales/en-US.json', import.meta.url), 'utf8'));

test('JarvisOrb.vue and ActionCard.vue contain zero hardcoded hex/rgb colors and zero decorative emojis', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;
  const hexColorRegex = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;
  const rgbColorRegex = /\brgba?\s*\(/i;

  for (const [name, source] of [
    ['JarvisOrb.vue', jarvisOrbSource],
    ['ActionCard.vue', actionCardSource],
  ]) {
    assert.equal(emojiRegex.test(source), false, `${name} contains decorative emoji`);
    assert.equal(hexColorRegex.test(source), false, `${name} contains hardcoded hex color`);
    assert.equal(rgbColorRegex.test(source), false, `${name} contains hardcoded rgb/rgba color`);
  }
});

test('JarvisOrb.vue implements all 4 moods, hero/compact size classes, aria-label, and prefers-reduced-motion', () => {
  // SFC script and template compilation
  const { descriptor } = parse(jarvisOrbSource, { filename: 'JarvisOrb.vue' });
  const compiled = compileScript(descriptor, { id: 'JarvisOrb.vue', inlineTemplate: true });
  assert.ok(compiled.content, 'JarvisOrb.vue script setup compiles successfully');

  // Verify mood props and classes
  for (const mood of ['idle', 'thinking', 'speaking', 'needs_human']) {
    assert.match(jarvisOrbSource, new RegExp(mood), `JarvisOrb.vue should reference mood "${mood}"`);
  }
  assert.match(jarvisOrbSource, /mood-idle/);
  assert.match(jarvisOrbSource, /mood-thinking/);
  assert.match(jarvisOrbSource, /mood-speaking/);
  assert.match(jarvisOrbSource, /mood-needs[-_]human/);

  // Verify size classes
  assert.match(jarvisOrbSource, /orb-hero|size-hero/);
  assert.match(jarvisOrbSource, /orb-compact|size-compact/);

  // Verify aria-label bindings
  assert.match(jarvisOrbSource, /:aria-label="ariaLabel"/);
  assert.match(jarvisOrbSource, /jarvisOrbIdle/);
  assert.match(jarvisOrbSource, /jarvisOrbThinking/);
  assert.match(jarvisOrbSource, /jarvisOrbSpeaking/);
  assert.match(jarvisOrbSource, /jarvisOrbNeedsHuman/);

  // Verify prefers-reduced-motion fallback
  assert.match(jarvisOrbSource, /@media\s*\(\s*prefers-reduced-motion:\s*reduce\s*\)/);
  assert.match(styleCssSource, /@media\s*\(\s*prefers-reduced-motion:\s*reduce\s*\)/);
});

test('JarvisOrb semantic CSS tokens exist in style.css and utilize oklch and light-dark', () => {
  const requiredTokens = [
    '--orb-core-cyan',
    '--orb-core-violet',
    '--orb-core-magenta',
    '--orb-core-blue',
    '--orb-amber-core',
    '--orb-amber-glow',
    '--orb-glow-soft',
    '--orb-surface-ring',
  ];

  for (const token of requiredTokens) {
    assert.match(styleCssSource, new RegExp(`${token}:\\s*light-dark\\(`), `style.css should define ${token} with light-dark`);
    assert.match(styleCssSource, new RegExp(`${token}:[^;]*oklch\\(`), `${token} should use oklch in style.css`);
  }
});

test('ActionCard.vue supports info, approval, takeover, and routine_draft kinds, progressive disclosure, and action emissions', () => {
  // SFC script and template compilation
  const { descriptor } = parse(actionCardSource, { filename: 'ActionCard.vue' });
  const compiled = compileScript(descriptor, { id: 'ActionCard.vue', inlineTemplate: true });
  assert.ok(compiled.content, 'ActionCard.vue script setup compiles successfully');

  // Verify kinds supported
  for (const kind of ['info', 'approval', 'takeover', 'routine_draft']) {
    assert.match(actionCardSource, new RegExp(kind), `ActionCard.vue should support kind "${kind}"`);
  }

  // Verify action emissions
  assert.match(actionCardSource, /defineEmits\(\s*\[[^\]]*'approve'[^\]]*\]\)/);
  assert.match(actionCardSource, /defineEmits\(\s*\[[^\]]*'reject'[^\]]*\]\)/);
  assert.match(actionCardSource, /defineEmits\(\s*\[[^\]]*'open-takeover'[^\]]*\]\)/);
  assert.match(actionCardSource, /defineEmits\(\s*\[[^\]]*'save-routine'[^\]]*\]\)/);

  // Verify progressive disclosure for technicalDetail
  assert.match(actionCardSource, /<details[^>]*card\.technicalDetail/);
  assert.match(actionCardSource, /<summary/);
  assert.match(actionCardSource, /cardTechnicalDetails/);

  // Verify risk badge levels
  for (const risk of ['side_effect', 'irreversible', 'settings']) {
    assert.match(actionCardSource, new RegExp(risk), `ActionCard.vue should support risk level "${risk}"`);
  }

  // Verify clean SVG icon map (zero emojis)
  assert.match(actionCardSource, /resolvedIcon === 'calendar'/);
  assert.match(actionCardSource, /resolvedIcon === 'weather'/);
  assert.match(actionCardSource, /resolvedIcon === 'browser'/);
  assert.match(actionCardSource, /resolvedIcon === 'shield'/);
  assert.match(actionCardSource, /resolvedIcon === 'file'/);
  assert.match(actionCardSource, /resolvedIcon === 'terminal'/);
  assert.match(actionCardSource, /resolvedIcon === 'routine'/);
});

test('All required i18n keys exist symmetrically in zh-CN.json and en-US.json with zero emoji characters', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;

  const requiredKeys = [
    'jarvisOrbIdle',
    'jarvisOrbThinking',
    'jarvisOrbSpeaking',
    'jarvisOrbNeedsHuman',
    'cardAllowOnce',
    'cardReject',
    'cardOpenTakeover',
    'cardSaveAsRoutine',
    'cardApproved',
    'cardRejected',
    'cardExpired',
    'cardTechnicalDetails',
    'cardRiskSideEffect',
    'cardRiskIrreversible',
    'cardRiskSettings',
  ];

  for (const key of requiredKeys) {
    assert.ok(typeof zhLocale[key] === 'string' && zhLocale[key].length > 0, `zh-CN.json missing key: ${key}`);
    assert.ok(typeof enLocale[key] === 'string' && enLocale[key].length > 0, `en-US.json missing key: ${key}`);

    assert.equal(emojiRegex.test(zhLocale[key]), false, `zh-CN.json[${key}] must not contain decorative emoji`);
    assert.equal(emojiRegex.test(enLocale[key]), false, `en-US.json[${key}] must not contain decorative emoji`);
  }
});
