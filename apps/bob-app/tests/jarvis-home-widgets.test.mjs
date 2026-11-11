import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parse, compileScript } from 'vue/compiler-sfc';
import { ref } from 'vue';
import { createKernelBridge } from '../src/kernel-bridge.mjs';
import { streamJarvisTurn, isBobNodeRuntime } from '../src/http-bridge.mjs';
import { useJarvisHome } from '../src/composables/useJarvisHome.mjs';

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

function createMockStream(chunks) {
  const encoder = new TextEncoder();
  let idx = 0;
  return new ReadableStream({
    pull(controller) {
      if (idx < chunks.length) {
        controller.enqueue(encoder.encode(chunks[idx]));
        idx++;
      } else {
        controller.close();
      }
    },
  });
}

test('streamJarvisTurn requires native host and parses framed SSE chunks across chunk boundaries', async () => {
  // 1. Rejects if not native runtime
  const origWindow = globalThis.window;
  const origFetch = globalThis.fetch;
  try {
    globalThis.window = undefined;
    await assert.rejects(
      streamJarvisTurn({ sessionId: 's1' }, () => {}),
      /NATIVE_HOST_REQUIRED/,
    );

    // 2. Mock Bob Node Runtime
    globalThis.window = {
      __BOB_NODE_RUNTIME__: true,
      __BOB_NODE_TOKEN__: 'test-token',
    };

    // 3. HTTP error rejection
    globalThis.fetch = async () => ({
      ok: false,
      status: 404,
      json: async () => ({ error: 'NOT_FOUND' }),
    });
    await assert.rejects(
      streamJarvisTurn({ sessionId: 's1' }, () => {}),
      /NOT_FOUND/,
    );

    // 4. Split chunk SSE parsing across boundaries
    const events = [];
    const splitChunks = [
      'data: {"type":"status","mood":"thinking"}\n\ndata: {"type":"text_delta","text":"Hel',
      'lo world"}\n\n',
      'data: {"type":"approval_required","approval":{"id":"app-1","title":"Delete temp file","risk_level":"side_effect"}}\r\n\r\n',
      'data: {"type":"takeover_required","takeover":{"id":"tk-1","title":"Solve captcha","url":"https://example.com/login"}}\n\n',
      'data: {"type":"done","ok":true,"message_id":"msg-final"}\n\n',
    ];

    let capturedHeaders = null;
    let capturedBody = null;
    globalThis.fetch = async (url, opts) => {
      assert.equal(url, '/api/jarvis/v1/stream');
      assert.equal(opts.method, 'POST');
      capturedHeaders = opts.headers;
      capturedBody = JSON.parse(opts.body);
      return {
        ok: true,
        status: 200,
        body: createMockStream(splitChunks),
      };
    };

    const doneResult = await streamJarvisTurn(
      { sessionId: 's1', requestId: 'r1', content: 'hello' },
      evt => events.push(evt),
    );

    assert.equal(capturedHeaders.Accept, 'text/event-stream');
    assert.equal(capturedHeaders.Authorization, 'Bearer test-token');
    assert.equal(capturedHeaders['X-Bob-Rpc'], '1');
    assert.deepEqual(capturedBody, { sessionId: 's1', requestId: 'r1', content: 'hello' });

    assert.equal(events.length, 5);
    assert.equal(events[0].type, 'status');
    assert.equal(events[0].mood, 'thinking');
    assert.equal(events[1].type, 'text_delta');
    assert.equal(events[1].text, 'Hello world');
    assert.equal(events[2].type, 'approval_required');
    assert.equal(events[2].approval.id, 'app-1');
    assert.equal(events[3].type, 'takeover_required');
    assert.equal(events[3].takeover.id, 'tk-1');
    assert.equal(events[4].type, 'done');
    assert.equal(doneResult.message_id, 'msg-final');
  } finally {
    globalThis.window = origWindow;
    globalThis.fetch = origFetch;
  }
});

