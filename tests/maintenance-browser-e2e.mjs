/**
 * Browser-level smoke for the real compiled Vue panel with demo-only clips.
 * Run against an isolated Vite preview (or CLIPVAULT_DEMO_URL).
 * Uses a fresh headless Edge profile; never reads the user's real clipboard.
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

// Serve ONLY compiled static assets over loopback, with no user clipboard,
// no external dev server and no dependency on a fixed, shared debug port.
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

test('ClipVault compiled panel: search/pin/settings/restart/clear/390px', { timeout: 90000 }, async () => {
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
    await page.locator('.row').first().waitFor({ timeout: 12000 });
    assert.equal(await page.locator('.row').count(), 11, 'synthetic sample data seeded');
    const horizontalOverflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth);
    assert.equal(horizontalOverflow, false, 'no 390px horizontal overflow');
    await page.screenshot({ path: artifacts + '/clipvault-390-overview.png' });

    const search = page.locator('input.search');
    await search.fill('会议纪要');
    await page.waitForTimeout(300);
    assert.equal(await page.locator('.row').count(), 1, 'search narrows records');
    await page.locator('button.search-clear').click();
    assert.equal(await page.locator('.row').count(), 11, 'clear search restores rows');

    await page.locator('.tab').filter({ hasText: '置顶' }).click();
    assert.equal(await page.locator('.row').count(), 2, 'pinned filter');
    await page.locator('.tab').filter({ hasText: '全部' }).click();
    await page.locator('#clip-d1 .pin').click();
    await page.locator('.tab').filter({ hasText: '置顶' }).click();
    assert.equal(await page.locator('.row').count(), 3, 'pin persists through filter');
    await page.reload({ waitUntil: 'networkidle' });
    await page.locator('.row').first().waitFor();
    await page.locator('.tab').filter({ hasText: '置顶' }).click();
    assert.equal(await page.locator('.row').count(), 3, 'pin survives reload');

    await page.locator('button[title="设置"]').click();
    await page.locator('.settings').waitFor();
    await page.locator('input[aria-label="敏感词过滤"]').fill('password, token');
    await page.waitForTimeout(900);
    await page.reload({ waitUntil: 'networkidle' });
    await page.locator('button[title="设置"]').click();
    assert.equal(await page.locator('input[aria-label="敏感词过滤"]').inputValue(), 'password, token',
      'settings persist after restart');
    await page.screenshot({ path: artifacts + '/clipvault-390-settings.png' });

    // Privacy: the public interactive demo must not secretly persist text
    // matching the user's configured sensitive-keyword filter.
    const blocked = await page.evaluate(() => ({
      accepted: window.cvDemo.addClip('TOKEN=synthetic-secret', 'text'),
      stored: localStorage.getItem('cv-demo-clips.v1').includes('synthetic-secret'),
      count: JSON.parse(localStorage.getItem('cv-demo-clips.v1')).length
    }));
    assert.equal(blocked.accepted, false, 'sensitive value should be rejected');
    assert.equal(blocked.stored, false, 'sensitive value must never reach localStorage');
    assert.equal(blocked.count, 11, 'filter must leave clip count unchanged');

    const countBefore = await page.evaluate(() => JSON.parse(localStorage.getItem('cv-demo-clips.v1')).length);
    assert.equal(countBefore, 11, 'demo data only; safe to test double-confirm delete');
    // A first click must NOT stay armed when the settings panel is dismissed.
    // Otherwise an unrelated later click could permanently delete history.
    await page.locator('button.clear-btn').click();
    await page.locator('button[title="设置"]').click();
    await page.locator('button[title="设置"]').click();
    assert.match(await page.locator('button.clear-btn').innerText(), /清空全部历史/,
      'closing settings cancels a pending destructive confirmation');
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('cv-demo-clips.v1')).length), 11);
    await page.locator('button.clear-btn').click();
    await page.locator('button[title="统计"]').click();
    await page.locator('button[title="设置"]').click();
    assert.match(await page.locator('button.clear-btn').innerText(), /清空全部历史/,
      'switching from settings to statistics also cancels destructive confirmation');
    await page.locator('button.clear-btn').click();
    assert.match(await page.locator('button.clear-btn').innerText(), /再点一次确认/);
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('cv-demo-clips.v1')).length), 11,
      'first clear click must not delete');
    await page.locator('button.clear-btn').click();
    await page.locator('button[title="设置"]').click();
    assert.equal(await page.locator('.row').count(), 0, 'confirmed clear empties demo rows');
    await page.reload({ waitUntil: 'networkidle' });
    assert.equal(await page.locator('.row').count(), 0, 'clear is durable across reload');
    assert.equal(errors.length, 0, 'Vue runtime errors: ' + errors.join('; '));
    console.log('PASS: 11 demo rows; 390px; search/clear; pin & reload; settings persistence; double-confirm & durable clear; zero page errors');
  } finally {
    await context.close();
    await browser.close();
    if (preview.server) await new Promise(resolve => preview.server.close(resolve));
  }
});
