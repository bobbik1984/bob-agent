import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';
import { partitionNativeActivity } from '../src/native-activity.mjs';
import {
  enableApprovalNotifications,
  getApprovalNotificationState,
  notifyPendingApproval,
  resetNotifiedApprovalsForTest,
  sendTestApprovalNotification,
} from '../src/approval-notifications.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(__dirname, '..');

test('partitionNativeActivity separates actionable approvals from execution logs and source-only entries', () => {
  const now = 1_000_000;
  const activity = {
    items: [
      { id: 'cmd-1', label: '/bin/bash -lc ls', status: 'completed' },
      { id: 'cmd-2', label: '/bin/bash -lc rg ...', status: 'running' },
    ],
    approvals: [
      { id: 'ext-1', status: 'pending', actionable: false },
      { id: 'expired-1', status: 'pending', actionable: true, permission_id: 'p-exp', expires_at: now - 10 },
      { id: 'live-1', status: 'pending', actionable: true, permission_id: 'p-live', expires_at: now + 60_000, title: 'Bash' },
    ],
  };

  const parts = partitionNativeActivity(activity, now);
  assert.equal(parts.actionableApprovals.length, 1);
  assert.equal(parts.actionableApprovals[0].permission_id, 'p-live');
  assert.equal(parts.logItems.length, 2);
  assert.equal(parts.historyApprovals.length, 2);
  assert.equal(parts.logCount, 4);
});

test('NativeActivity.vue keeps execution logs collapsed by default and places confirmation cards outside details', () => {
  const src = fs.readFileSync(path.join(appRoot, 'src', 'components', 'NativeActivity.vue'), 'utf8');
  assert.doesNotMatch(src, /<details[^>]*:open=/, 'execution logs must stay collapsed by default');
  const detailsClose = src.indexOf('</details>');
  const standaloneIdx = src.indexOf('native-approval-standalone-card');
  assert.ok(detailsClose !== -1 && standaloneIdx !== -1 && standaloneIdx > detailsClose);
});

