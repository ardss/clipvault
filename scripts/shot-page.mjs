// Render docs/index.html headlessly and screenshot — local preview loop.
import { chromium } from 'playwright-core'
import { pathToFileURL } from 'url'
import path from 'path'
import { fileURLToPath } from 'url'
const root = path.dirname(fileURLToPath(import.meta.url))
const target = process.argv[2] || path.join(root, '..', 'docs', 'index.html')
const out = process.argv[3] || 'C:/tmp/page_preview.png'
const width = parseInt(process.argv[4] || '1280')
const height = parseInt(process.argv[5] || '900')
const browser = await chromium.launch({
  executablePath: 'C:/Program Files/Google/Chrome/Application/chrome.exe',
  headless: true,
})
const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 2 })
await page.goto(pathToFileURL(target).href, { waitUntil: 'load' })
await page.waitForTimeout(400)
await page.screenshot({ path: out, fullPage: true })
// layout metrics for objective QA
const m = await page.evaluate(() => {
  const r = (s) => { const e = document.querySelector(s); if (!e) return null
    const b = e.getBoundingClientRect(); return { x: Math.round(b.x), y: Math.round(b.y), w: Math.round(b.width), h: Math.round(b.height) } }
  return {
    title: document.title,
    h1: r('h1'), shot: r('.hero-shot'), cta: r('.cta'), grid: r('.grid'),
    feats: document.querySelectorAll('.f').length,
    bodyScrollW: document.body.scrollWidth, innerW: window.innerWidth,
    overflowX: document.body.scrollWidth > window.innerWidth,
  }
})
console.log(JSON.stringify(m, null, 1))
await browser.close()
