// Uses an already-installed Playwright; never installs dependencies or reaches a remote Agent.
// BOB_BROWSER_MODULE may point to the bundled playwright/index.mjs on this development host.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const { chromium } = await import(process.env.BOB_BROWSER_MODULE ? pathToFileURL(process.env.BOB_BROWSER_MODULE).href : 'playwright');
const base = process.env.BOB_UI_TEST_URL || 'http://127.0.0.1:5178';
const offline = process.env.BOB_UI_OFFLINE === '1';
if (!offline) assert.equal(new URL(base).hostname, '127.0.0.1');
const output = join(tmpdir(), 'bob-remote-agent-ui');
await mkdir(output, { recursive: true });
const browser = await chromium.launch({ channel: 'msedge', headless: true });
try {
  for (const [name, width, height, lang] of [['mobile', 390, 844, 'zh-CN'], ['desktop', 1280, 900, 'en-US']]) {
    const page = await browser.newPage({ viewport: { width, height } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    const networkRequests = [];
    page.on('request', request => { if (/^https?:/.test(request.url())) networkRequests.push(request.url()); });
    await page.route('**/*', route => {
      const url = new URL(route.request().url());
      return (offline ? url.protocol === 'file:' : url.origin === base) ? route.continue() : route.abort();
    });
    const preview = new URL('../dist-release/previews/remote-agent-preview.html', import.meta.url);
    preview.searchParams.set('lang', lang);
    await page.goto(offline ? preview.href : `${base}/tests/fixtures/remote-agent-preview.html?lang=${lang}`);
    await page.locator('input[type=url]').fill('https://fixture.invalid/?token=fixture-only');
    await page.locator('button[type=submit]').click();
    await page.getByRole('button', { name: '建筑模型渲染 · 示例' }).click();
    await page.locator('.remote-message').last().waitFor();
    assert.equal(await page.locator('.remote-transcript img').count(), 0);
    assert.equal(await page.evaluate(() => Boolean(window.__unsafe)), false);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    assert.equal(await page.locator('.remote-transcript').evaluate(el => el.scrollWidth <= el.clientWidth), true);
    await page.locator('textarea').fill('Render a sample');
    await page.locator('.remote-composer button[type=submit]').click();
    await page.getByText('实时文字 · Live text', { exact: true }).waitFor();
    assert.equal(await page.locator('.remote-transcript details').getAttribute('open'), null);
    await page.getByRole('button', { name: lang === 'zh-CN' ? '仅允许本次' : 'Allow once', exact: true }).click();
    await page.getByText(lang === 'zh-CN' ? '决定已送达 DSH' : 'Decision delivered to DSH', { exact: true }).waitFor();
    await page.getByRole('button', { name: lang === 'zh-CN' ? '请求停止当前轮次' : 'Request stop', exact: true }).click();
    await page.getByText(lang === 'zh-CN' ? '已请求取消，等待 DSH 终止事件' : 'Cancellation requested; awaiting DSH termination event', { exact: true }).waitFor();
    assert.equal(await page.locator('.remote-composer button[type=submit]').isDisabled(), true);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    await page.screenshot({ path: join(output, `${name}.png`), fullPage: true });
    await page.getByRole('button', { name: lang === 'zh-CN' ? '断开本次连接' : 'Disconnect session' }).click();
    await page.locator('input[type=password]').waitFor();
    assert.equal(await page.locator('input[type=password]').inputValue(), '');
    assert.equal((await page.locator('input[type=url]').inputValue()).includes('token'), false);
    assert.deepEqual(await page.evaluate(() => window.__remoteFixtureCalls), [['send', 'watch'], ['answer', 'watch'], ['cancel', 'watch'], ['disconnect', 'fixture']]);
    assert.deepEqual(errors, []);
    if (offline) assert.deepEqual(networkRequests, []);
    await page.close();
  }
  console.log(`Remote Agent UI: mobile + desktop passed; simulated data only. Screenshots: ${output}`);
} finally { await browser.close(); }
