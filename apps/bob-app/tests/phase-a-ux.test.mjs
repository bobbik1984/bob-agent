import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { createPersistentPreviewCache } from '../src/persistent-preview-cache.mjs';
import { createReadingCache, readingKey } from '../src/reading-cache.mjs';

const dshSource = readFileSync(new URL('../src/components/DshSessions.vue', import.meta.url), 'utf8')
  .split('<script setup>')[1]
  .split('</script>')[0]
  .replace(/^import .*;\r?\n/gm, '');

const previewSource = readFileSync(new URL('../src/components/DshFilePreview.vue', import.meta.url), 'utf8');
const previewScript = previewSource
  .split('<script setup>')[1]
  .split('</script>')[0]
  .replace(/^import .*;\r?\n/gm, '');

function mountDsh(storage = new Map(), overrides = {}) {
  const clipboardWrites = [];
  let openFileHandler = null;
  const bridge = {
    dshPage: async (c, s) => ({
      address: { connection_id: c, session_id: s },
      messages: [],
      cursor: 20,
      before: 0,
      has_more: false,
    }),
    dshPrompt: async () => ({ status: 'accepted', session_id: 'original', request_id: 'request-1' }),
    dshReceipt: async () => ({ status: 'unknown' }),
    dshPreview: async (c, s, p) => ({
      connection_id: c,
      session_id: s,
      path: p,
      extension: 'md',
      bytes: 4,
      data: 'dGVzdA==',
      version: 'v1',
    }),
    ...overrides,
  };
  const context = {
    createPersistentPreviewCache,
    readingCache: createReadingCache({ storage: null }),
    readingKey,
    provide: (k, v) => {
      if (k === 'dshOpenFile') openFileHandler = v;
    },
    nextTick: async () => {},
    defineProps: () => ({ nodes: [], target: null }),
    ref: value => ({ value }),
    computed: f => ({
      get value() {
        return f();
      },
    }),
    watch: () => {},
    onUnmounted: () => {},
    useI18n: () => ({ t: x => x }),
    kernelBridge: bridge,
    localStorage: {
      getItem: k => storage.get(k) || null,
      setItem: (k, v) => storage.set(k, v),
    },
    navigator: {
      clipboard: {
        writeText: async text => {
          clipboardWrites.push(text);
        },
      },
    },
    crypto: { randomUUID: () => 'request-1' },
    setTimeout: () => 0,
    clearTimeout: () => {},
  };
  vm.createContext(context);
  vm.runInContext(
    dshSource +
      '\nglobalThis.actions={open,send,refresh,closePreview,preview,reader,history,draft,syncStatus,syncStatusLabel,hasNewMessages,scrollToBottom,formatMessageTime,copyMessage,copiedSeq,sending,awaitingReply,nodeActivity,replying,activityLabel,fileCache,connection};',
    context,
  );
  context.actions.connection.value = 'device';
  return {
    a: context.actions,
    context,
    clipboardWrites,
    openFile: path => openFileHandler?.(path),
  };
}

