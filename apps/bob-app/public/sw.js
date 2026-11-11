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

  const title = payload.title || 'Bob';
  const options = {
    body: payload.body || '',
    icon: payload.icon || '/logos/pi.svg',
    badge: payload.badge || '/logos/pi.svg',
    data: {
      url: payload.url || '/',
      timestamp: Date.now(),
    },
  };

  event.waitUntil(self.registration.showNotification(title, options));
});

self.addEventListener('notificationclick', event => {
  event.notification.close();
  const targetUrl = event.notification.data?.url || '/';

  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then(clientList => {
      for (const client of clientList) {
        if (client.url && 'focus' in client) {
          return client.focus();
        }
      }
      if (self.clients.openWindow) {
        return self.clients.openWindow(targetUrl);
      }
    }),
  );
});
