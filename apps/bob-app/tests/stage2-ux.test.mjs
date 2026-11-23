import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('Stage 2: DshFilePreview backdrop blur and frosted overlay', () => {
  const previewSource = readFileSync(new URL('../src/components/DshFilePreview.vue', import.meta.url), 'utf8');
  assert.match(previewSource, /\.dsh-preview::backdrop\s*\{[^}]*backdrop-filter:\s*blur/);
  assert.match(previewSource, /onBackdropClick/);
  assert.match(previewSource, /@click="onBackdropClick"/);
  assert.match(previewSource, /dsh-modal-blur-active/);
  const styleSource = readFileSync(new URL('../src/style.css', import.meta.url), 'utf8');
  assert.match(styleSource, /body\.dsh-modal-blur-active/);
  assert.match(styleSource, /filter:\s*blur/);
});

test('Stage 2: DshSessions voice dictation and session archive noise reduction', () => {
  const dshSource = readFileSync(new URL('../src/components/DshSessions.vue', import.meta.url), 'utf8');
  assert.match(dshSource, /voice-btn/);
  assert.match(dshSource, /toggleVoiceInput/);
  assert.match(dshSource, /toggleArchiveSession/);
  assert.match(dshSource, /archivedList/);
  assert.match(dshSource, /dsh-archived-group/);
});

test('Stage 2: DshArtifactLink supported document types include PDF and HTML', () => {
  const linkSource = readFileSync(new URL('../src/components/DshArtifactLink.vue', import.meta.url), 'utf8');
  assert.match(linkSource, /pdf/);
  assert.match(linkSource, /html/);
});

