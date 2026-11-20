import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createKernelBridge } from '../src/kernel-bridge.mjs';
import { useJarvisHome } from '../src/composables/useJarvisHome.mjs';

const appRoot = path.resolve(import.meta.dirname, '..');

test('kernelBridge maps BOB-D-018 orchestration and bootstrap RPCs', async () => {
  const calls = [];
  const bridge = createKernelBridge(
    async (cmd, args) => {
      calls.push({ cmd, args });
      return { ok: true };
    },
    () => true,
  );

  await bridge.orchestrationList('conv-main');
  await bridge.orchestrationAccept('orch-1');
  await bridge.orchestrationContinue({ runId: 'orch-1', prompt: 'Fix test', extraRounds: 2 });
  await bridge.orchestrationCancel('orch-1');
  await bridge.nodeBootstrapWorkspace({
    connectionId: 'vps3',
    slug: 'demo-repo',
    gitUrl: 'https://github.com/org/demo.git',
  });

  assert.deepEqual(
    calls.map(c => c.cmd),
    [
      'app_orchestration_list',
      'app_orchestration_accept',
      'app_orchestration_continue',
      'app_orchestration_cancel',
      'app_node_bootstrap_workspace',
    ],
  );
  assert.equal(calls[2].args.extra_rounds, 2);
  assert.equal(calls[4].args.slug, 'demo-repo');
});

test('useJarvisHome tracks active runs and review cards and dispatches accept/continue/cancel', async () => {
  let runs = [
    {
      run_id: 'orch-active',
      status: 'running',
      title: 'Compile VPS3',
      connection_id: 'vps3',
      harness: 'codex',
      current_round: 1,
      max_auto_rounds: 3,
      stage: 'execute',
    },
    {
      run_id: 'orch-review',
      status: 'awaiting_review',
      title: 'Fix UI',
      review_card: {
        id: 'card-orch-1',
        kind: 'orchestration_review',
        title: 'Fix UI · DoD Satisfied',
        summary: 'All 270 tests passed',
        status: 'pending',
        technicalDetail: JSON.stringify({
          run_id: 'orch-review',
          connection_id: 'vps3',
          session_id: 'codex:s1',
          can_accept: true,
          can_continue: true,
          suggested_next_prompt: 'Deploy to VPS2',
        }),
      },
    },
  ];
  const ops = [];
  const jarvis = useJarvisHome({
    kernelBridge: {
      orchestrationList: async () => ({ runs }),
      orchestrationAccept: async runId => {
        ops.push(['accept', runId]);
        runs = runs.map(r => (r.run_id === runId ? { ...r, status: 'completed' } : r));
        return { ok: true };
      },
      orchestrationContinue: async args => {
        ops.push(['continue', args.runId, args.prompt]);
        return { ok: true };
      },
      orchestrationCancel: async runId => {
        ops.push(['cancel', runId]);
        runs = runs.map(r => (r.run_id === runId ? { ...r, status: 'cancelled' } : r));
        return { ok: true };
      },
    },
  });

  await jarvis.refreshOrchestrations();
  assert.equal(jarvis.activeOrchestrationRuns.value.length, 1);
  assert.equal(jarvis.reviewOrchestrationCards.value.length, 1);

  await jarvis.continueOrchestration({
    run_id: 'orch-review',
    suggested_next_prompt: 'Deploy to VPS2',
  });
  await jarvis.acceptOrchestration('orch-review');
  await jarvis.cancelOrchestration('orch-active');

  assert.deepEqual(ops, [
    ['continue', 'orch-review', 'Deploy to VPS2'],
    ['accept', 'orch-review'],
    ['cancel', 'orch-active'],
  ]);
  assert.equal(jarvis.activeOrchestrationRuns.value.length, 0);
  assert.equal(jarvis.reviewOrchestrationCards.value.length, 0);
});

test('orchestration i18n keys exist symmetrically and ActionCard supports orchestration_review', () => {
  const zh = JSON.parse(fs.readFileSync(path.join(appRoot, 'src/locales/zh-CN.json'), 'utf8'));
  const en = JSON.parse(fs.readFileSync(path.join(appRoot, 'src/locales/en-US.json'), 'utf8'));
  for (const key of [
    'cardOrchAccept',
    'cardOrchContinue',
    'cardOrchInspect',
    'orchBannerTitle',
    'orchRoundBadge',
  ]) {
    assert.ok(zh[key], `Missing zh-CN key: ${key}`);
    assert.ok(en[key], `Missing en-US key: ${key}`);
  }
  const cardVue = fs.readFileSync(path.join(appRoot, 'src/components/ActionCard.vue'), 'utf8');
  assert.ok(cardVue.includes("kind === 'orchestration_review'"));
  assert.ok(cardVue.includes('orch-accept'));
  assert.ok(cardVue.includes('orch-continue'));
  assert.ok(cardVue.includes('orch-inspect'));
});
