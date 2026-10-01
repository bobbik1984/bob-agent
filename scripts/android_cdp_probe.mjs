// Read-only startup probe for an ADB-forwarded Android WebView debugger.
const browserMode = process.argv.includes('--browser');
const listing = await (await fetch(browserMode ? 'http://127.0.0.1:9222/json/version' : 'http://127.0.0.1:9222/json')).json();
const target = browserMode ? listing : listing.find((item) => item.type === 'page');
if (!target) throw new Error('No WebView target');

const ws = new WebSocket(target.webSocketDebuggerUrl);
const timeout = setTimeout(() => {
  console.error('CDP request timed out');
  ws.close();
  process.exitCode = 1;
}, 8000);

ws.addEventListener('open', () => {
  console.log('CDP connected');
  if (browserMode) {
    ws.send(JSON.stringify({ id: 1, method: 'Browser.getVersion' }));
    return;
  }
  ws.send(JSON.stringify({ id: 2, method: 'Page.enable' }));
  ws.send(JSON.stringify({ id: 3, method: 'Runtime.enable' }));
  ws.send(JSON.stringify({
    id: 1,
    method: 'Runtime.evaluate',
    params: {
      expression: `JSON.stringify({
        readyState: document.readyState,
        splashPresent: !!document.getElementById('native-splash'),
        appChildren: document.getElementById('app')?.childElementCount,
        diagnosticFunction: typeof window.__bobBootDiag,
        scripts: [...document.scripts].map((node) => new URL(node.src).pathname).filter(Boolean),
        resourceCount: performance.getEntriesByType('resource').length
      })`,
      returnByValue: true,
    },
  }));
});

ws.addEventListener('message', (event) => {
  const data = JSON.parse(event.data);
  if (data.id !== 1) {
    if (data.id) console.log(`CDP method ${data.id} replied`, JSON.stringify(data.error ?? {}));
    return;
  }
  clearTimeout(timeout);
  console.log(data.result?.result?.value ?? JSON.stringify(data.result));
  ws.close();
});
ws.addEventListener('error', (event) => {
  clearTimeout(timeout);
  console.error('CDP WebSocket failed:', event.message ?? 'unknown');
  process.exitCode = 1;
});
