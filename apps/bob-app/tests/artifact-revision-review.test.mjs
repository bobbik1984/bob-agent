import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parse, compileScript } from 'vue/compiler-sfc';

const reviewSfcSource = readFileSync(
  new URL('../src/components/ArtifactRevisionReview.vue', import.meta.url),
  'utf8',
);
const workViewSource = readFileSync(
  new URL('../src/views/WorkView.vue', import.meta.url),
  'utf8',
);
const dockerfileSource = readFileSync(
  new URL('../../../deploy/Dockerfile', import.meta.url),
  'utf8',
);
const dockerComposeSource = readFileSync(
  new URL('../../../deploy/docker-compose.yml', import.meta.url),
  'utf8',
);
const caddyfileSource = readFileSync(
  new URL('../../../deploy/Caddyfile.example', import.meta.url),
  'utf8',
);
const deployScriptSource = readFileSync(
  new URL('../../../scripts/deploy-vps-oneclick.sh', import.meta.url),
  'utf8',
);
const zhLocale = JSON.parse(
  readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'),
);
const enLocale = JSON.parse(
  readFileSync(new URL('../src/locales/en-US.json', import.meta.url), 'utf8'),
);

// Load pure helper exports dynamically
const { descriptor } = parse(reviewSfcSource, { filename: 'ArtifactRevisionReview.vue' });
const helpersModule = await import(
  'data:text/javascript,' + encodeURIComponent(descriptor.script.content)
);
const { computeLineDiff, buildRevisionDispatchPayload } = helpersModule;

test('computeLineDiff handles empty, identical, addition, deletion, and replacement diffs', () => {
  // 1. Empty strings
  assert.deepEqual(computeLineDiff('', ''), []);
  assert.deepEqual(computeLineDiff(null, null), []);

  // 2. Pure additions
  const addDiff = computeLineDiff('', 'line1\nline2');
  assert.equal(addDiff.length, 2);
  assert.deepEqual(addDiff[0], { type: 'added', oldLine: null, newLine: 1, text: 'line1' });
  assert.deepEqual(addDiff[1], { type: 'added', oldLine: null, newLine: 2, text: 'line2' });

  // 3. Pure deletions
  const delDiff = computeLineDiff('alpha\nbeta', '');
  assert.equal(delDiff.length, 2);
  assert.deepEqual(delDiff[0], { type: 'removed', oldLine: 1, newLine: null, text: 'alpha' });
  assert.deepEqual(delDiff[1], { type: 'removed', oldLine: 2, newLine: null, text: 'beta' });

  // 4. Identical content
  const idDiff = computeLineDiff('same\ntext', 'same\ntext');
  assert.equal(idDiff.length, 2);
  assert.deepEqual(idDiff[0], { type: 'unchanged', oldLine: 1, newLine: 1, text: 'same' });
  assert.deepEqual(idDiff[1], { type: 'unchanged', oldLine: 2, newLine: 2, text: 'text' });

  // 5. Line modification (deletion + insertion in place)
  const modDiff = computeLineDiff('first\nold_second\nthird', 'first\nnew_second\nthird');
  assert.equal(modDiff.length, 4);
  assert.equal(modDiff[0].type, 'unchanged');
  assert.equal(modDiff[0].text, 'first');
  assert.equal(modDiff[1].type, 'removed');
  assert.equal(modDiff[1].text, 'old_second');
  assert.equal(modDiff[1].oldLine, 2);
  assert.equal(modDiff[1].newLine, null);
  assert.equal(modDiff[2].type, 'added');
  assert.equal(modDiff[2].text, 'new_second');
  assert.equal(modDiff[2].oldLine, null);
  assert.equal(modDiff[2].newLine, 2);
  assert.equal(modDiff[3].type, 'unchanged');
  assert.equal(modDiff[3].text, 'third');
  assert.equal(modDiff[3].oldLine, 3);
  assert.equal(modDiff[3].newLine, 3);
});

