import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parse, compileScript } from 'vue/compiler-sfc';
import { createKernelBridge } from '../src/kernel-bridge.mjs';
import { useJarvisHome } from '../src/composables/useJarvisHome.mjs';

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

function createMockSseStream(events) {
  const encoder = new TextEncoder();
  const chunks = events.map(evt => `data: ${JSON.stringify(evt)}\n\n`);
  let index = 0;
  return new ReadableStream({
    pull(controller) {
      if (index < chunks.length) {
        controller.enqueue(encoder.encode(chunks[index]));
        index += 1;
      } else {
        controller.close();
      }
    },
  });
}

test('kernelBridge maps modelPresetsList, jarvisWorkerTasks, jarvisWorkerCancel, and featureReportError RPCs', async () => {
  const calls = [];
  const bridge = createKernelBridge(
    async (command, args) => {
      calls.push({ command, args });
      return { ok: true };
    },
    () => true,
  );

  await bridge.modelPresetsList();
  await bridge.jarvisWorkerTasks('conv-main', 3);
  await bridge.jarvisWorkerTasks();
  await bridge.jarvisWorkerCancel('worker-task-1');
  await bridge.featureReportError('daily_focus_matrix', 'ReferenceError: x is not defined');

  assert.deepEqual(calls, [
    { command: 'app_model_presets_list', args: {} },
    { command: 'app_jarvis_worker_tasks', args: { sessionId: 'conv-main', afterSeq: 3 } },
    { command: 'app_jarvis_worker_tasks', args: { sessionId: null, afterSeq: 0 } },
    { command: 'app_jarvis_worker_cancel', args: { taskId: 'worker-task-1' } },
    {
      command: 'app_feature_report_error',
      args: { name: 'daily_focus_matrix', error: 'ReferenceError: x is not defined' },
    },
  ]);
});

