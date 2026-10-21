// Bundle the actual Vue panel and its offline fixture; no server/CDN/token needed.
import { build } from 'vite';
import vue from '@vitejs/plugin-vue';
import { mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const result = await build({
  configFile: false, root, plugins: [vue()],
  define: { 'process.env.NODE_ENV': JSON.stringify('production') },
  build: {
    write: false, minify: true, cssCodeSplit: false, assetsInlineLimit: 10000000,
    lib: { entry: resolve(root, 'tests/fixtures/remote-agent-preview-entry.js'), name: 'BobOfflinePreview', formats: ['iife'] },
  },
});
const output = (Array.isArray(result) ? result : [result]).flatMap(x => x.output);
const js = output.filter(x => x.type === 'chunk').map(x => x.code).join('\n').replace(/<\/script/gi, '<\\/script');
const css = output.filter(x => x.type === 'asset' && x.fileName.endsWith('.css')).map(x => x.source).join('\n');
const html = `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:; connect-src 'none'">
<title>Bob 0.9.29 · 离线交互示例</title><style>${css}
body { margin:0; padding:16px; background:var(--bg-primary); color:var(--text-primary); }
.preview-notice { max-width:1100px; margin:0 auto 16px; padding:16px; border:1px solid var(--border-color); border-radius:12px; background:var(--bg-secondary); }
.preview-notice p { margin:8px 0 0; line-height:1.6; color:var(--text-secondary); }
#app { max-width:1100px; margin:auto; }
</style></head><body><aside class="preview-notice"><strong>Bob 0.9.29 · 离线交互示例 / Offline demo</strong>
<p>这是 Bob 工作界面的真实组件，但全部数据是模拟的，不连接 DSH、不执行任务。输入任意示例网址（例如 https://demo.invalid），点击连接、选择会话，即可试用发送、审批与请求取消。请勿输入真实令牌。</p>
<p>Actual Bob component with simulated data only. No DSH connection or task execution. Use an example URL, connect and select a session. Never enter a real token. Cancellation deliberately remains unconfirmed in this demo.</p></aside><div id="app"></div><script>${js}</script></body></html>`;
const target = resolve(root, 'dist-release/previews/remote-agent-preview.html');
await mkdir(dirname(target), { recursive: true });
await writeFile(target, html, 'utf8');
console.log(`Self-contained offline preview: ${target}`);