test('Step A1: deliverable preview supports top-left back, safe-area inset, and layered zoom -> preview -> session back', async () => {
  assert.match(previewSource, /env\(safe-area-inset-top/);
  assert.match(previewSource, /class="dsh-preview-back"/);

  const historyStack = [];
  const emitted = [];
  let mountedHook = null;
  let popHandler = null;

  const ctx = {
    defineProps: () => ({ address: { connectionId: 'c', sessionId: 's', path: 'docs/report.png' } }),
    defineEmits: () => event => emitted.push(event),
    useI18n: () => ({ t: x => x }),
    provide: () => {},
    inject: () => async () => ({
      connection_id: 'c',
      session_id: 's',
      path: 'docs/report.png',
      extension: 'png',
      bytes: 4,
      data: 'dGVzdA==',
    }),
    ref: value => ({ value }),
    onMounted: fn => {
      mountedHook = fn;
    },
    onUnmounted: () => {},
    history: {
      pushState: state => historyStack.push(state),
      back: () => {
        historyStack.pop();
        popHandler?.();
      },
      go: delta => {
        for (let i = 0; i < Math.abs(delta); i++) historyStack.pop();
      },
    },
    window: {
      addEventListener: (ev, fn) => {
        if (ev === 'popstate') popHandler = fn;
      },
      removeEventListener: () => {},
    },
    URL: { createObjectURL: () => 'blob:test', revokeObjectURL: () => {} },
    Blob: class {},
    Uint8Array,
    atob: () => 'test',
    TextDecoder,
  };
  vm.createContext(ctx);
  vm.runInContext(
    previewScript + '\nglobalThis.previewActions={zoomed,toggleZoom,handleBack,closeAll,onPopState};',
    ctx,
  );
  await mountedHook();
  assert.equal(historyStack.length, 1);
  assert.equal(ctx.previewActions.zoomed.value, false);

  // Enter image zoom -> pushes second layer
  ctx.previewActions.toggleZoom();
  assert.equal(ctx.previewActions.zoomed.value, true);
  assert.equal(historyStack.length, 2);

  // First Android/browser back exits zoom without closing preview
  ctx.previewActions.onPopState();
  assert.equal(ctx.previewActions.zoomed.value, false);
  assert.equal(emitted.length, 0);

  // Second Android/browser back closes preview and returns to session
  ctx.previewActions.onPopState();
  assert.deepEqual(emitted, ['close']);

  // Verify DshSessions restores exact scroll position after closing preview without reloading session
  const m = mountDsh();
  const el = { scrollHeight: 2400, scrollTop: 0, clientHeight: 600 };
  m.a.reader.value = el;
  await m.a.open({ id: 'original' });
  el.scrollTop = 420;
  m.openFile('docs/report.png');
  assert.equal(m.a.preview.value.path, 'docs/report.png');
  el.scrollTop = 0;
  await m.a.closePreview();
  assert.equal(m.a.preview.value, null);
  assert.equal(el.scrollTop, 420);
});

test('Step A2: old session displays cached body immediately, transitions syncStatus, handles offline, and shows new-message pill when scrolled up', async () => {
  const m = mountDsh();
  const cachedPage = {
    address: { connection_id: 'device', session_id: 'original' },
    messages: [{ sequence: 10, role: 'assistant', text: 'cached message' }],
    cursor: 10,
    before: 10,
    has_more: false,
  };
  await m.context.readingCache.put(readingKey('device', 'history', '', 'original'), cachedPage);

  let releaseRefresh;
  m.context.kernelBridge.dshPage = () =>
    new Promise(resolve => {
      releaseRefresh = () =>
        resolve({
          address: { connection_id: 'device', session_id: 'original' },
          messages: [{ sequence: 11, role: 'assistant', text: 'delta message' }],
          cursor: 11,
          before: 11,
          has_more: false,
        });
    });

  const el = { scrollHeight: 2000, scrollTop: 0, clientHeight: 600 };
  m.a.reader.value = el;
  const openPromise = m.a.open({ id: 'original' });
  await new Promise(r => setImmediate(r));

  // Immediately displays cached body and "checking" state
  assert.equal(m.a.history.value.messages[0].text, 'cached message');
  assert.equal(m.a.syncStatusLabel.value, 'dshSyncChecking');

  // User scrolls up to read history while background check runs
  el.scrollTop = 120;
  releaseRefresh();
  await openPromise;

  // Delta merged without forcing scroll to bottom; floating "new messages" pill shown
  assert.equal(m.a.history.value.messages.length, 2);
  assert.equal(el.scrollTop, 120);
  assert.equal(m.a.hasNewMessages.value, true);
  assert.equal(m.a.syncStatusLabel.value, 'dshSyncCompleted');

  // Clicking "new messages" scrolls to bottom and clears indicator
  m.a.scrollToBottom();
  assert.equal(el.scrollTop, 2000);
  assert.equal(m.a.hasNewMessages.value, false);

  // Offline failure preserves cached body & draft and shows offline_cached status
  m.a.draft.value = 'unsent draft';
  m.context.kernelBridge.dshPage = async () => {
    throw Error('DSH_HISTORY_UNAVAILABLE');
  };
  await m.a.refresh();
  assert.equal(m.a.syncStatusLabel.value, 'dshSyncOfflineCached');
  assert.equal(m.a.history.value.messages.length, 2);
  assert.equal(m.a.draft.value, 'unsent draft');
});

test('Step A3: message time formats real timestamps, shows date across days, never fabricates missing time, and copies raw Markdown', async () => {
  const m = mountDsh();
  assert.equal(m.a.formatMessageTime(null), '');
  assert.equal(m.a.formatMessageTime(undefined), '');
  assert.equal(m.a.formatMessageTime(0), '');
  assert.equal(m.a.formatMessageTime('invalid'), '');

  const now = new Date();
  const todayMs = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 9, 5).getTime();
  assert.equal(m.a.formatMessageTime(todayMs), '09:05');

  const pastYearMs = new Date(2025, 0, 2, 14, 30).getTime();
  assert.equal(m.a.formatMessageTime(pastYearMs), '2025-01-02 14:30');

  const md = 'Here is **bold** and `code`\n```rust\nfn main() {}\n```';
  const ok = await m.a.copyMessage({ sequence: 42, text: md });
  assert.equal(ok, true);
  assert.deepEqual(m.clipboardWrites, [md]);
  assert.equal(m.a.copiedSeq.value, 42);
});