test('Clerk fast reply + task_dispatched unlocks jarvisStreaming immediately while tracking background Worker & followup_batched', async () => {
  const origWindow = globalThis.window;
  const origFetch = globalThis.fetch;

  try {
    globalThis.window = {
      __BOB_NODE_RUNTIME__: true,
      __BOB_NODE_TOKEN__: 'tok-dual-mode',
    };

    let cancelCalledWith = null;
    const mockBridge = {
      jarvisWorkerTasks: async () => [
        {
          task_id: 'wrk-101',
          conversation_id: 'sess-jarvis',
          title: 'Build Margin Calculator Widget',
          objective: 'Create interactive margin calculator widget',
          maker_model: 'deepseek-reasoner',
          status: 'running',
          steps: [
            { seq: 1, tool_name: 'feature_scaffold', summary: 'Drafting widget schema' },
            { seq: 2, tool_name: 'feature_verify', summary: 'Validating sandbox HTML' },
          ],
        },
      ],
      jarvisWorkerCancel: async taskId => {
        cancelCalledWith = taskId;
        return { ok: true, task_id: taskId, status: 'cancelled' };
      },
    };

    const jarvis = useJarvisHome({
      kernelBridge: mockBridge,
      reload: async () => {},
    });

    // Turn 1: Clerk streams fast acknowledgment and dispatches deep task to Maker Worker
    globalThis.fetch = async () => ({
      ok: true,
      status: 200,
      body: createMockSseStream([
        { type: 'status', mood: 'speaking' },
        { type: 'text_delta', text: 'On it — building your margin calculator in the background.' },
        {
          type: 'task_dispatched',
          taskId: 'wrk-101',
          title: 'Build Margin Calculator Widget',
          objective: 'Create interactive margin calculator widget',
          makerModel: 'deepseek-reasoner',
        },
        { type: 'done', ok: true, message_id: 'msg-clerk-1' },
      ]),
    });

    const deltas = [];
    await jarvis.runJarvisTurn({
      sessionId: 'sess-jarvis',
      requestId: 'req-1',
      messageId: 'usr-1',
      content: 'Create a quote margin calculator widget for me',
      onDelta: txt => deltas.push(txt),
    });

    // Clerk reply finished -> jarvisStreaming MUST unlock immediately so chat input stays 100% open!
    assert.equal(jarvis.jarvisStreaming.value, false);
    assert.equal(jarvis.jarvisSpeaking.value, false);
    assert.equal(deltas.at(-1), 'On it — building your margin calculator in the background.');

    // Meanwhile activeWorkerTasks tracks background Worker and orbMood reflects 'thinking'
    assert.equal(jarvis.activeWorkerTasks.value.length, 1);
    assert.equal(jarvis.activeWorkerTasks.value[0].task_id, 'wrk-101');
    assert.equal(jarvis.activeWorkerTasks.value[0].maker_model, 'deepseek-reasoner');
    assert.equal(jarvis.orbMood.value, 'thinking');

    // Turn 2 while Worker is still running: user sends follow-up -> followup_batched updates active task
    globalThis.fetch = async () => ({
      ok: true,
      status: 200,
      body: createMockSseStream([
        { type: 'text_delta', text: 'Got it, added currency selector to the active task.' },
        {
          type: 'followup_batched',
          taskId: 'wrk-101',
          summary: 'Include USD/CNY currency toggle',
        },
        { type: 'done', ok: true, message_id: 'msg-clerk-2' },
      ]),
    });

    await jarvis.runJarvisTurn({
      sessionId: 'sess-jarvis',
      requestId: 'req-2',
      messageId: 'usr-2',
      content: 'Also include USD/CNY currency toggle',
    });

    assert.equal(jarvis.jarvisStreaming.value, false);
    assert.equal(jarvis.activeWorkerTasks.value[0].followupBatched, true);
    assert.equal(jarvis.activeWorkerTasks.value[0].followupCount, 1);
    assert.equal(
      jarvis.activeWorkerTasks.value[0].followupSummary,
      'Include USD/CNY currency toggle',
    );

    // Poll StepLedger updates
    await jarvis.refreshWorkerTasks('sess-jarvis');
    assert.equal(jarvis.activeWorkerTasks.value[0].steps.length, 2);
    assert.equal(
      jarvis.activeWorkerTasks.value[0].steps[1].summary,
      'Validating sandbox HTML',
    );

    // Explicit Cancel invokes app_jarvis_worker_cancel and clears activeWorkerTasks
    await jarvis.cancelWorkerTask('wrk-101');
    assert.equal(cancelCalledWith, 'wrk-101');
    assert.equal(jarvis.activeWorkerTasks.value.length, 0);
    assert.equal(jarvis.orbMood.value, 'idle');
    jarvis.stopWorkerPolling();
  } finally {
    globalThis.window = origWindow;
    globalThis.fetch = origFetch;
  }
});

