/**
 * Webview-level e2e for the panel (TESTING_ARCHITECTURE_PLAN §3.4, L4):
 * panel open / search / copy-back paste / fetch-on-open decision,
 * against the real compiled Vue bundle + demo-shim (no real GUI app,
 * no real clipboard) — safe to run anywhere Edge is installed, hence
 * nightly-eligible. Mode follows tests/maintenance-browser-e2e.mjs:
 * node:test + playwright-core (channel msedge) + dist static server.
 *
 * Run: `npm run test:e2e:panel` (builds dist first).
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, readFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { resolve, dirname, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';

const artifacts = 'tests/.artifacts';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'dist');

async function startStaticServer() {
  const types = { '.html':'text/html; charset=utf-8', '.js':'text/javascript; charset=utf-8',
    '.css':'text/css; charset=utf-8', '.png':'image/png', '.svg':'image/svg+xml',
    '.woff2':'font/woff2', '.ico':'image/x-icon' };
  const server = createServer(async (request, response) => {
    try {
      const url = new URL(request.url, 'http://127.0.0.1');
      const rel = decodeURIComponent(url.pathname).replace(/^\/+/, '') || 'index.html';
      const full = resolve(root, rel);
      if (!full.startsWith(root + '\\') && full !== resolve(root, 'index.html')) {
        response.writeHead(403).end('denied');
        return;
      }
      const bytes = await readFile(full);
      response.writeHead(200, { 'content-type': types[extname(full)] || 'application/octet-stream' });
      response.end(bytes);
    } catch {
      response.writeHead(404).end('missing');
    }
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  return { server, url: 'http://127.0.0.1:' + server.address().port + '/' };
}

// Wrap the shim's invoke to record which commands the real bundle issued —
// this is how the fetch-on-open decision (incident #6) is observed without
// a zoom window (demo shim has none: create_zoom resolves null).
const INVOKE_SPY = () => {
  const list = [];
  window.__cvInvoked = list;
  window.__TAURI_INTERNALS__.invoke = ((orig) => (cmd, args) => {
    list.push(cmd);
    return orig(cmd, args);
  })(window.__TAURI_INTERNALS__.invoke);
};
test('ClipVault panel webview: open/search/paste-back/fetch-on-open', { timeout: 90000 }, async () => {
  await mkdir(artifacts, { recursive: true });
  const preview = process.env.CLIPVAULT_DEMO_URL
    ? { server: null, url: process.env.CLIPVAULT_DEMO_URL }
    : await startStaticServer();
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  const context = await browser.newContext({ viewport: { width: 390, height: 690 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  try {
    const response = await page.goto(preview.url, { waitUntil: 'networkidle', timeout: 18000 });
    assert.equal(response.status(), 200);
    // install the invoke spy AFTER page scripts: the demo shim defines
    // __TAURI_INTERNALS__ in a module script, so addInitScript would run too early
    await page.evaluate(INVOKE_SPY);

    // 1) panel opens with demo rows and a live search box
    await page.locator('.row').first().waitFor({ timeout: 12000 });
    assert.equal(await page.locator('.row').count(), 11, 'panel opened with seeded demo rows');
    assert.notEqual(await page.locator('input.search').count(), 0, 'search input present');

    // 2) search narrows and clears back
    const search = page.locator('input.search');
    await search.fill('会议纪要');
    await page.waitForTimeout(300);
    assert.equal(await page.locator('.row').count(), 1, 'search narrows to 1 row');
    await page.locator('button.search-clear').click();
    await page.waitForTimeout(300);
    assert.equal(await page.locator('.row').count(), 11, 'clearing search restores rows');

    // 3) copy-back paste: clicking a row routes through paste_clip and the
    //    demo shim surfaces its own hint toast (real app pastes to prior window)
    const targetRow = page.locator('.row', { hasText: '会议纪要' }).first();
    const seedPasteCount = await page.evaluate(() => {
      const rows = JSON.parse(localStorage.getItem('cv-demo-clips.v1'));
      return rows.find(r => r.preview.includes('会议纪要')).paste_count;
    });
    await targetRow.click();
    await page.locator('.toast').waitFor({ timeout: 5000 });
    assert.match(await page.locator('.toast').innerText(), /在线演示/,
      'paste_clip click surfaces the demo paste toast');
    const pasteCount = await page.evaluate(() => {
      const rows = JSON.parse(localStorage.getItem('cv-demo-clips.v1'));
      return rows.find(r => r.preview.includes('会议纪要')).paste_count;
    });
    assert.equal(pasteCount, seedPasteCount + 1, 'paste_clip incremented the demo paste counter');
    await page.screenshot({ path: artifacts + '/panel-paste-toast.png' });

    // 4) fetch-on-open decision (incident #6 regression):
    //    hovering a long-text row (content >= 4096) must issue `clip_content`;
    //    hovering a short row must NOT.
    const LONG = 'L'.repeat(4200);
    await page.evaluate((text) => { window.cvDemo.addClip(text, 'text') }, LONG);
    await page.waitForTimeout(400); // clips-changed debounce -> refresh
    const longRow = page.locator('.row', { hasText: LONG.slice(0, 40) }).first();
    await longRow.hover();
    await page.waitForTimeout(300);
    let invoked = await page.evaluate(() => window.__cvInvoked);
    assert.ok(invoked.includes('clip_content'),
      'fetch-on-open: long text (>=4096) must trigger clip_content, got: ' + invoked.join(','));

    await page.evaluate(() => { window.__cvInvoked = [] });
    const shortRow = page.locator('.row', { hasText: '会议纪要' }).first();
    await shortRow.hover();
    await page.waitForTimeout(300);
    invoked = await page.evaluate(() => window.__cvInvoked);
    assert.ok(!invoked.includes('clip_content'),
      'fetch-on-open: short text must NOT trigger clip_content, got: ' + invoked.join(','));

    assert.equal(errors.length, 0, 'Vue runtime errors: ' + errors.join('; '));
    console.log('PASS: panel open (11 demo rows); search narrow/clear; paste_clip toast + counter; fetch-on-open long=yes short=no; zero page errors');
  } finally {
    await context.close();
    await browser.close();
    if (preview.server) await new Promise(resolve => preview.server.close(resolve));
  }
});
