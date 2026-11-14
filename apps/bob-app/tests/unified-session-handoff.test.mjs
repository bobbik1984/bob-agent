import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(__dirname, '..');
const dshSessionsPath = path.join(appRoot, 'src', 'components', 'DshSessions.vue');
const kernelBridgePath = path.join(appRoot, 'src', 'kernel-bridge.mjs');
const zhLocalePath = path.join(appRoot, 'src', 'locales', 'zh-CN.json');
const enLocalePath = path.join(appRoot, 'src', 'locales', 'en-US.json');

const HANDOFF_I18N_KEYS = [
  'codexAppServerBusyBadge',
  'codexTerminalBusyBadge',
  'agyTerminalBusyBadge',
  'codexHotTakeoverConfirmTitle',
  'codexHotTakeoverConfirmDesc',
  'codexTakeoverConfirmTitle',
  'codexTakeoverConfirmDesc',
  'agyTakeoverConfirmTitle',
  'agyTakeoverConfirmDesc',
  'agyIdeProtectedTitle',
  'agyIdeProtectedDesc',
  'bobHolderActiveBadge',
  'bobHolderReleaseBtn',
  'bobHolderReleasedNotice',
  'bobHolderBusyNotice',
  'terminalHandoffBtn',
  'terminalHandoffCardTitle',
  'terminalHandoffCardDesc',
  'copyTerminalCmdBtn',
];

function extractBlock(source, tag) {
  const m = source.match(new RegExp(`<${tag}(?:\\s[^>]*)?>([\\s\\S]*?)<\\/${tag}>`));
  assert.ok(m, `expected <${tag}> block in DshSessions.vue`);
  return m[1];
}

function makeReactivity() {
  const ref = (v) => ({ value: v });
  const computed = (fn) => ({ get value() { return fn(); } });
  return { ref, computed, watch: () => {}, onUnmounted: () => {}, nextTick: async () => {}, provide: () => {} };
}

function createStorage() {
  const map = new Map();
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k),
    key: (i) => Array.from(map.keys())[i] ?? null,
    get length() { return map.size; },
  };
}

function mountDshSessions({ bridgeOverrides = {}, zhMessages } = {}) {
  const vue = fs.readFileSync(dshSessionsPath, 'utf8');
  let script = extractBlock(vue, 'script');
  script = script.replace(/^\s*import .*;\r?\n/gm, '');
  script += `
;globalThis.__exports = {
  connection, selected, history, draft, sending, notice, error,
  approvalMode, takeoverPrompt, terminalHolderOverride, bobHolderOverride,
  handoffCardOpen, releasingHolder, copiedCliCmd,
  activeTerminalHolder, isRemoteProtected, isClaudeRemoteControl,
  isExternalHolderBusy, isClaudeTerminalBusy, activeBobHolder,
  isBobBackgroundIdleHeld, holderBusyBadgeKey, remoteProtectedTitleKey,
  remoteProtectedDescKey, activeTakeoverKeys, supportsApprovalMode,
  supportsTerminalHandoff, cliResumeCommand,
  open, back, refresh, send, confirmTakeoverSend, handoffToTerminal, copyCliResumeCommand,
};
`;

  const calls = [];
  const copied = [];
  const kernelBridge = {
    async listConnections() { return { connections: [{ id: 'local' }] }; },
    async dshSessions() { return { sessions: [] }; },
    async dshPage(c, s) {
      calls.push(['dshPage', c, s]);
      return {
        address: { connection_id: c, session_id: s },
        cursor: 1,
        before: 0,
        messages: [],
        history_snapshot: true,
        workspace: '/repo/app',
        terminal_holder: { busy: false, remote_control: false },
        bob_holder: { held: false, running: false, kind: null, pid: null },
      };
    },
    async dshPrompt(c, s, text, rid, atts, opts) {
      calls.push(['dshPrompt', c, s, text, rid, opts]);
      return { status: 'accepted', session_id: s, request_id: rid };
    },
    async nativeEvents() { return { events: [], snapshots: [], cursor: 0, epoch: '' }; },
    async dshLive() { return { active: false }; },
    async nativeCancel(c, s, rid) {
      calls.push(['nativeCancel', c, s, rid]);
      return { status: 'released', released: true };
    },
    async nativeRelease(c, s) {
      calls.push(['nativeRelease', c, s]);
      return { status: 'released', released: true };
    },
    ...bridgeOverrides,
  };

  const { ref, computed, watch, onUnmounted, nextTick, provide } = makeReactivity();
  const localeMsgs = zhMessages || JSON.parse(fs.readFileSync(zhLocalePath, 'utf8'));
  const t = (k, p) => {
    let s = localeMsgs[k] ?? k;
    if (p) for (const [pk, pv] of Object.entries(p)) s = s.replace(`{${pk}}`, String(pv));
    return s;
  };
  let timers = 0;
  const ctx = {
    kernelBridge,
    ref, computed, watch, onUnmounted, nextTick, provide,
    useI18n: () => ({ t }),
    defineProps: () => ({ nodes: [{ id: 'local', name: 'local', enabled: true }], target: null }),
    defineEmits: () => () => {},
    localStorage: createStorage(),
    navigator: { clipboard: { writeText: async (txt) => { copied.push(txt); } } },
    crypto: { randomUUID: () => `req-${++timers}` },
    setTimeout: (fn) => { return ++timers; },
    clearTimeout: () => {},
    artifactPath: (x) => x,
    isNotAcceptedReceipt: (r, s, rid) => !r || (r.status !== 'accepted' && r.status !== undefined),
    unconfirmActivity: (a) => a,
    settleNativeActivity: (a) => a,
    orderedNativeActivities: () => [],
    waitingNativeApproval: () => null,
    createChatScroll: () => ({ reset() {}, onScroll() {}, restore() {}, atBottom: ref(true) }),
    readingCache: { get: async () => null, set: async () => {}, put: async () => {} },
    readingKey: (c, s) => `${c}:${s}`,
    connectionPrefix: (c) => `conv-${c}:`,
    isTestOrScratchSession: () => false,
    createPersistentPreviewCache: () => ({ get: () => null, set: () => {}, clear: () => {} }),
    Error, Map, Set, Array, Object, String, Boolean, Number, Math, JSON, Date, Promise, RegExp,
  };
  vm.runInContext(script, vm.createContext(ctx));
  ctx.__exports.connection.value = 'local';
  return { state: ctx.__exports, calls, copied, t };
}