test('Step A4: 5-stage send and streaming status covers sending -> accepted -> processing -> replying -> completed', async () => {
  const m = mountDsh();
  await m.a.open({ id: 'original' });
  assert.equal(m.a.activityLabel.value, '');

  // 1. Sending / waiting for receipt
  m.a.sending.value = true;
  assert.equal(m.a.activityLabel.value, 'dshSendingWaitReceipt');
  m.a.sending.value = false;

  // 2. Accepted / awaiting reply
  m.a.awaitingReply.value = 'request-1';
  assert.equal(m.a.activityLabel.value, 'dshAwaitingReply');

  // 3. Node processing
  m.a.nodeActivity.value = { running: true, at: Date.now() };
  assert.equal(m.a.activityLabel.value, 'dshNodeProcessing');

  // 4. Replying (incremental assistant output arriving before turn completion)
  m.context.kernelBridge.dshPage = async (c, s) => ({
    address: { connection_id: c, session_id: s },
    cursor: 25,
    before: 25,
    has_more: false,
    messages: [{ sequence: 25, role: 'assistant', text: 'partial chunk' }],
    request_results: [{ request_id: 'request-1', status: 'running' }],
  });
  await m.a.refresh();
  assert.equal(m.a.activityLabel.value, 'dshReplying');

  // 5. Completed
  m.context.kernelBridge.dshPage = async (c, s) => ({
    address: { connection_id: c, session_id: s },
    cursor: 26,
    before: 26,
    has_more: false,
    messages: [{ sequence: 26, role: 'assistant', text: 'final reply' }],
    request_results: [{ request_id: 'request-1', status: 'completed' }],
  });
  m.a.nodeActivity.value = { running: false, at: Date.now() };
  await m.a.refresh();
  assert.equal(m.a.awaitingReply.value, null);
  assert.equal(m.a.activityLabel.value, '');
});

test('App.vue lossless modular split and sidebar Agent logo anti-overflow CSS', async () => {
  const { parse, compileScript } = await import('vue/compiler-sfc');
  const appVue = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8');
  const lines = appVue.split(/\r?\n/);
  assert.ok(lines.length < 500, `Expected App.vue < 500 lines, got ${lines.length}`);
  for (const mod of [
    './composables/useChatSession.mjs',
    './composables/useOnlineWorkNode.mjs',
    './composables/useModelHub.mjs',
    './composables/usePersonalOps.mjs',
    './composables/useQuickNote.mjs',
    './views/SettingsView.vue',
    './components/NavigationDrawer.vue',
    './components/PrimaryChatPane.vue',
    './components/QuickNoteOverlay.vue',
  ]) {
    assert.ok(appVue.includes(mod), `App.vue should import ${mod}`);
  }
  const providedKeys = new Set(['t', 'locale', 'brandLogo', 'getProviderLogo', 'canCancelExecution']);
  for (const mod of [
    '../src/composables/useChatSession.mjs',
    '../src/composables/useOnlineWorkNode.mjs',
    '../src/composables/useModelHub.mjs',
    '../src/composables/usePersonalOps.mjs',
    '../src/composables/useQuickNote.mjs',
  ]) {
    const txt = readFileSync(new URL(mod, import.meta.url), 'utf8');
    const assignMatch = txt.match(/Object\.assign\(ctx,\s*\{([\s\S]*?)\}\);/);
    assert.ok(assignMatch, `${mod} should assign exports onto ctx`);
    for (const m of assignMatch[1].matchAll(/\b([a-zA-Z0-9_]+)\b/g)) {
      providedKeys.add(m[1]);
    }
  }
  for (const file of [
    '../src/App.vue',
    '../src/views/SettingsView.vue',
    '../src/components/NavigationDrawer.vue',
    '../src/components/PrimaryChatPane.vue',
    '../src/components/QuickNoteOverlay.vue',
  ]) {
    const src = readFileSync(new URL(file, import.meta.url), 'utf8');
    const { descriptor } = parse(src, { filename: file });
    const compiled = compileScript(descriptor, { id: file, inlineTemplate: true });
    const unresolvedVars = [...new Set([...compiled.content.matchAll(/_ctx\.([a-zA-Z0-9_]+)/g)].map(m => m[1]))];
    const unresolvedComponents = [...new Set([...compiled.content.matchAll(/_resolveComponent\(["']([^"']+)["']\)/g)].map(m => m[1]))];
    assert.deepEqual(unresolvedVars, [], `${file} has unresolved template variables: ${unresolvedVars.join(', ')}`);
    assert.deepEqual(unresolvedComponents, [], `${file} has unresolved components: ${unresolvedComponents.join(', ')}`);
    const injectMatch = src.match(/const\s*\{([\s\S]*?)\}\s*=\s*inject\('bobAppShell'\)/);
    if (injectMatch) {
      const keys = [...injectMatch[1].matchAll(/\b([a-zA-Z0-9_]+)\b/g)].map(x => x[1]);
      const missing = keys.filter(k => !providedKeys.has(k));
      assert.deepEqual(missing, [], `${file} injects keys missing from bobAppShell: ${missing.join(', ')}`);
    }
  }
  const styleCss = readFileSync(new URL('../src/style.css', import.meta.url), 'utf8');
  assert.match(styleCss, /\.navigation-drawer\s*>\s*\*\s*\{\s*flex-shrink:\s*0;\s*\}/);
  assert.match(styleCss, /\.sidebar-primary-agent-row\s*\{[^}]*min-height:\s*3\.35rem;/s);
  assert.match(styleCss, /\.sidebar-primary-agent-row\s*\{[^}]*flex-shrink:\s*0;/s);
});