test('Zero hardcoded colors, zero emojis, and locale key parity across touched UI files', () => {
  const emojiRegex = /[\u{1F000}-\u{1FAFF}]/u;
  const hexColorRegex = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/;
  const rgbColorRegex = /\brgba?\s*\(/i;

  for (const rel of [
    'src/components/NativeActivity.vue',
    'src/components/DshSessions.vue',
    'src/views/SettingsView.vue',
    'src/approval-notifications.mjs',
  ]) {
    const content = fs.readFileSync(path.join(appRoot, rel), 'utf8');
    assert.equal(emojiRegex.test(content), false, `${rel} must not contain emojis`);
    assert.equal(hexColorRegex.test(content), false, `${rel} must not contain hardcoded hex colors`);
    assert.equal(rgbColorRegex.test(content), false, `${rel} must not contain hardcoded rgb/rgba colors`);
  }

  const zh = JSON.parse(fs.readFileSync(path.join(appRoot, 'src', 'locales', 'zh-CN.json'), 'utf8'));
  const en = JSON.parse(fs.readFileSync(path.join(appRoot, 'src', 'locales', 'en-US.json'), 'utf8'));
  assert.deepEqual(Object.keys(zh).sort(), Object.keys(en).sort());
  for (const key of [
    'permissionModeWorkspaceAuto',
    'nativeApprovalBannerTitle',
    'nativeApprovalRemainingMin',
    'approvalNotifyCardTitle',
    'approvalNotifyEnableBtn',
    'approvalNotifyTestBtn',
    'approvalNotifyIosPwaHint',
  ]) {
    assert.ok(zh[key] && en[key], `missing locale key ${key}`);
    assert.equal(emojiRegex.test(zh[key] + en[key]), false, `emoji disallowed in locale key ${key}`);
  }
});

test('approval-notifications deduplicates permission alerts, handles SW push subscription, and detects iOS Home Screen requirement', async () => {
  resetNotifiedApprovalsForTest();
  const shown = [];
  let subscribedPayload = null;
  let pushTestCalled = 0;

  const iosEnv = {
    Notification: { permission: 'default', requestPermission: async () => 'granted' },
    navigator: { userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X)', maxTouchPoints: 5 },
    window: { matchMedia: () => ({ matches: false }) },
  };
  const iosState = getApprovalNotificationState(iosEnv);
  assert.equal(iosState.isIosNeedsHomeScreen, true);

  const swReg = {
    pushManager: {
      getSubscription: async () => null,
      subscribe: async () => ({ toJSON: () => ({ endpoint: 'https://fcm.googleapis.com/fcm/send/demo', keys: { p256dh: 'a', auth: 'b' } }) }),
    },
    showNotification: async (title, opts) => {
      shown.push({ title, opts });
    },
  };
  const mockNav = {
    userAgent: 'Mozilla/5.0 (Linux; Android 14)',
    serviceWorker: {
      register: async () => swReg,
      getRegistration: async () => swReg,
    },
  };
  const bridge = {
    pushVapidPublicKey: async () => ({ publicKey: Buffer.from(new Uint8Array(65).fill(4)).toString('base64url') }),
    pushSubscribe: async sub => {
      subscribedPayload = sub;
      return { ok: true };
    },
    pushTest: async () => {
      pushTestCalled += 1;
      return { ok: true };
    },
  };

  const env = {
    Notification: { permission: 'default', requestPermission: async () => 'granted' },
    navigator: mockNav,
    window: { matchMedia: () => ({ matches: false }) },
  };
  const enabled = await enableApprovalNotifications(bridge, env);
  assert.equal(enabled.ok, true);
  assert.equal(enabled.subscribed, true);
  assert.equal(subscribedPayload.endpoint, 'https://fcm.googleapis.com/fcm/send/demo');

  const grantedEnv = {
    Notification: { permission: 'granted', requestPermission: async () => 'granted' },
    navigator: mockNav,
    window: { matchMedia: () => ({ matches: false }) },
  };
  assert.equal(await notifyPendingApproval({ id: 'perm-1', title: 'Run git push', body: '123' }, {}, grantedEnv), true);
  assert.equal(await notifyPendingApproval({ id: 'perm-1', title: 'Run git push', body: '123' }, {}, grantedEnv), false);
  assert.equal(await sendTestApprovalNotification(bridge, { title: 'Test', body: 'OK' }, grantedEnv), true);
  assert.equal(shown.length, 2);
  assert.equal(pushTestCalled, 1);

  const swSrc = fs.readFileSync(path.join(appRoot, 'public', 'sw.js'), 'utf8');
  assert.match(swSrc, /Bob · 等待你确认/);
  assert.match(swSrc, /bob:approval-notification-click/);
});

test('DshSessions supports workspace_auto mode, persists selection to localStorage, and surfaces pendingNativeApprovals', async () => {
  const vue = fs.readFileSync(path.join(appRoot, 'src', 'components', 'DshSessions.vue'), 'utf8');
  const script = vue.split('<script setup>')[1].split('</script>')[0].replace(/^\s*import .*;\r?\n/gm, '');
  const store = new Map([['bob.dsh.approvalMode', 'workspace_auto']]);
  let now = 5_000_000;
  const ctx = {
    kernelBridge: {
      dshPage: async (c, s) => ({ address: { connection_id: c, session_id: s }, messages: [], cursor: 1, before: 0, has_more: false }),
      nativeEvents: async () => ({ events: [], snapshots: [], cursor: 0, epoch: '' }),
      dshLive: async () => ({ active: false }),
    },
    ref: v => ({ value: v }),
    computed: f => ({ get value() { return f(); } }),
    watch: () => {},
    onUnmounted: () => {},
    nextTick: async () => {},
    provide: () => {},
    defineProps: () => ({ nodes: [], target: null }),
    useI18n: () => ({ t: k => k }),
    localStorage: { getItem: k => store.get(k) || null, setItem: (k, v) => store.set(k, String(v)) },
    crypto: { randomUUID: () => 'req-1' },
    setTimeout: () => 0,
    clearTimeout: () => {},
    Date: Object.assign(function (...a) { return new Date(...a); }, { now: () => now, parse: Date.parse }),
    artifactPath: x => x,
    isNotAcceptedReceipt: () => false,
    unconfirmActivity: a => a,
    settleNativeActivity: a => a,
    orderedNativeActivities: drafts => Object.entries(drafts).filter(([, v]) => v.activity).map(([id, v]) => ({ id, activity: v.activity })),
    waitingNativeApproval: () => false,
    createChatScroll: () => ({ reset() {}, binding: () => {}, following: true }),
    readingCache: { get: async () => null, set: async () => {}, put: async () => {} },
    readingKey: (c, s) => `${c}:${s}`,
    connectionPrefix: c => c,
    isTestOrScratchSession: () => false,
    createPersistentPreviewCache: () => ({ get: () => null, set: () => {}, clear: () => {} }),
    Error, Map, Set, Array, Object, String, Boolean, Number, Math, JSON, Promise, RegExp,
  };
  vm.runInContext(`${script}\nglobalThis.__dsh = { approvalMode, permissionModeOptions, nativeDrafts, pendingNativeApprovals, reader, scrollToNativeApproval };`, vm.createContext(ctx));
  assert.equal(ctx.__dsh.approvalMode.value, 'workspace_auto');
  assert.ok(ctx.__dsh.permissionModeOptions.value.some(o => o.value === 'workspace_auto'));
  ctx.__dsh.nativeDrafts.value = {
    'req-a': {
      timestamp: now,
      activity: {
        items: [{ id: 'i1', label: 'git status', status: 'completed' }],
        approvals: [{ id: 'a1', status: 'pending', actionable: true, permission_id: 'perm-sticky-1', expires_at: now + 120_000, title: 'Bash' }],
      },
    },
  };
  assert.equal(ctx.__dsh.pendingNativeApprovals.value.length, 1);
  assert.equal(ctx.__dsh.pendingNativeApprovals.value[0].approval.permission_id, 'perm-sticky-1');
});
