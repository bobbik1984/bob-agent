import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { markRecentProject } from '../src/recent-projects.mjs';

const chatSessionSrc = readFileSync(new URL('../src/composables/useChatSession.mjs', import.meta.url), 'utf8');
const modalSrc = readFileSync(new URL('../src/components/SidebarCreateForm.vue', import.meta.url), 'utf8');
const dshSrc = readFileSync(new URL('../src/components/DshSessions.vue', import.meta.url), 'utf8');
const zhJson = JSON.parse(readFileSync(new URL('../src/locales/zh-CN.json', import.meta.url), 'utf8'));
const enJson = JSON.parse(readFileSync(new URL('../src/locales/en-US.json', import.meta.url), 'utf8'));

const readSection = (src, from, through) => src.slice(src.indexOf(from), src.indexOf(through));

function mountCreateModalState(bridge, projects = []) {
  const storage = new Map();
  const navigationOpen = { value: true };
  const ctx = vm.createContext({
    markRecentProject,
    ref: value => ({ value }),
    computed: get => ({ get value() { return get(); } }),
    ctx: {
      modelHub: {
        value: {
          state: {
            remote_nodes: [
              { id: 'vps2', name: 'VPS2 Build Host', url: 'https://vps2.example.invalid/' },
              { id: 'empty-node', name: 'Empty Node', url: 'https://empty.example.invalid/' },
            ],
          },
        },
      },
    },
    catalog: { value: { projects } },
    kernelBridge: bridge,
    localStorage: {
      getItem: key => storage.get(key),
      setItem: (key, value) => storage.set(key, value),
      removeItem: key => storage.delete(key),
    },
    crypto: { randomUUID: () => 'hierarchy-uuid' },
    TextEncoder,
    available: true,
    browserClient: true,
    navigationOpen,
    t: key => key,
    applyCatalogPreferences: () => {},
    selectRemoteConversation: () => {},
  });
  const script = [
    readSection(chatSessionSrc, 'const creatingKind =', 'const layoutState ='),
    readSection(chatSessionSrc, 'function beginCreateItem(', 'async function cancel(execution)'),
    `globalThis.actions = {
      startCreateProject,
      startCreateConversation,
      creatingKind,
      creatingForProjectId,
      creationDeviceId,
      creationConnectionId,
      creationMode,
      creationWorkspace,
      creationWorkspaces,
      creationAvailableTools,
      creationError,
      selectCreationDevice,
      selectCreationTool,
      cancelCreateItem,
      navigationOpen,
    }`,
  ].join('\n');
  vm.runInContext(script, ctx);
  return ctx.actions;
}

test('create modal cascades Project -> Device -> Available Tools on Device -> Best Workspace', async () => {
  const actions = mountCreateModalState(
    {
      dshList: async (nodeId) => {
        if (nodeId === 'vps2') {
          return [
            { harness: 'codex', workspace: '/srv/alpha' },
            { harness: 'agy', workspace: '/srv/alpha' },
            { harness: 'agy', workspace: '/srv/beta' },
            { source_kind: 'dsh_native', workspace: '/srv/dsh-only' },
          ];
        }
        return [];
      },
    },
    [{ id: 'proj-beta', title: 'Beta', workspace: '/srv/beta' }],
  );

  actions.startCreateConversation('proj-beta');
  assert.equal(actions.creatingKind.value, 'conversation');
  assert.equal(actions.creatingForProjectId.value, 'proj-beta');
  assert.equal(actions.creationDeviceId.value, 'local');
  assert.equal(actions.navigationOpen.value, false);

  await actions.selectCreationDevice('vps2');
  assert.equal(actions.creationDeviceId.value, 'vps2');
  assert.equal(actions.creationConnectionId.value, 'vps2');
  // Auto-selects first available remote tool ('dsh') and exposes availability counts
  assert.equal(actions.creationMode.value, 'dsh');
  const toolMap = Object.fromEntries(actions.creationAvailableTools.value.map(item => [item.id, item]));
  assert.equal(toolMap.dsh.available, true);
  assert.equal(toolMap.dsh.count, 1);
  assert.equal(toolMap.agy.available, true);
  assert.equal(toolMap.agy.count, 2);
  assert.equal(toolMap.claude.available, false);
  assert.equal(toolMap.claude.count, 0);
  assert.equal(toolMap.pi.available, false);

  // Switching to AGY picks workspace matching the selected project's workspace hint
  actions.selectCreationTool('agy');
  assert.equal(actions.creationMode.value, 'agy');
  assert.deepEqual(Array.from(actions.creationWorkspaces.value), ['/srv/alpha', '/srv/beta']);
  assert.equal(actions.creationWorkspace.value, '/srv/beta');

  // Selecting an empty device surfaces explicit unavailable error and clears tool
  await actions.selectCreationDevice('empty-node');
  assert.equal(actions.creationMode.value, '');
  assert.equal(actions.creationError.value, 'createNoToolsOnDevice');
});

test('SidebarCreateForm uses Teleport frosted modal without hardcoded colors or emojis', () => {
  assert.match(modalSrc, /<Teleport to="body">/);
  assert.match(modalSrc, /backdrop-filter:\s*blur\(/);
  assert.doesNotMatch(modalSrc, /#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/);
});

test('DshSessions permits opening and resuming resumable interrupted AGY CLI sessions', () => {
  assert.match(dshSrc, /function canReadOrResume\(/);
  assert.match(dshSrc, /dsh-interrupted-banner/);
  assert.doesNotMatch(dshSrc, /#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/);
});

test('zh-CN.json and en-US.json keep 100% key parity for all creation, quota, and resume keys', () => {
  const zhKeys = Object.keys(zhJson).sort();
  const enKeys = Object.keys(enJson).sort();
  assert.deepEqual(zhKeys, enKeys);
  for (const required of [
    'createStepProjectTitle',
    'createStepDevice',
    'createStepTool',
    'createStepWorkspace',
    'createDetachedDaemonHint',
    'quotaAutoCheckpointSynced',
    'agyCliInterruptedTitle',
    'agyResumeNowBtn',
  ]) {
    assert.ok(zhJson[required], `missing zh-CN key: ${required}`);
    assert.ok(enJson[required], `missing en-US key: ${required}`);
  }
});
