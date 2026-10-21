// Uses an already-installed Playwright; never installs dependencies or reaches a remote Agent.
// BOB_BROWSER_MODULE may point to the bundled playwright/index.mjs on this development host.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const { chromium } = await import(process.env.BOB_BROWSER_MODULE ? pathToFileURL(process.env.BOB_BROWSER_MODULE).href : 'playwright');
const base = process.env.BOB_UI_TEST_URL || 'http://127.0.0.1:5178';
assert.equal(new URL(base).hostname, '127.0.0.1');
const output = join(tmpdir(), 'bob-remote-agent-ui');
await mkdir(output, { recursive: true });
const browser = await chromium.launch({ channel: 'msedge', headless: true });
try {
  for (const [name, width, height, lang] of [['mobile', 390, 844, 'zh-CN'], ['desktop', 1280, 900, 'en-US']]) {
    const page = await browser.newPage({ viewport: { width, height } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.route('**/*', route => new URL(route.request().url()).origin === base ? route.continue() : route.abort());
    await page.goto(`${base}/tests/fixtures/remote-agent-preview.html?lang=${lang}`);
    await page.locator('input[type=url]').fill('https://fixture.invalid/?token=fixture-only');
    await page.locator('button[type=submit]').click();
    await page.getByRole('button', { name: '建筑模型渲染 · 示例' }).click();
    await page.locator('.remote-message').last().waitFor();
    assert.equal(await page.locator('.remote-transcript img').count(), 0);
    assert.equal(await page.evaluate(() => Boolean(window.__unsafe)), false);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    assert.equal(await page.locator('.remote-transcript').evaluate(el => el.scrollWidth <= el.clientWidth), true);
    await page.screenshot({ path: join(output, `${name}.png`), fullPage: true });
    await page.getByRole('button', { name: lang === 'zh-CN' ? '断开本次连接' : 'Disconnect session' }).click();
    await page.locator('input[type=password]').waitFor();
    assert.equal(await page.locator('input[type=password]').inputValue(), '');
    assert.equal((await page.locator('input[type=url]').inputValue()).includes('token'), false);
    assert.deepEqual(await page.evaluate(() => window.__remoteFixtureCalls), [['disconnect', 'fixture']]);
    assert.deepEqual(errors, []);
    await page.close();
  }
  console.log(`Remote Agent UI: mobile + desktop passed; simulated data only. Screenshots: ${output}`);
} finally { await browser.close(); }
