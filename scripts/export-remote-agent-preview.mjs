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
<title>Bob 0.9.30 · 离线交互示例</title><style>${css}
body { margin:0; padding:16px; background:var(--bg-primary); color:var(--text-primary); }
.preview-notice { max-width:1100px; margin:0 auto 16px; padding:16px; border:1px solid var(--border-color); border-radius:12px; background:var(--bg-secondary); }
.preview-notice p { margin:8px 0 0; line-height:1.6; color:var(--text-secondary); }
#app { max-width:1100px; margin:auto; }
</style></head><body><aside class="preview-notice"><strong>Bob 0.9.30 · 离线交互示例 / Offline demo</strong>
<p>真实设置与工作组件，全部数据为模拟。点击“使用系统解锁”（本页仅模拟，不调用设备认证），再填写名称与示例网址（https://demo.invalid），保存后点击 work 选择会话。请勿输入真实密码或令牌；此页不保存凭据、不连接 DSH。</p>
<p>Simulated data only. Click Unlock with system (simulated here, not device authentication), enter a name and example URL, save, then choose work. Never enter real credentials. Cancellation remains unconfirmed in this demo.</p></aside><div id="app"></div><script>${js}</script></body></html>`;
const target = resolve(root, 'dist-release/previews/remote-agent-preview.html');
await mkdir(dirname(target), { recursive: true });
await writeFile(target, html, 'utf8');
console.log(`Self-contained offline preview: ${target}`);
