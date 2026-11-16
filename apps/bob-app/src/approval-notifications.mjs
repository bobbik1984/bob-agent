const notifiedApprovalIds = new Set();

function rememberNotifiedId(key) {
  if (notifiedApprovalIds.has(key)) return false;
  if (notifiedApprovalIds.size >= 256) {
    const first = notifiedApprovalIds.values().next().value;
    if (first !== undefined) notifiedApprovalIds.delete(first);
  }
  notifiedApprovalIds.add(key);
  return true;
}

function detectIosMobile(nav = globalThis.navigator) {
  const ua = String(nav?.userAgent || '');
  if (/iPad|iPhone|iPod/.test(ua)) return true;
  if (/Macintosh/.test(ua) && Number(nav?.maxTouchPoints || 0) > 1) return true;
  return false;
}

function detectStandalonePwa(win = globalThis.window, nav = globalThis.navigator) {
  if (nav?.standalone === true) return true;
  try {
    return Boolean(win?.matchMedia?.('(display-mode: standalone)')?.matches);
  } catch {
    return false;
  }
}

export function getApprovalNotificationState(env = {}) {
  const NotificationApi = env.Notification ?? globalThis.Notification;
  const nav = env.navigator ?? globalThis.navigator;
  const win = env.window ?? globalThis.window;
  const supported =
    typeof NotificationApi !== 'undefined' &&
    typeof NotificationApi?.requestPermission === 'function';
  const permission = supported ? String(NotificationApi.permission || 'default') : 'unsupported';
  const isIos = detectIosMobile(nav);
  const isStandalone = detectStandalonePwa(win, nav);
  return {
    supported,
    permission,
    isIos,
    isStandalone,
    isIosNeedsHomeScreen: isIos && !isStandalone,
  };
}

let swRegisterPromise = null;

export async function registerApprovalServiceWorker(nav = globalThis.navigator) {
  if (!nav?.serviceWorker || typeof nav.serviceWorker.register !== 'function') {
    return null;
  }
  if (!swRegisterPromise) {
    swRegisterPromise = nav.serviceWorker
      .register('/sw.js')
      .catch(() => null);
  }
  return swRegisterPromise;
}

export function urlBase64ToUint8Array(base64String) {
  const clean = String(base64String || '').trim();
  const padding = '='.repeat((4 - (clean.length % 4)) % 4);
  const base64 = (clean + padding).replace(/-/g, '+').replace(/_/g, '/');
  const raw = globalThis.atob(base64);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i += 1) {
    out[i] = raw.charCodeAt(i);
  }
  return out;
}

export async function subscribeWebPushIfAvailable(kernelBridge, reg, env = {}) {
  const nav = env.navigator ?? globalThis.navigator;
  const registration =
    reg ||
    (await registerApprovalServiceWorker(nav)) ||
    (await nav?.serviceWorker?.getRegistration?.('/sw.js').catch?.(() => null));
  if (!registration?.pushManager || !kernelBridge?.pushVapidPublicKey || !kernelBridge?.pushSubscribe) {
    return false;
  }
  try {
    const existing = await registration.pushManager.getSubscription();
    if (existing) {
      await kernelBridge.pushSubscribe(typeof existing.toJSON === 'function' ? existing.toJSON() : existing);
      return true;
    }
    const keyRes = await kernelBridge.pushVapidPublicKey();
    const publicKey = keyRes?.publicKey || keyRes?.public_key;
    if (!publicKey) return false;
    const sub = await registration.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: urlBase64ToUint8Array(publicKey),
    });
    await kernelBridge.pushSubscribe(typeof sub.toJSON === 'function' ? sub.toJSON() : sub);
    return true;
  } catch {
    return false;
  }
}

export async function enableApprovalNotifications(kernelBridge, env = {}) {
  const NotificationApi = env.Notification ?? globalThis.Notification;
  const nav = env.navigator ?? globalThis.navigator;
  const initialState = getApprovalNotificationState(env);
  if (!initialState.supported) {
    return { ok: false, subscribed: false, state: initialState };
  }
  let permission = initialState.permission;
  if (permission === 'default') {
    try {
      permission = await NotificationApi.requestPermission();
    } catch {
      permission = 'denied';
    }
  }
  const nextState = getApprovalNotificationState({
    ...env,
    Notification: { ...NotificationApi, permission, requestPermission: NotificationApi.requestPermission },
  });
  if (permission !== 'granted') {
    return { ok: false, subscribed: false, state: nextState };
  }
  const reg = await registerApprovalServiceWorker(nav);
  const subscribed = await subscribeWebPushIfAvailable(kernelBridge, reg, env);
  return { ok: true, subscribed, state: nextState };
}

export async function notifyPendingApproval(item, opts = {}, env = {}) {
  const id = String(item?.id || '').trim();
  if (!id) return false;
  const NotificationApi = env.Notification ?? globalThis.Notification;
  if (!NotificationApi || NotificationApi.permission !== 'granted') {
    return false;
  }
  if (!opts?.force && !rememberNotifiedId(id)) {
    return false;
  }

  const title = String(item?.title || 'Bob');
  const body = String(item?.body || '');
  const tag = String(item?.tag || `bob-approval-${id}`);
  const payload = {
    body,
    icon: '/logos/pi.svg',
    badge: '/logos/pi.svg',
    tag,
    renotify: true,
    requireInteraction: item?.requireInteraction !== false,
    data: {
      url: String(item?.url || '/'),
      approvalId: id,
      timestamp: Date.now(),
    },
  };

  const nav = env.navigator ?? globalThis.navigator;
  try {
    const reg =
      (await nav?.serviceWorker?.getRegistration?.('/sw.js').catch?.(() => null)) ||
      (await registerApprovalServiceWorker(nav));
    if (reg && typeof reg.showNotification === 'function') {
      await reg.showNotification(title, payload);
      return true;
    }
  } catch {}

  if (typeof NotificationApi === 'function') {
    try {
      new NotificationApi(title, payload);
      return true;
    } catch {}
  }
  return false;
}

export async function sendTestApprovalNotification(kernelBridge, copy = {}, env = {}) {
  const ok = await notifyPendingApproval(
    {
      id: `test-${Date.now()}`,
      title: copy.title || 'Bob',
      body: copy.body || '',
      tag: 'bob-approval-test',
      requireInteraction: false,
    },
    { force: true },
    env,
  );
  if (typeof kernelBridge?.pushTest === 'function') {
    await kernelBridge.pushTest().catch(() => null);
  }
  return ok;
}

export function resetNotifiedApprovalsForTest() {
  notifiedApprovalIds.clear();
}

globalThis.__bobApprovalNotifications = {
  registerApprovalServiceWorker,
  getApprovalNotificationState,
  enableApprovalNotifications,
  notifyPendingApproval,
  sendTestApprovalNotification,
};