test('StepLedger disconnect recovery & automatic completion sync via refreshJarvisState', async () => {
  let ledgerState = [
    {
      task_id: 'wrk-recover-7',
      conversation_id: 'sess-main',
      title: 'Analyze Quarterly Report',
      objective: 'Summarize revenue & margin trends',
      maker_model: 'gpt-4.1',
      status: 'running',
      steps: [
        { seq: 1, kind: 'read_file', summary: 'Parsed Q3 spreadsheet rows' },
        { seq: 2, kind: 'compute', summary: 'Calculated YoY operating margin' },
      ],
    },
  ];
  let reloadCount = 0;
  let featuresRefreshed = 0;

  // Simulate fresh browser reload (new composable instance with empty local state)
  const recoveredHome = useJarvisHome({
    kernelBridge: {
      jarvisWorkerTasks: async sessionId => {
        assert.equal(sessionId, 'sess-main');
        return ledgerState;
      },
      jarvisApprovalsList: async () => [],
      jarvisBudgetGet: async () => ({ monthly_cap_usd: 20, spent_usd: 2.1 }),
      featuresList: async () => {
        featuresRefreshed += 1;
        return [];
      },
    },
    reload: async () => {
      reloadCount += 1;
    },
  });

  assert.equal(recoveredHome.activeWorkerTasks.value.length, 0);

  // Recover active Worker task and its StepLedger progress immediately on refreshJarvisState
  await recoveredHome.refreshJarvisState('sess-main');
  assert.equal(recoveredHome.activeWorkerTasks.value.length, 1);
  assert.equal(recoveredHome.activeWorkerTasks.value[0].task_id, 'wrk-recover-7');
  assert.equal(recoveredHome.activeWorkerTasks.value[0].steps.length, 2);
  assert.equal(recoveredHome.orbMood.value, 'thinking');

  const prevFeaturesRefreshed = featuresRefreshed;

  // Worker completes task on backend -> next refreshWorkerTasks triggers reload() & refreshJarvisState()
  ledgerState = [
    {
      ...ledgerState[0],
      status: 'completed',
      steps: [
        ...ledgerState[0].steps,
        { seq: 3, kind: 'finish', summary: 'Published executive summary' },
      ],
    },
  ];

  await recoveredHome.refreshWorkerTasks('sess-main');
  assert.equal(recoveredHome.activeWorkerTasks.value.length, 0);
  assert.equal(recoveredHome.orbMood.value, 'idle');
  assert.equal(reloadCount, 1);
  assert.ok(featuresRefreshed > prevFeaturesRefreshed, 'refreshFeatures triggered on Worker completion');
  recoveredHome.stopWorkerPolling();
});

test('Static audit on PrimaryChatPane.vue: compiles cleanly, renders worker step ledger, zero hardcoded colors & zero emoji', () => {
  const { descriptor } = parse(primaryChatPaneSource, { filename: 'PrimaryChatPane.vue' });
  const compiled = compileScript(descriptor, { id: 'PrimaryChatPane.vue', inlineTemplate: true });
  assert.ok(compiled.content, 'PrimaryChatPane.vue compiles cleanly');

  assert.match(primaryChatPaneSource, /class="jarvis-worker-banner"/);
  assert.match(primaryChatPaneSource, /class="worker-step-ledger"/);
  assert.match(primaryChatPaneSource, /cancelWorkerTask/);

  const emojiRegex = /[\u{1F000}-\u{1FAFF}\u{2600}-\u{27BF}]/u;
  const hexColorRegex = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;
  const rgbColorRegex = /\brgba?\s*\(/i;
  const hslColorRegex = /\bhsla?\s*\(/i;
  const oklchColorRegex = /\boklch\s*\(/i;

  assert.equal(emojiRegex.test(primaryChatPaneSource), false, 'Zero decorative emoji');
  assert.equal(hexColorRegex.test(primaryChatPaneSource), false, 'Zero #hex colors');
  assert.equal(rgbColorRegex.test(primaryChatPaneSource), false, 'Zero rgb/rgba colors');
  assert.equal(hslColorRegex.test(primaryChatPaneSource), false, 'Zero hsl/hsla colors');
  assert.equal(oklchColorRegex.test(primaryChatPaneSource), false, 'Zero oklch colors');

  const workerKeys = [
    'workerBannerTitle',
    'workerStatusQueued',
    'workerStatusRunning',
    'workerStatusWaitingApproval',
    'workerStepCount',
    'workerPreparingSteps',
    'workerFollowupMerged',
    'workerExpandSteps',
    'workerCollapseSteps',
    'workerCancelBtn',
    'workerCancellingBtn',
  ];
  for (const key of workerKeys) {
    assert.ok(zhLocale[key], `zh-CN.json missing ${key}`);
    assert.ok(enLocale[key], `en-US.json missing ${key}`);
    assert.equal(emojiRegex.test(zhLocale[key]), false);
    assert.equal(emojiRegex.test(enLocale[key]), false);
  }
});
