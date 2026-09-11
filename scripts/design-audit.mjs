// Quantitative design audit of docs/index.html via computed styles.
import { chromium } from 'playwright-core'
import { pathToFileURL } from 'url'
import path from 'path'
import { fileURLToPath } from 'url'
const root = path.dirname(fileURLToPath(import.meta.url))
const browser = await chromium.launch({ executablePath: 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true })
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } })
await page.goto(pathToFileURL(path.join(root, '..', 'docs', 'index.html')).href, { waitUntil: 'networkidle' })
const audit = await page.evaluate(() => {
  const pick = (sel, props) => {
    const e = document.querySelector(sel)
    if (!e) return { sel, missing: true }
    const cs = getComputedStyle(e)
    const b = e.getBoundingClientRect()
    const out = { sel, x: Math.round(b.x), y: Math.round(b.y), w: Math.round(b.width), h: Math.round(b.height) }
    for (const p of props) out[p] = cs[p]
    return out
  }
  return {
    h1: pick('h1', ['fontSize', 'fontWeight', 'letterSpacing', 'color']),
    tagline: pick('.tagline', ['fontSize', 'lineHeight', 'color']),
    btnPrimary: pick('.btn.primary', ['fontSize', 'padding', 'borderRadius', 'backgroundColor']),
    heroShot: pick('.hero-shot', ['width']),
    nav: pick('nav', ['height', 'borderBottomColor']),
    featTitle: pick('.f h3', ['fontSize', 'fontWeight']),
    featBody: pick('.f p', ['fontSize', 'lineHeight', 'color']),
    h2: pick('h2', ['fontSize']),
    docWidth: document.body.scrollWidth,
    imageNatural: (() => { const i = document.querySelector('.hero-shot'); return i ? { w: i.naturalWidth, h: i.naturalHeight, dispW: Math.round(i.getBoundingClientRect().width) } : null })(),
  }
})
console.log(JSON.stringify(audit, null, 1))
await browser.close()
