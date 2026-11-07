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
  ];

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