test('Stage 2: Locale coverage for voice input and session archive', () => {
  const zh = JSON.parse(readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'));
  const en = JSON.parse(readFileSync(new URL('../src/locales/en-US.json', import.meta.url), 'utf8'));

  const requiredKeys = [
    'voiceInputBtn',
    'voiceInputListening',
    'voiceInputUnsupported',
    'voiceInputError',
    'dshArchiveSession',
    'dshUnarchiveSession',
    'dshArchivedSessions',
    'dshSearchInSession',
    'dshSearchMessagesPlaceholder',
    'dshSearchMatchCount',
    'dshSearchClear',
    'dshSearchNoMatches',
    'processExecution',
    'processExecutionSteps',
    'auditLogSummary',
    'viewport720p',
    'viewport1080p',
    'viewportTablet',
    'bgCheckerboard',
    'visualAssetsCount',
    'quoteFeedbackBtn',
  ];

  assert.equal(zh.dshAgent, 'DSH');
  assert.equal(en.dshAgent, 'DSH');

  for (const k of requiredKeys) {
    assert.ok(zh[k], `Missing zh-CN key: ${k}`);
    assert.ok(en[k], `Missing en-US key: ${k}`);
  }
});

test('Stage 2: DshSessions in-conversation search, snippet match, and composer thumbnail preview', () => {
  const dshSource = readFileSync(new URL('../src/components/DshSessions.vue', import.meta.url), 'utf8');
  assert.match(dshSource, /dsh-search-toggle-btn/);
  assert.match(dshSource, /inSessionSearchOpen/);
  assert.match(dshSource, /filteredDisplayMessages/);
  assert.match(dshSource, /dsh-search-snippet/);
  assert.match(dshSource, /chip-thumb/);
});

test('Stage 2.5: DshSessions renders HarnessIcon across session cards, topbar, and message headers', () => {
  const dshSource = readFileSync(new URL('../src/components/DshSessions.vue', import.meta.url), 'utf8');
  assert.match(dshSource, /import HarnessIcon from '\.\/HarnessIcon\.vue';/);
  assert.match(dshSource, /class="session-card-icon"/);
  assert.match(dshSource, /class="topbar-harness-icon"/);
  assert.match(dshSource, /class="role-harness-icon"/);
});

test('Stage 2.5: AUDIT_LINE_RE correctly detects code checklist lines and MessageContent supports audit & execution folding', async () => {
  const { AUDIT_LINE_RE, parseMessageSegments } = await import('../src/thinking-process.mjs');
  const sampleAudit = 'tests/build.gd:49 [BREAKS] hardcoded\nscripts/tune.gd:205 [CHECK] tilt_shift\nres.tres:1 [CHECK] omits\nvis.md:15 [DOC] specs';
  const segs = parseMessageSegments(sampleAudit);
  assert.equal(segs.length, 1);
  assert.equal(segs[0].kind, 'audit');
  assert.equal(segs[0].lines.length, 4);

  const mcSource = readFileSync(new URL('../src/components/MessageContent.vue', import.meta.url), 'utf8');
  assert.match(mcSource, /audit-checklist-box/);
  assert.match(mcSource, /execution-process-box/);
});

test('Stage 2 Design Studio: DshFilePreview supports interactive HTML viewports (720p/1080p/tablet), orientation, reload, and image checkerboard inspector', () => {
  const previewSource = readFileSync(new URL('../src/components/DshFilePreview.vue', import.meta.url), 'utf8');
  assert.match(previewSource, /mobile_720/);
  assert.match(previewSource, /mobile_1080/);
  assert.match(previewSource, /getViewportDimensions/);
  assert.match(previewSource, /toggleOrientation/);
  assert.match(previewSource, /reloadHtml/);
  assert.match(previewSource, /canvas-checkerboard/);
  assert.match(previewSource, /canvas-dark/);
  assert.match(previewSource, /canvas-light/);
  assert.match(previewSource, /html-device-frame/);
  assert.match(previewSource, /dsh-inspector-toolbar/);
});

test('Stage 2 Design Studio: DshSessions scans visual assets and renders visual asset reel strip', () => {
  const dshSource = readFileSync(new URL('../src/components/DshSessions.vue', import.meta.url), 'utf8');
  assert.match(dshSource, /sessionVisualAssets/);
  assert.match(dshSource, /visualReelOpen/);
  assert.match(dshSource, /dsh-visual-reel-toggle-btn/);
  assert.match(dshSource, /dsh-visual-reel-strip/);
  assert.match(dshSource, /visual-reel-card/);
  assert.match(dshSource, /openAssetPreview/);
});

test('Stage 2 Design Studio: Visual feedback loop quotes images to prompt composer, and supports paste / drag-drop attachments with workspace', () => {
  const previewSource = readFileSync(new URL('../src/components/DshFilePreview.vue', import.meta.url), 'utf8');
  assert.match(previewSource, /quoteForFeedback/);
  assert.match(previewSource, /quote-feedback-btn/);
  assert.match(previewSource, /defineEmits\(\['close',\s*'quote'\]\)/);

  const dshSource = readFileSync(new URL('../src/components/DshSessions.vue', import.meta.url), 'utf8');
  assert.match(dshSource, /onQuoteToPrompt/);
  assert.match(dshSource, /onComposerPaste/);
  assert.match(dshSource, /onComposerDrop/);
  assert.match(dshSource, /reel-card-quote-btn/);
  assert.match(dshSource, /workspace:\s*selected\.value\?\.workspace/);
});


test('Stage 3 Chief of Staff: cross-node task normalization and test session shielding', () => {
  const cdSource = readFileSync(new URL('../src/conversation-directory.mjs', import.meta.url), 'utf8');
  assert.match(cdSource, /resolveCanonicalProject/);
  assert.match(cdSource, /isTestOrScratchSession/);
  assert.match(cdSource, /CANONICAL_PROJECTS/);
  assert.match(cdSource, /isTestGroup/);

  const dshSource = readFileSync(new URL('../src/components/DshSessions.vue', import.meta.url), 'utf8');
  assert.match(dshSource, /isTestOrScratchSession/);
  assert.match(dshSource, /testList/);
  assert.match(dshSource, /dsh-test-group/);

  const dirSource = readFileSync(new URL('../src/components/ConversationDirectory.vue', import.meta.url), 'utf8');
  assert.match(dirSource, /directory-test-group/);
  assert.match(dirSource, /node-tag-pill/);

  const zhLocale = JSON.parse(readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'));
  assert.ok(zhLocale.testAndScratchSessions);
  const enLocale = JSON.parse(readFileSync(new URL('../src/locales/en-US.json', import.meta.url), 'utf8'));
  assert.ok(enLocale.testAndScratchSessions);
});
