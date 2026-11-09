import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const source = readFileSync(new URL('../src/composables/useChatSession.mjs', import.meta.url), 'utf8');
const readSection = (from, through) => source.slice(source.indexOf(from), source.indexOf(through));
const identity = { id: 'node-1', url: 'https://example.invalid/' };
const workspace = '/projects/demo';

function mount(bridge, storage = new Map()) {
  const opened = [];
  const ctx = vm.createContext({
    ref: value => ({value}),
    computed: get => ({get value() { return get(); }}),
    ctx: {modelHub: {value: {state: {remote_nodes: [identity]}}}},
    kernelBridge: bridge,
    localStorage: {
      getItem: key => storage.get(key),
      setItem: (key,value) => storage.set(key,value),
      removeItem: key => storage.delete(key),
    },
    crypto: {randomUUID: () => 'test-uuid'},
    TextEncoder,
    available: true,
    browserClient: true,
    navigationOpen: {value: false},
    t: key => key,
    applyCatalogPreferences: () => {},
    selectRemoteConversation: target => opened.push(target),
  });
  const script = [
    readSection('const creatingKind =', 'const layoutState ='),
    readSection('function beginCreateItem(', 'async function cancel(execution)'),
    'globalThis.actions = {startCreateProject, startCreateConversation, newItemTitle, creationMode, creationConnectionId, creationWorkspace, creationWorkspaces, loadCreationWorkspaces, submitCreateItem, creationError}',
  ].join('\n');
  vm.runInContext(script, ctx);
  return {a:ctx.actions, opened, storage};
}

async function chooseDsh(a) {
  a.startCreateProject();
  a.newItemTitle.value = 'Demo';
  a.creationMode.value = 'dsh';
  a.creationConnectionId.value = identity.id;
  await a.loadCreationWorkspaces();
  assert.deepEqual(Array.from(a.creationWorkspaces.value), [workspace]);
  a.creationWorkspace.value = workspace;
}

test('a lost create reply and a lost project-link reply recover the same native session', async () => {
  const calls = [];
  const links = [];
  let linkWrites = 0;
  const bridge = {
    dshList: async () => [
      {source_kind:'dsh_native', workspace},
      {source_kind:'dsh_native', workspace},
      {source_kind:'claude_native', workspace:'/another'},
    ],
    createProject: async id => ({projects:[{id}]}),
    projectLinks: async () => ({links}),
    dshCreate: async (c,id,cwd,title) => {
      calls.push([c,id,cwd,title]);
      if (calls.length === 1) throw Error('lost acknowledgement');
      return {address:{connection_id:c,session_id:id},workspace:cwd,title,source_kind:'dsh_native'};
    },
    projectLinkSet: async arg => {
      linkWrites++;
      links.push({connection_id:arg.connectionId,harness:arg.harness,session_id:arg.sessionId,project_id:arg.projectId});
      throw Error('lost association acknowledgement');
    },
  };
  const initial = mount(bridge);
  await chooseDsh(initial.a);
  await initial.a.submitCreateItem();
  assert.equal(initial.a.creationError.value, 'createRemoteRetry');
  assert.equal(initial.opened.length, 0);

  const restored = mount(bridge, initial.storage);
  restored.a.startCreateProject();
  await restored.a.submitCreateItem();
  assert.equal(restored.a.creationError.value, 'createLinkUnconfirmed');
  await restored.a.submitCreateItem();
  assert.equal(linkWrites, 1);
  assert.deepEqual(calls, Array(3).fill([identity.id,'session-test-uuid',workspace,'Demo']));
  assert.equal(restored.opened[0].session.id, 'session-test-uuid');
  assert.equal(restored.storage.size, 0);
});

test('read-only project associations cannot launch a remote session', async () => {
  let createCalls = 0;
  const m = mount({
    dshList: async () => [{source_kind:'dsh_native', workspace}],
    projectLinks: async () => ({links:[], writable:false}),
    dshCreate: async () => { createCalls++; },
  });
  await chooseDsh(m.a);
  await m.a.submitCreateItem();
  assert.equal(createCalls, 0);
  assert.equal(m.opened.length, 0);
  assert.equal(m.storage.size, 0);
  assert.equal(m.a.creationError.value, 'createProjectLinkUnavailable');
});