test('kernelBridge maps new jarvisChat, jarvisApprovalsList, jarvisApprove, jarvisReject, and jarvisBudget RPCs', async () => {
  const calls = [];
  const bridge = createKernelBridge(
    async (command, args) => {
      calls.push({ command, args });
      return { ok: true };
    },
    () => true,
  );

  await bridge.jarvisChat({
    sessionId: 'sess-1',
    requestId: 'req-1',
    messageId: 'msg-1',
    content: 'Do task',
  });
  await bridge.jarvisApprovalsList('sess-1');
  await bridge.jarvisApprove('appr-1');
  await bridge.jarvisReject('appr-2');
  await bridge.jarvisBudgetGet();
  await bridge.jarvisBudgetSet(25.5);

  assert.deepEqual(calls, [
    {
      command: 'app_jarvis_chat',
      args: { sessionId: 'sess-1', requestId: 'req-1', messageId: 'msg-1', content: 'Do task' },
    },
    { command: 'app_jarvis_approvals_list', args: { sessionId: 'sess-1' } },
    { command: 'app_jarvis_approve', args: { approvalId: 'appr-1' } },
    { command: 'app_jarvis_reject', args: { approvalId: 'appr-2' } },
    { command: 'app_jarvis_budget_get', args: {} },
    { command: 'app_jarvis_budget_set', args: { monthlyCapUsd: 25.5 } },
  ]);

  const unavailableBridge = createKernelBridge(() => assert.fail('should not invoke'), () => false);
  await assert.rejects(() => unavailableBridge.jarvisChat({}), /NATIVE_HOST_REQUIRED/);
  await assert.rejects(() => unavailableBridge.jarvisApprovalsList(), /NATIVE_HOST_REQUIRED/);
  await assert.rejects(() => unavailableBridge.jarvisApprove('a'), /NATIVE_HOST_REQUIRED/);
  await assert.rejects(() => unavailableBridge.jarvisReject('a'), /NATIVE_HOST_REQUIRED/);
  await assert.rejects(() => unavailableBridge.jarvisBudgetGet(), /NATIVE_HOST_REQUIRED/);
  await assert.rejects(() => unavailableBridge.jarvisBudgetSet(10), /NATIVE_HOST_REQUIRED/);
});

test('useJarvisHome manages L1 vs Pro widget visibility, reorder, persistence, and orbMood transitions', async () => {
  const storage = new Map();
  const origLocalStorage = globalThis.localStorage;
  globalThis.localStorage = {
    getItem: k => storage.get(k) || null,
    setItem: (k, v) => storage.set(k, String(v)),
    removeItem: k => storage.delete(k),
  };

  try {
    const mockCtx = {
      modelHub: ref({ state: { remote_nodes: [] } }),
      onlineState: ref({ connected: false }),
      kernelBridge: {
        jarvisApprovalsList: async () => [],
        jarvisBudgetGet: async () => ({ monthly_cap_usd: 10, spent_usd: 1.5 }),
        jarvisApprove: async id => ({ id, approved: true }),
        jarvisReject: async id => ({ id, rejected: true }),
        jarvisChat: async () => ({ ok: true }),
      },
      reload: async () => {},
    };

    const jarvis = useJarvisHome(mockCtx);

    // Initial tier is L1 (no remote nodes and not online)
    assert.equal(jarvis.hasProCluster.value, false);

    // Initial visible widgets: 'pending' is hidden because pendingApprovals is empty; 'quotas' & 'nodes' hidden because tier is pro
    const initialVisibleIds = jarvis.visibleWidgets.value.map(w => w.id);
    assert.deepEqual(initialVisibleIds, ['today', 'inbox', 'routines']);

    // When pending approval arrives, 'pending' widget auto-surfaces
    jarvis.pendingApprovals.value.push({
      id: 'app-test-1',
      kind: 'approval',
      title: 'Deploy to Prod',
      status: 'pending',
    });
    const withApprovalVisibleIds = jarvis.visibleWidgets.value.map(w => w.id);
    assert.deepEqual(withApprovalVisibleIds, ['pending', 'today', 'inbox', 'routines']);

    // orbMood transitions:
    // With pending approvals: 'needs_human'
    assert.equal(jarvis.orbMood.value, 'needs_human');

    // Approve the action
    await jarvis.approveAction('app-test-1');
    assert.equal(jarvis.pendingApprovals.value[0].status, 'approved');
    assert.equal(jarvis.pendingApprovalCount.value, 0);

    // With 0 pending approvals and not streaming/speaking: 'idle'
    assert.equal(jarvis.orbMood.value, 'idle');

    // During streaming
    jarvis.jarvisStreaming.value = true;
    assert.equal(jarvis.orbMood.value, 'thinking');

    // Speaking
    jarvis.jarvisSpeaking.value = true;
    assert.equal(jarvis.orbMood.value, 'speaking');

    jarvis.jarvisStreaming.value = false;
    jarvis.jarvisSpeaking.value = false;
    assert.equal(jarvis.orbMood.value, 'idle');

    // Move widget reorder test: move 'inbox' up (swaps order with 'today')
    jarvis.moveWidget('inbox', -1);
    const reorderedIds = jarvis.visibleWidgets.value.map(w => w.id);
    assert.deepEqual(reorderedIds, ['inbox', 'today', 'routines']);

    // Storage persistence verified
    assert.ok(storage.has('bob-jarvis-widgets-v1'));

    // Reset widgets restores defaults
    jarvis.resetWidgets();
    const resetIds = jarvis.visibleWidgets.value.map(w => w.id);
    assert.deepEqual(resetIds, ['today', 'inbox', 'routines']);

    // Pro cluster activation surfaces pro widgets when enabled
    mockCtx.modelHub.value = { state: { remote_nodes: [{ id: 'node-remote-1', enabled: true }] } };
    assert.equal(jarvis.hasProCluster.value, true);

    jarvis.toggleWidgetEnabled('quotas');
    assert.ok(jarvis.visibleWidgets.value.some(w => w.id === 'quotas'));
  } finally {
    globalThis.localStorage = origLocalStorage;
  }
});

