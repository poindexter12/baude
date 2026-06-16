// Minimal service worker: cache the app shell for installability/offline
// shell; never cache API calls (they're live session state). Also receives
// Web Push notifications from the daemon.
const CACHE = "baude-v4";
const SHELL = ["/", "/app.js", "/style.css", "/manifest.webmanifest", "/icon.svg"];

self.addEventListener("install", (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches.keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (e) => {
  const url = new URL(e.request.url);
  const isShell = e.request.method === "GET" && url.origin === location.origin
    && SHELL.includes(url.pathname);
  if (!isShell) return; // API and everything else: straight to the network
  // network-first so app updates roll out; cache covers offline
  e.respondWith(
    fetch(e.request)
      .then((res) => {
        const copy = res.clone();
        caches.open(CACHE).then((c) => c.put(e.request, copy));
        return res;
      })
      .catch(() => caches.match(e.request))
  );
});

self.addEventListener("push", (e) => {
  let d = {};
  try { d = e.data ? e.data.json() : {}; } catch { /* non-JSON push */ }
  e.waitUntil(self.registration.showNotification(d.title || "baude", {
    body: d.body || "",
    tag: d.tag,
    icon: "/icon-192.png",
    badge: "/icon-192.png",
    data: d,
  }));
});

self.addEventListener("notificationclick", (e) => {
  e.notification.close();
  const sid = e.notification.data && e.notification.data.sid;
  const url = sid ? `/#/s/${sid}` : "/";
  e.waitUntil(
    clients.matchAll({ type: "window", includeUncontrolled: true }).then((list) => {
      for (const c of list) {
        if ("focus" in c) {
          c.navigate(url);
          return c.focus();
        }
      }
      return clients.openWindow(url);
    })
  );
});
