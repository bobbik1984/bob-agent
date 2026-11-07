import {artifactPath} from '../src/artifact-path.mjs';
import {isNotAcceptedReceipt} from '../src/native-receipt.mjs';
import {unconfirmActivity,settleNativeActivity,orderedNativeActivities,waitingNativeApproval} from '../src/native-activity.mjs';
import {createChatScroll} from '../src/chat-scroll.mjs';
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
  const context = {artifactPath,isNotAcceptedReceipt,unconfirmActivity,settleNativeActivity,orderedNativeActivities,waitingNativeApproval,
    createChatScroll,
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
      '\nglobalThis.actions={open,send,refresh,forceRefresh,onReaderTouchStart,onReaderTouchMove,onReaderTouchEnd,pullUpDistance,attachments,onFilesSelected,removeAttachment,closePreview,preview,reader,history,draft,syncStatus,syncStatusLabel,hasNewMessages,scrollToBottom,formatMessageTime,copyMessage,copiedSeq,sending,awaitingReply,nodeActivity,replying,activityLabel,fileCache,connection,refreshDshLive,dshLiveState,dshLiveMessages,refreshNativeEvents,nativeDrafts,nativeMessages,cancellableRequest,cancelNative,cancelRequest,notice};',
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

  const translated = lang => (key, args = {}) => JSON.parse(readFileSync(new URL(`../src/locales/${lang}.json`, import.meta.url), 'utf8'))[key].replace('{count}', String(args.count));
  // 1. Natural tool step summaries (Claude mobile parity)
  assert.equal(
    summarizeProcessSteps(['`[Tool: Bash]` ls', '`[Tool: Read]` a.gd'], 'en-US', translated('en-US')),
    'Command calls: 1, Read calls: 1',
  );
  assert.equal(
    summarizeProcessSteps(['`[Tool: Bash]` ls', '`[Tool: Read]` a.gd'], 'zh-CN', translated('zh-CN')),
    '命令调用 1 次，读取调用 1 次',
  );
  assert.equal(
    summarizeProcessSteps(['`[Tool: Bash]` a', '`[Tool: Bash]` b', '`[Tool: Bash]` c'], 'en-US', translated('en-US')),
    'Command calls: 3',
  );

  const returned = parseMessageSegments('```thinking\n- `[Tool: exec]` check\n- `[ToolResult: exec]` permission failed\n```');
  assert.equal(returned[0].steps.length, 2);
  assert.equal(summarizeProcessSteps(returned[0].steps, 'en-US', translated('en-US')), 'Command calls: 1');

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

  // 4. Terminal rod never wraps across lines and activity status never duplicates
  const dshVue = readFileSync(new URL('../src/components/DshSessions.vue', import.meta.url), 'utf8');
  assert.ok(!dshVue.includes('class="dsh-live-terminal-card"'), 'DshSessions should not duplicate activityLabel in both reader and composer');
  assert.match(dshVue, /\\u2060\$\{rodFrames/);
  const styleCss = readFileSync(new URL('../src/style.css', import.meta.url), 'utf8');
  assert.match(styleCss, /\.terminal-rod\s*\{[^}]*width:\s*3\.4ch;[^}]*white-space:\s*nowrap;[^}]*word-break:\s*keep-all;/s);
});