test('useJarvisHome runJarvisTurn executes fallback cascade when native host is unavailable', async () => {
  const origWindow = globalThis.window;
  try {
    globalThis.window = undefined; // Force fallback path (!isBobNodeRuntime)

    let jarvisChatCalled = false;
    const mockCtx = {
      modelHub: { state: { remote_nodes: [] } },
      onlineState: { connected: false },
      kernelBridge: {
        jarvisChat: async ({ requestId }) => {
          jarvisChatCalled = true;
          return { id: requestId, reply: 'Chat fallback' };
        },
      },
    };

    const jarvis = useJarvisHome(mockCtx);
    const result = await jarvis.runJarvisTurn({
      sessionId: 'sess-fallback',
      requestId: 'req-fb-1',
      content: 'Hello fallback',
    });

    assert.equal(jarvisChatCalled, true);
    assert.equal(result.id, 'req-fb-1');
    assert.equal(jarvis.jarvisStreaming.value, false);
    assert.equal(jarvis.jarvisSpeaking.value, false);
  } finally {
    globalThis.window = origWindow;
  }
});

test('JarvisWidgetGrid.vue and PrimaryChatPane.vue contain zero hardcoded hex/rgb colors and zero decorative emojis', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;
  const hexColorRegex = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;
  const rgbColorRegex = /\brgba?\s*\(/i;

  for (const [name, source] of [
    ['JarvisWidgetGrid.vue', jarvisWidgetGridSource],
    ['PrimaryChatPane.vue', primaryChatPaneSource],
  ]) {
    assert.equal(emojiRegex.test(source), false, `${name} contains decorative emoji`);
    assert.equal(hexColorRegex.test(source), false, `${name} contains hardcoded hex color`);
    assert.equal(rgbColorRegex.test(source), false, `${name} contains hardcoded rgb/rgba color`);
  }
});

test('JarvisWidgetGrid.vue and PrimaryChatPane.vue compile cleanly with SFC compiler', () => {
  for (const [name, source] of [
    ['JarvisWidgetGrid.vue', jarvisWidgetGridSource],
    ['PrimaryChatPane.vue', primaryChatPaneSource],
  ]) {
    const { descriptor } = parse(source, { filename: name });
    const compiled = compileScript(descriptor, { id: name, inlineTemplate: true });
    assert.ok(compiled.content, `${name} script setup compiles successfully`);
  }
});

test('All Jarvis home widget i18n keys exist symmetrically in zh-CN.json and en-US.json with zero decorative emojis', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;

  const requiredWidgetKeys = [
    'jarvisWidgetsTitle',
    'jarvisCustomizeWidgets',
    'jarvisResetWidgets',
    'jarvisWidgetsDone',
    'jarvisMoveUp',
    'jarvisMoveDown',
    'jarvisAdvancedWidgets',
    'jarvisWidgetPendingTitle',
    'jarvisWidgetPendingDesc',
    'jarvisWidgetPendingReview',
    'jarvisWidgetTodayTitle',
    'jarvisWidgetTodayEmpty',
    'jarvisWidgetTodayOpen',
    'jarvisWidgetInboxTitle',
    'jarvisQuickSparkPlaceholder',
    'jarvisWidgetInboxUnsorted',
    'jarvisWidgetRoutinesTitle',
    'jarvisWidgetRoutinesSummary',
    'jarvisWidgetCreateRoutine',
    'jarvisCreateRoutinePrompt',
    'jarvisWidgetQuotasTitle',
    'jarvisMonthlyBudget',
    'jarvisMonthlyBudgetSpent',
    'jarvisWidgetNodesTitle',
    'jarvisWidgetNodesCount',
    'jarvisWidgetNodesOpen',
    'jarvisCloseWidgetsSheet',
    'jarvisOpenWidgetsSheet',
  ];

  for (const key of requiredWidgetKeys) {
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
