// Bob Agent - Web Push & PWA Service Worker
self.addEventListener('install', () => {
  self.skipWaiting();
});

self.addEventListener('activate', event => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener('push', event => {
  let payload = {};
  if (event.data) {
    try {
      payload = event.data.json();
    } catch {
      payload = {
        title: 'Bob',
        body: event.data.text(),
      };
    }
  }

  const title = payload.title || 'Bob · 等待你确认';
  const options = {
    body: payload.body || '有新的高危操作或派单指令等待确认，点击打开处理。',
    icon: payload.icon || '/logos/pi.svg',
    badge: payload.badge || '/logos/pi.svg',
    tag: payload.tag || 'bob-approval-wakeup',
    renotify: true,
    requireInteraction: true,
    data: {
      url: payload.url || '/',
      approvalId: payload.approvalId || null,
      timestamp: Date.now(),
    },
  };

  event.waitUntil(self.registration.showNotification(title, options));
});

self.addEventListener('notificationclick', event => {
  event.notification.close();
  const notifData = event.notification.data || {};
  const targetUrl = notifData.url || '/';

  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then(clientList => {
      for (const client of clientList) {
        if (client.url && 'focus' in client) {
          try {
            client.postMessage({
              type: 'bob:approval-notification-click',
              url: targetUrl,
              approvalId: notifData.approvalId || null,
            });
          } catch {}
          return client.focus();
        }
      }
      if (self.clients.openWindow) {
        return self.clients.openWindow(targetUrl);
      }
    }),
  );
});