test('Collapsible thinking & process section consolidates tool calls and preserves interleaved progress paragraphs', async () => {
  const {
    extractThinkingAndBody,
    parseMessageSegments,
    summarizeProcessSteps,
    consolidateSessionMessages,
    TERMINAL_ROD_FRAMES,
  } = await import('../src/thinking-process.mjs');
  assert.deepEqual(TERMINAL_ROD_FRAMES, ['|', '/', '-', '\\']);

  // 1. Natural tool step summaries (Claude mobile parity)
  assert.equal(
    summarizeProcessSteps(['`[Tool: Bash]` ls', '`[Tool: Read]` a.gd'], 'en-US'),
    'Ran a command, read a file',
  );
  assert.equal(
    summarizeProcessSteps(['`[Tool: Bash]` ls', '`[Tool: Read]` a.gd'], 'zh-CN'),
    '执行了 1 条命令，读取了 1 个文件',
  );
  assert.equal(
    summarizeProcessSteps(['`[Tool: Bash]` a', '`[Tool: Bash]` b', '`[Tool: Bash]` c'], 'en-US'),
    'Ran 3 commands',
  );

  // 2. Fenced thinking block + <think> tag + standalone [Tool:] paragraphs
  const parsed = extractThinkingAndBody(
    '```thinking\n- `[Tool: Bash]` cd /home/bobbik/winter-shelter\n- `[Tool: Read]` snapshots/heatmap.png\n```\n\n<think>Checking snow depth values</think>\n\n`[Tool: Grep]` KEY_F3\n\nAll 591 checks pass and CI is green.',
  );
  assert.equal(parsed.steps.length, 4);
  assert.equal(parsed.body, 'All 591 checks pass and CI is green.');

  // 3. Multi-step Claude turn consolidation preserves all visible text paragraphs and interleaves tool steps
  const rawTurn = [
    { sequence: 1, role: 'user', text: 'Agree with all 4' },
    { sequence: 2, role: 'assistant', text: 'Starting with the four setup steps, then reading the input and UI code.' },
    { sequence: 3, role: 'assistant', text: '`[Tool: Bash]` cd /home/bobbik/dsh_projects/winter-shelter' },
    { sequence: 4, role: 'assistant', text: '`[Tool: Read]` scripts/ui/game_ui.gd' },
    { sequence: 5, role: 'assistant', text: 'Setup is finished and the fixes are committed as 7586aa6.' },
    { sequence: 6, role: 'assistant', text: '`[Tool: Bash]` grep -rn "KEY_F3" scripts' },
    { sequence: 7, role: 'assistant', text: 'All 36 checks on the full touch-only loop pass.' },
  ];
  const consolidated = consolidateSessionMessages(rawTurn);
  assert.equal(consolidated.length, 2);
  assert.equal(consolidated[0].role, 'user');
  assert.equal(consolidated[1].role, 'assistant');
  assert.equal(consolidated[1].sequence, 7);
  assert.equal(consolidated[1].thinkingSteps.length, 3);
  const segs = parseMessageSegments(consolidated[1].interleavedText);
  assert.equal(segs.length, 5);
  assert.equal(segs[0].kind, 'text');
  assert.equal(segs[1].kind, 'steps');
  assert.equal(segs[1].steps.length, 2);
  assert.equal(segs[2].kind, 'text');
  assert.equal(segs[3].kind, 'steps');
  assert.equal(segs[4].kind, 'text');
});