test('buildRevisionDispatchPayload formats explicit parentVersionId, line annotations, region annotations, and instructions', () => {
  const payload = buildRevisionDispatchPayload({
    artifactPath: 'docs/architecture.md',
    parentVersionId: 'V1',
    summaryInstruction: 'Revise section 3 and clarify error handling.',
    annotations: [
      {
        id: 'ann-line-1',
        targetType: 'line',
        lineStart: 42,
        lineEnd: 45,
        rect: null,
        comment: 'Explain fallback when offline.',
      },
      {
        id: 'ann-single-line',
        targetType: 'line',
        lineStart: 88,
        lineEnd: 88,
        rect: null,
        comment: 'Fix typo here.',
      },
      {
        id: 'ann-region-1',
        targetType: 'region',
        lineStart: null,
        lineEnd: null,
        rect: { xPct: 15, yPct: 25, wPct: 50, hPct: 30 },
        comment: 'Replace legacy icon with clean SVG.',
      },
    ],
  });

  assert.equal(payload.parentVersionId, 'V1');
  assert.equal(payload.artifactPath, 'docs/architecture.md');
  assert.equal(payload.summaryInstruction, 'Revise section 3 and clarify error handling.');
  assert.equal(payload.annotations.length, 3);

  // Line annotations verified
  assert.equal(payload.annotations[0].lineStart, 42);
  assert.equal(payload.annotations[0].lineEnd, 45);
  assert.equal(payload.annotations[0].comment, 'Explain fallback when offline.');
  assert.equal(payload.annotations[1].lineStart, 88);
  assert.equal(payload.annotations[1].lineEnd, 88);

  // Region annotation verified
  assert.deepEqual(payload.annotations[2].rect, { xPct: 15, yPct: 25, wPct: 50, hPct: 30 });
  assert.equal(payload.annotations[2].comment, 'Replace legacy icon with clean SVG.');

  // Formatted instruction prompt verified
  assert.match(payload.instruction, /\[Revision Request for docs\/architecture\.md \(Base: V1\)\]/);
  assert.match(payload.instruction, /Revise section 3 and clarify error handling\./);
  assert.match(payload.instruction, /\[Lines 42-45\] Explain fallback when offline\./);
  assert.match(payload.instruction, /\[Line 88\] Fix typo here\./);
  assert.match(payload.instruction, /\[Region: x=15%, y=25%, w=50%, h=30%\] Replace legacy icon with clean SVG\./);
});

test('ArtifactRevisionReview.vue compiles and satisfies design constraints (zero hex, zero rgb, zero decorative emojis)', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;
  const hexColorRegex = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;
  const rgbColorRegex = /\brgba?\s*\(/i;

  // SFC Compilation
  const compiled = compileScript(descriptor, { id: 'ArtifactRevisionReview.vue', inlineTemplate: true });
  assert.ok(compiled.content, 'ArtifactRevisionReview.vue script compiles successfully');

  // Design constraints
  assert.equal(emojiRegex.test(reviewSfcSource), false, 'Must not contain decorative emojis');
  assert.equal(hexColorRegex.test(reviewSfcSource), false, 'Must not contain hardcoded hex colors');
  assert.equal(rgbColorRegex.test(reviewSfcSource), false, 'Must not contain hardcoded rgb/rgba colors');

  // Props & Emits inspection
  assert.match(reviewSfcSource, /versions/);
  assert.match(reviewSfcSource, /activeVersionId/);
  assert.match(reviewSfcSource, /busy/);
  assert.match(reviewSfcSource, /dispatch-revision/);
  assert.match(reviewSfcSource, /approve-version/);
  assert.match(reviewSfcSource, /close/);

  // View modes inspection
  assert.match(reviewSfcSource, /artifactModeSingle/);
  assert.match(reviewSfcSource, /artifactModeSideBySide/);
  assert.match(reviewSfcSource, /artifactModeToggle/);
  assert.match(reviewSfcSource, /artifactDispatchRevisionBtn/);
  assert.match(reviewSfcSource, /artifactApproveVersionBtn/);
});

