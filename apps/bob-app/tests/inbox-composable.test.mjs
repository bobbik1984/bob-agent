import { test } from 'node:test';
import assert from 'node:assert/strict';
import { useInbox } from '../src/composables/useInbox.mjs';

test('useInbox computes counts and filters sparks correctly', async () => {
  const calls = [];
  const fakeBridge = {
    inboxList: async () => [
      { id: '1', content: 'Spark 1', created_at: 100, triage_status: 'unsorted' },
      { id: '2', content: 'Spark 2', created_at: 200, triage_status: 'sorted', destination_kind: 'project_todo' },
      { id: '3', content: 'Spark 3', created_at: 300, triage_status: 'sorted', destination_kind: 'keep' },
      { id: '4', content: 'Spark 4', created_at: 400, triage_status: 'unsorted' },
    ],
  };

  const inbox = useInbox({
    available: true,
    kernelBridge: fakeBridge,
    t: k => k,
  });

  await inbox.loadInbox();

  assert.equal(inbox.inboxItems.value.length, 4);
  assert.equal(inbox.unsortedCount.value, 2);
  assert.equal(inbox.sortedCount.value, 1);
  assert.equal(inbox.laterCount.value, 1);

  // default filter 'all': newest first
  assert.deepEqual(
    inbox.filteredItems.value.map(x => x.id),
    ['4', '3', '2', '1'],
  );

  // filter 'unsorted'
  inbox.inboxFilter.value = 'unsorted';
  assert.deepEqual(
    inbox.filteredItems.value.map(x => x.id),
    ['4', '1'],
  );

  // filter 'later'
  inbox.inboxFilter.value = 'later';
  assert.deepEqual(
    inbox.filteredItems.value.map(x => x.id),
    ['3'],
  );

  // filter 'sorted'
  inbox.inboxFilter.value = 'sorted';
  assert.deepEqual(
    inbox.filteredItems.value.map(x => x.id),
    ['2'],
  );
});

test('useInbox zero-decision capture calls inboxCapture and reloads', async () => {
  const calls = [];
  const fakeBridge = {
    inboxCapture: async args => {
      calls.push({ cmd: 'capture', args });
      return { id: args.id };
    },
    inboxList: async () => {
      calls.push({ cmd: 'list' });
      return [{ id: 'new-1', content: 'Captured text', triage_status: 'unsorted' }];
    },
  };

  const inbox = useInbox({
    available: true,
    kernelBridge: fakeBridge,
    t: k => k,
  });

  inbox.captureInput.value = 'Captured text';
  await inbox.submitZeroDecisionCapture();

  assert.equal(inbox.captureInput.value, '');
  assert.equal(calls.length, 2);
  assert.equal(calls[0].cmd, 'capture');
  assert.equal(calls[0].args.content, 'Captured text');
  assert.equal(calls[1].cmd, 'list');
  assert.equal(inbox.unsortedCount.value, 1);
});

test('useInbox triage draft lifecycle: draft, apply suggestion, apply all', async () => {
  const calls = [];
  const fakeBridge = {
    inboxTriageDraft: async ({ regenerate }) => ({
      id: 'batch-42',
      status: 'draft',
      batch: {
        suggestions: [
          {
            spark_ids: ['spark-a'],
            destination: 'calendar',
            title: 'Meeting tomorrow',
            start_time: '15:00',
          },
          {
            spark_ids: ['spark-b'],
            destination: 'project_todo',
            project_id: 'proj-bob',
            project_title: 'bob-agent',
          },
        ],
      },
    }),
    inboxTriageSet: async args => {
      calls.push({ cmd: 'triage_set', args });
      return { ok: true };
    },
    inboxTriageClose: async args => {
      calls.push({ cmd: 'triage_close', args });
      return { ok: true };
    },
    inboxList: async () => [
      { id: 'spark-a', triage_status: 'unsorted', triage_version: 0 },
      { id: 'spark-b', triage_status: 'unsorted', triage_version: 0 },
    ],
  };

  const inbox = useInbox({
    available: true,
    kernelBridge: fakeBridge,
    t: k => k,
  });

  await inbox.loadInbox();
  await inbox.loadTriageDraft(true);

  assert.ok(inbox.triageDraft.value);
  assert.equal(inbox.triageDraft.value.batch.suggestions.length, 2);

  // Apply single suggestion
  await inbox.applyTriageSuggestion(0);
  assert.equal(inbox.triageDraft.value.batch.suggestions[0]._applied, true);
  assert.equal(calls[0].cmd, 'triage_set');
  assert.equal(calls[0].args.captureId, 'spark-a');

  // Apply second suggestion finishes the batch
  await inbox.applyTriageSuggestion(1);
  assert.equal(inbox.triageDraft.value, null); // closed when all done
  assert.equal(calls[calls.length - 1].cmd, 'triage_close');
  assert.equal(calls[calls.length - 1].args.status, 'applied');
});