test('locale parity and CSS/template hygiene for unified handoff', () => {
  const zh = JSON.parse(fs.readFileSync(zhLocalePath, 'utf8'));
  const en = JSON.parse(fs.readFileSync(enLocalePath, 'utf8'));
  assert.deepEqual(Object.keys(zh).sort(), Object.keys(en).sort(), 'zh-CN and en-US must share identical keys');
  for (const key of HANDOFF_I18N_KEYS) {
    assert.ok(typeof zh[key] === 'string' && zh[key].trim().length > 0, `zh missing ${key}`);
    assert.ok(typeof en[key] === 'string' && en[key].trim().length > 0, `en missing ${key}`);
    assert.equal(/[\u{1F300}-\u{1FAFF}]/u.test(zh[key] + en[key]), false, `no emoji allowed in ${key}`);
  }

  const vue = fs.readFileSync(dshSessionsPath, 'utf8');
  const style = extractBlock(vue, 'style');
  assert.equal(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/i.test(style), false, 'DshSessions.vue styles must use CSS variables only');
  const selectedMatch = vue.match(/<template v-if="selected">[\s\S]*?<\/template>/);
  assert.ok(selectedMatch && !selectedMatch[0].includes('@click="refresh"'), 'selected template must not add @click="refresh"');

  const kb = fs.readFileSync(kernelBridgePath, 'utf8');
  assert.match(kb, /nativeRelease:[\s\S]*app_native_cancel[\s\S]*__bob_release_holder__/);
});

test('codex app_server holder opens hot-interrupt confirmation and sends takeover=true with approvalMode', async () => {
  const { state, calls } = mountDshSessions({
    bridgeOverrides: {
      async dshPage(c, s) {
        return {
          address: { connection_id: c, session_id: s },
          cursor: 1, before: 0, messages: [], history_snapshot: true,
          workspace: '/repo/codex-proj',
          terminal_holder: {
            busy: true, pid: 4321, harness: 'codex', mode: 'codex_app_server',
            can_hot_interrupt: true, remote_control: false,
          },
          bob_holder: { held: false, running: false },
        };
      },
    },
  });
  await state.open({ id: 'codex:thread-123', title: 'Codex Session', harness: 'codex' });
  assert.equal(state.isExternalHolderBusy.value, true);
  assert.equal(state.isRemoteProtected.value, false);
  assert.equal(state.supportsApprovalMode.value, true);
  assert.equal(state.holderBusyBadgeKey.value, 'codexAppServerBusyBadge');

  state.draft.value = 'Please inspect tests';
  await state.send();
  assert.equal(calls.filter((c) => c[0] === 'dshPrompt').length, 0);
  assert.equal(state.activeTakeoverKeys.value.title, 'codexHotTakeoverConfirmTitle');

  state.confirmTakeoverSend();
  await new Promise((r) => setImmediate(r));
  const promptCalls = calls.filter((c) => c[0] === 'dshPrompt');
  assert.equal(promptCalls.length, 1);
  assert.equal(promptCalls[0][3], 'Please inspect tests');
  assert.equal(promptCalls[0][5].takeover, true);
  assert.equal(promptCalls[0][5].approvalMode, 'interactive');
});