test('Bottom pull-up refresh, top-right force refresh icon, and unified composer box with attachments and model switcher', async () => {
  let pageCalls = 0;
  const sentPrompts = [];
  const m = mountDsh(new Map(), {
    dshPage: async (c, s) => {
      pageCalls += 1;
      return {
        address: { connection_id: c, session_id: s },
        messages: [{ sequence: pageCalls, role: 'assistant', text: 'msg ' + pageCalls }],
        cursor: pageCalls,
        before: 0,
        has_more: false,
      };
    },
    dshPrompt: async (c, s, text, reqId) => {
      sentPrompts.push(text);
      return { status: 'accepted', session_id: s, request_id: reqId };
    },
  });
  const el = { scrollHeight: 2000, scrollTop: 1400, clientHeight: 600 };
  m.a.reader.value = el;
  await m.a.open({ id: 'original' });
  assert.equal(pageCalls, 1);

  // 1. Pull up at bottom of history triggers forceRefresh
  m.a.onReaderTouchStart({ touches: [{ clientY: 500 }] });
  m.a.onReaderTouchMove({ touches: [{ clientY: 420 }] });
  assert.ok(m.a.pullUpDistance.value >= 36);
  m.a.onReaderTouchEnd();
  await new Promise(r => setImmediate(r));
  assert.equal(pageCalls, 2);

  // 2. Attachment selection & removal in composer
  await m.a.onFilesSelected({
    target: {
      files: [{ name: 'patch.diff', size: 18, type: 'text/plain', text: async () => '+fixed' }],
      value: 'C:\\fakepath\\patch.diff',
    },
  });
  assert.equal(m.a.attachments.value.length, 1);
  assert.equal(m.a.attachments.value[0].name, 'patch.diff');
  m.a.draft.value = 'Please review';
  await m.a.send();
  assert.equal(sentPrompts.length, 1);
  assert.match(sentPrompts[0], /Please review/);
  assert.match(sentPrompts[0], /\[Attachment: patch\.diff\]/);
  assert.equal(m.a.attachments.value.length, 0);

  // 3. Structural checks across DshSessions.vue, PrimaryChatPane.vue, App.vue, and style.css
  const dshVue = readFileSync(new URL('../src/components/DshSessions.vue', import.meta.url), 'utf8');
  const selectedTemplate = dshVue.split('<template v-if="selected">')[1];
  assert.ok(!selectedTemplate.includes('@click="refresh"'), 'Top refresh button removed from selected conversation header');
  assert.ok(selectedTemplate.includes('class="dsh-compose-box"'), 'Unified composer box in DshSessions');
  assert.ok(selectedTemplate.includes('class="composer-tool-btn"'), 'Attachment button in DshSessions composer');
  assert.ok(selectedTemplate.includes('class="composer-submit-btn"'), 'Fixed bottom-right send button in DshSessions composer');

  const primaryVue = readFileSync(new URL('../src/components/PrimaryChatPane.vue', import.meta.url), 'utf8');
  assert.ok(primaryVue.includes('openComposerFilePicker'), 'Attachment button in PrimaryChatPane composer');
  assert.ok(primaryVue.includes('class="composer-model-pill"'), 'Model switcher button in PrimaryChatPane composer');

  const appVue = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8');
  assert.match(appVue, /class="device-indicator"[\s\S]*?@click="forceRefreshActiveConversation"/);

  const styleCss = readFileSync(new URL('../src/style.css', import.meta.url), 'utf8');
  assert.match(styleCss, /\.device-indicator svg\{width:17px;height:17px;/);
  assert.match(styleCss, /\.composer-submit-btn\{margin-left:auto;flex-shrink:0;height:1\.85rem;/);
});

test('native snapshot refresh replaces growing text even when cursor is unchanged', async () => {
  let text='partial';
  const {a}=mountDsh(new Map(), {dshPage:async(c,s)=>({address:{connection_id:c,session_id:s},cursor:1,before:1,has_more:false,history_snapshot:true,messages:[{sequence:1,role:'assistant',text}]})});
  await a.open({id:'codex:original'});
  text='partial and final reply';await a.refresh();
  assert.equal(a.history.value.messages[0].text,text);
  assert.equal(a.hasNewMessages.value,true);
});
test('native rolling snapshot removes messages that left the window without retaining stale tail', async () => {
  let rows=[{sequence:1,role:'user',text:'old'},{sequence:2,role:'assistant',text:'old answer'}];
  const {a}=mountDsh(new Map(), {dshPage:async(c,s)=>({address:{connection_id:c,session_id:s},cursor:rows.length,before:1,has_more:false,history_snapshot:true,messages:rows})});
  await a.open({id:'codex:original'});
  rows=[{sequence:1,role:'assistant',text:'new answer'}];await a.refresh();
  assert.equal(a.history.value.messages.length,1);
  assert.equal(a.history.value.messages[0].text,'new answer');
});


test('DSH live state preserves reading position and replaces reconnect baseline without duplication', async()=>{
 let frame={address:{connection_id:'device',session_id:'original'},status:'live',cursor:20,active:true,messages:[{id:'attempt',text:'partial',active:true,sequence:null}]};
 const {a}=mountDsh(new Map(),{dshLive:async()=>frame});
 await a.open({id:'original'});await a.refreshDshLive();
 assert.equal(a.dshLiveMessages.value[0].text,'partial');
 a.reader.value={scrollHeight:1000,clientHeight:200,scrollTop:100};
 frame={...frame,messages:[{id:'attempt',text:'partial plus',active:true,sequence:null}]};
 await a.refreshDshLive();assert.equal(a.reader.value.scrollTop,100);assert.equal(a.hasNewMessages.value,true);
 frame={...frame,status:'reconnecting',active:false,messages:[]};
 await a.refreshDshLive();assert.equal(a.dshLiveMessages.value[0].text,'partial plus');assert.equal(a.replying.value,false);
 frame={...frame,status:'live',active:true,messages:[{id:'attempt',text:'partial plus restored',active:true,sequence:null}]};
 await a.refreshDshLive();await a.refreshDshLive();assert.equal(a.dshLiveMessages.value[0].text,'partial plus restored');
 a.history.value.cursor=21;frame={...frame,active:false,messages:[{id:'attempt',text:'final',active:false,sequence:21}]};
 await a.refreshDshLive();assert.equal(a.dshLiveMessages.value.length,0);
});

test('DSH live response cannot cross the selected session identity', async()=>{
 let frame={address:{connection_id:'device',session_id:'other'},status:'live',cursor:20,active:true,messages:[{id:'a',text:'wrong session',active:true}]};
 const {a}=mountDsh(new Map(),{dshLive:async()=>frame});await a.open({id:'original'});await a.refreshDshLive();
 assert.equal(a.dshLiveMessages.value.length,0);assert.equal(a.dshLiveState.value.status,'reconnecting');
});

test('DSH projection lag triggers one refresh per live cursor instead of repeated history downloads', async()=>{
 let calls=0,cursor=21;
 const {a}=mountDsh(new Map(),{
  dshPage:async(c,s)=>{calls++;return {address:{connection_id:c,session_id:s},cursor:20,before:0,has_more:false,messages:[{sequence:20,role:'assistant',text:'saved'}]};},
  dshLive:async(c,s)=>({address:{connection_id:c,session_id:s},status:'live',cursor,active:false,messages:[]})
 });
 await a.open({id:'original'});await new Promise(r=>setImmediate(r));
 const baseline=calls;
 await a.refreshDshLive();await a.refreshDshLive();await new Promise(r=>setImmediate(r));
 assert.equal(calls,baseline);
 cursor=22;await a.refreshDshLive();await new Promise(r=>setImmediate(r));
 assert.equal(calls,baseline+1);
});

test('native stop remains pending until an exact terminal event',async()=>{
 let events=[{sequence:1,request_id:'r',session_id:'pi:original',kind:'accepted',payload:{cancel_supported:true}}];
 const {a}=mountDsh(new Map(),{nativeEvents:async()=>({session_id:'pi:original',events}),nativeCancel:async()=>({session_id:'pi:original',request_id:'r',status:'cancel_requested'})});
 await a.open({id:'pi:original'});await a.refreshNativeEvents();assert.equal(a.cancellableRequest.value,'r');
 await a.cancelNative();assert.equal(a.cancelRequest.value,'r');assert.equal(a.notice.value,'nativeCancelPending');
 events=[{sequence:2,request_id:'r',session_id:'pi:original',kind:'cancelled',payload:{}}];await a.refreshNativeEvents();
 assert.ok(!a.cancellableRequest.value);assert.equal(a.notice.value,'nativeCancelled');
});
test('external disconnect retains text but cannot claim it is still generating',async()=>{
 let source='live',events=[{sequence:1,request_id:'external-codex-thread-turn',session_id:'codex:original',kind:'message_snapshot',payload:{text:'partial'}},{sequence:2,request_id:'external-codex-thread-turn',session_id:'codex:original',kind:'running',payload:{}}];
 const {a}=mountDsh(new Map(),{nativeEvents:async()=>({session_id:'codex:original',external_live:source,events})});
 await a.open({id:'codex:original'});await a.refreshNativeEvents();assert.equal(a.replying.value,true);assert.ok(!a.cancellableRequest.value);
 source='unavailable';events=[];await a.refreshNativeEvents();assert.equal(a.replying.value,false);assert.equal(a.nativeDrafts.value['external-codex-thread-turn'].text,'partial');
});

test('native live turn merges against multiple saved assistant records without hiding new text or crossing user turns',async()=>{
 const {a}=mountDsh(new Map());
 a.history.value={messages:[{role:'user',text:'request'},{role:'assistant',text:'First paragraph.'},{role:'assistant',text:'[Tool: check]'},{role:'assistant',text:'Second paragraph.'}]};
 a.nativeDrafts.value={r:{text:'First paragraph.\n\nSecond paragraph.',active:true}};
 assert.equal(a.nativeMessages.value.length,0);
 a.nativeDrafts.value={r:{text:'First paragraph.\n\nSecond paragraph.\n\nNew output.',active:true}};
 assert.equal(a.nativeMessages.value.length,1);
 a.history.value.messages.splice(2,0,{role:'user',text:'different request'});
 a.nativeDrafts.value={r:{text:'First paragraph.\n\nSecond paragraph.',completed:true}};
 assert.equal(a.nativeMessages.value.length,1);
});

test('expired event cursor replaces drafts from checkpoint then accepts only newer deltas',async()=>{
 let page={session_id:'pi:original',epoch:'store-a',cursor:100,reset:true,history_required:true,snapshots:[{request_id:'r',session_id:'pi:original',text:'restored',status:'running',cancel_supported:true}],events:[]};
 const calls=[];const {a}=mountDsh(new Map(),{nativeEvents:async(...args)=>{calls.push(args);return page;}});
 await a.open({id:'pi:original'});await a.refreshNativeEvents();assert.equal(a.nativeDrafts.value.r.text,'restored');assert.equal(a.cancellableRequest.value,'r');
 page={session_id:'pi:original',epoch:'store-a',cursor:101,events:[{sequence:101,request_id:'r',session_id:'pi:original',kind:'text_delta',payload:{text:' new'}}]};
 await a.refreshNativeEvents();await a.refreshNativeEvents();assert.equal(a.nativeDrafts.value.r.text,'restored new');assert.equal(calls.at(-1)[3],'store-a');
});
test('failed event read retains partial text and asks for a fresh baseline on reconnect',async()=>{
 let fail=false;const calls=[];const {a}=mountDsh(new Map(),{nativeEvents:async(...args)=>{calls.push(args);if(fail)throw Error('offline');return {session_id:'pi:original',epoch:'store-a',cursor:10,reset:true,snapshots:[{request_id:'r',session_id:'pi:original',text:'partial',status:'running'}],events:[]};}});
 await a.open({id:'pi:original'});await a.refreshNativeEvents();fail=true;await a.refreshNativeEvents();assert.equal(a.nativeDrafts.value.r.text,'partial');assert.equal(a.replying.value,false);
 fail=false;await a.refreshNativeEvents();assert.equal(calls.at(-1)[3],'');assert.equal(a.replying.value,true);assert.equal(a.nativeDrafts.value.r.text,'partial');
});

test('recovered old terminal checkpoints are not appended as replies to a newer user turn',()=>{
 const {a}=mountDsh(new Map());
 a.history.value={messages:[{role:'user',text:'new request',timestamp:2000}]};
 a.nativeDrafts.value={old:{text:'old result',timestamp:1000,terminal:true},newer:{text:'new result',timestamp:3000,terminal:true},pending:{text:'still running',timestamp:1500,active:true}};
 assert.deepEqual(Array.from(a.nativeMessages.value,m=>m.id),['pending','newer']);
 a.awaitingReply.value='old';assert.equal(a.nativeMessages.value.length,3);
});