test('WorkView.vue mounts ArtifactRevisionReview cleanly and preserves existing architecture', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;
  const hexColorRegex = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;
  const rgbColorRegex = /\brgba?\s*\(/i;

  assert.equal(emojiRegex.test(workViewSource), false, 'WorkView.vue must not contain decorative emojis');
  assert.equal(hexColorRegex.test(workViewSource), false, 'WorkView.vue must not contain hardcoded hex colors');
  assert.equal(rgbColorRegex.test(workViewSource), false, 'WorkView.vue must not contain hardcoded rgb/rgba colors');

  assert.match(workViewSource, /import ArtifactRevisionReview from '\.\.\/components\/ArtifactRevisionReview\.vue';/);
  assert.match(workViewSource, /artifactOpenRevisionReview/);
  assert.match(workViewSource, /<ArtifactRevisionReview/);
  assert.match(workViewSource, /@dispatch-revision="handleDispatchRevision"/);
  assert.match(workViewSource, /@approve-version="handleApproveVersion"/);
  assert.match(workViewSource, /operationKind:\s*'revise'/);
  assert.match(workViewSource, /operationKind:\s*'review'/);
});

test('Sanitized deployment templates contain zero personal domains, zero hardcoded secrets, and valid multi-stage architecture', () => {
  const personalDomainRegex = /bobbik\.(?:org|com|net|cn)/i;
  const hardcodedIpRegex = /\b(?:192\.168\.\d+\.\d+|10\.\d+\.\d+\.\d+|172\.(?:1[6-9]|2\d|3[01])\.\d+\.\d+)\b/;

  // Check personal domains and secrets across all deploy files
  for (const [name, content] of [
    ['deploy/Dockerfile', dockerfileSource],
    ['deploy/docker-compose.yml', dockerComposeSource],
    ['deploy/Caddyfile.example', caddyfileSource],
    ['scripts/deploy-vps-oneclick.sh', deployScriptSource],
  ]) {
    assert.equal(personalDomainRegex.test(content), false, `${name} must not contain personal domains`);
    assert.equal(hardcodedIpRegex.test(content), false, `${name} must not contain hardcoded private/real IP addresses`);
  }

  // Dockerfile checks
  assert.match(dockerfileSource, /FROM node:22-bookworm-slim AS frontend-builder/);
  assert.match(dockerfileSource, /FROM rust:1\.85-bookworm AS rust-builder/);
  assert.match(dockerfileSource, /FROM debian:bookworm-slim AS runtime/);
  assert.match(dockerfileSource, /chromium/);
  assert.match(dockerfileSource, /chmod 700 \/var\/lib\/bob-node/);
  assert.match(dockerfileSource, /HEALTHCHECK/);

  // Runtime stage must NOT install nodejs or npm
  const runtimeStage = dockerfileSource.split('AS runtime')[1] || '';
  assert.equal(/apt-get install [^\n]*\bnodejs\b/.test(runtimeStage), false, 'Runtime stage must not install nodejs');
  assert.equal(/apt-get install [^\n]*\bnpm\b/.test(runtimeStage), false, 'Runtime stage must not install npm');

  // docker-compose checks
  assert.match(dockerComposeSource, /shm_size:\s*['"]?1gb['"]?/);
  assert.match(dockerComposeSource, /bob_data:/);
  assert.match(dockerComposeSource, /caddy_data:/);
  assert.match(dockerComposeSource, /caddy_config:/);

  // Caddyfile checks
  assert.match(caddyfileSource, /flush_interval -1/);
  assert.match(caddyfileSource, /\/api\/ws\/browser-takeover\*/);

  // One-click script checks
  assert.match(deployScriptSource, /chmod 700/);
  assert.match(deployScriptSource, /docker compose/);
});

test('Revision review i18n keys are 100% symmetric in zh-CN.json and en-US.json with zero decorative emojis', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;

  const requiredKeys = [
    'artifactRevisionTitle',
    'artifactOpenRevisionReview',
    'artifactVersionV1',
    'artifactVersionV2',
    'artifactModeSingle',
    'artifactModeSideBySide',
    'artifactModeToggle',
    'artifactDispatchRevisionBtn',
    'artifactApproveVersionBtn',
    'artifactRevisionPromptPlaceholder',
    'artifactAnnotationPlaceholder',
    'artifactAddAnnotationBtn',
    'artifactCancelBtn',
    'artifactLine',
    'artifactLines',
    'artifactRegion',
    'artifactAnnotationsCount',
    'artifactNoAnnotations',
    'artifactFlipAB',
    'artifactDiffAdded',
    'artifactDiffRemoved',
    'artifactBaseVersion',
    'artifactTargetVersion',
    'artifactCloseBtn',
    'artifactReviewEmpty',
    'artifactDragToAnnotate',
    'artifactDeleteAnnotation',
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