test('agy CLI holder supports empty-draft pill takeover and AGY IDE enforces read-only protection', async () => {
  const cliHost = mountDshSessions({
    bridgeOverrides: {
      async dshPage(c, s) {
        return {
          address: { connection_id: c, session_id: s },
          cursor: 1, before: 0, messages: [], history_snapshot: true,
          workspace: '/repo/agy-cli',
          terminal_holder: { busy: true, pid: 7002, harness: 'agy', mode: 'agy_terminal', remote_control: false },
          bob_holder: { held: false, running: false },
        };
      },
    },
  });
  await cliHost.state.open({ id: 'agy:sess-cli', title: 'AGY CLI', harness: 'agy' });
  assert.equal(cliHost.state.holderBusyBadgeKey.value, 'agyTerminalBusyBadge');
  cliHost.state.takeoverPrompt.value = cliHost.state.activeTerminalHolder.value;
  assert.equal(cliHost.state.activeTakeoverKeys.value.title, 'agyTakeoverConfirmTitle');
  cliHost.state.confirmTakeoverSend();
  await new Promise((r) => setImmediate(r));
  const cliPrompts = cliHost.calls.filter((c) => c[0] === 'dshPrompt');
  assert.equal(cliPrompts.length, 1);
  assert.equal(cliPrompts[0][3], cliHost.t('agyResumeDefaultPrompt'));
  assert.equal(cliPrompts[0][5].takeover, true);

  const ideHost = mountDshSessions({
    bridgeOverrides: {
      async dshPage(c, s) {
        return {
          address: { connection_id: c, session_id: s },
          cursor: 1, before: 0, messages: [], history_snapshot: true,
          workspace: '/repo/agy-ide',
          terminal_holder: { busy: true, pid: 7001, harness: 'agy', mode: 'agy_ide', remote_control: true },
          bob_holder: { held: false, running: false },
        };
      },
    },
  });
  await ideHost.state.open({ id: 'agy:sess-ide', title: 'AGY IDE', harness: 'agy' });
  assert.equal(ideHost.state.isRemoteProtected.value, true);
  assert.equal(ideHost.state.isExternalHolderBusy.value, false);
  assert.equal(ideHost.state.remoteProtectedTitleKey.value, 'agyIdeProtectedTitle');
  ideHost.state.draft.value = 'Should be blocked';
  await ideHost.state.send();
  assert.equal(ideHost.calls.filter((c) => c[0] === 'dshPrompt').length, 0);
  assert.equal(ideHost.state.notice.value, ideHost.t('agyIdeProtectedTitle'));
});

test('reverse handoff releases idle bob_holder, builds CLI resume command, copies to clipboard, and blocks when busy', async () => {
  const { state, calls, copied, t } = mountDshSessions({
    bridgeOverrides: {
      async dshPage(c, s) {
        return {
          address: { connection_id: c, session_id: s },
          cursor: 1, before: 0, messages: [], history_snapshot: true,
          workspace: '/repo/my app',
          terminal_holder: { busy: false, remote_control: false },
          bob_holder: { held: true, running: false, kind: 'codex_observer', pid: 5555 },
        };
      },
    },
  });
  await state.open({ id: 'codex:thread-abc', title: 'Codex Handoff', harness: 'codex' });
  assert.equal(state.isBobBackgroundIdleHeld.value, true);
  assert.equal(state.cliResumeCommand.value, "cd '/repo/my app' && codex resume thread-abc");

  await state.handoffToTerminal();
  assert.deepEqual(calls.filter((c) => c[0] === 'nativeRelease'), [['nativeRelease', 'local', 'codex:thread-abc']]);
  assert.equal(state.isBobBackgroundIdleHeld.value, false);
  assert.equal(state.handoffCardOpen.value, true);
  assert.equal(state.notice.value, t('bobHolderReleasedNotice'));

  await state.copyCliResumeCommand();
  assert.deepEqual(copied, ["cd '/repo/my app' && codex resume thread-abc"]);
  assert.equal(state.copiedCliCmd.value, true);

  // Busy turn prevents reverse release without contacting host
  state.bobHolderOverride.value = { held: true, running: true, kind: 'native_worker', pid: 6666 };
  await state.handoffToTerminal();
  assert.equal(calls.filter((c) => c[0] === 'nativeRelease').length, 1);
  assert.equal(state.notice.value, t('bobHolderBusyNotice'));

  // Claude & AGY CLI command formatting
  await state.open({ id: 'claude-jsonl:11112222-3333', title: 'Claude', harness: 'claude', workspace: '/repo/claude' });
  assert.equal(state.cliResumeCommand.value, 'cd /repo/claude && claude --resume 11112222-3333');
  await state.open({ id: 'agy:agy-id', title: 'AGY', harness: 'agy', workspace: '/repo/agy' });
  assert.equal(state.cliResumeCommand.value, 'cd /repo/agy && agy');
});
