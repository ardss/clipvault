import { chromium } from 'playwright-core'
import fs from 'node:fs'

const exe = 'C:/Users/13397/AppData/Local/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-win64/chrome-headless-shell.exe'
const out = 'K:/Project/1-Active-Projects/clipvault/tmp-ux'
fs.mkdirSync(out, { recursive: true })
const browser = await chromium.launch({ executablePath: exe, headless: true })
const page = await browser.newPage({ viewport: { width: 380, height: 620 } })
await page.goto('http://localhost:5199/', { waitUntil: 'networkidle' })
await page.waitForTimeout(800)

const shot = async n => { await page.screenshot({ path: `${out}/${n}.png` }); console.log('shot', n) }

await shot('01-initial-list')

// search flow
await page.keyboard.type('会议')
await page.waitForTimeout(500)
await shot('02-search-meeting')

// keyboard nav: arrows + enter
await page.keyboard.press('ArrowDown'); await page.keyboard.press('ArrowDown')
await page.waitForTimeout(300)
await shot('03-arrow-select')
await page.keyboard.press('Enter')
await page.waitForTimeout(400)
await shot('04-after-enter')

// clear search, check dedup-ish long list + pinned
await page.fill('input[type="search"], input[placeholder]', '')
await page.keyboard.press('Escape')
await page.waitForTimeout(400)
await shot('05-list-all')

// image content card
await page.keyboard.type('png') // hope matches file entry? try filter image instead
await page.waitForTimeout(300)

// use the filter tabs if present
const tabs = await page.$$('button')
const labels = []
for (const t of tabs) labels.push((await t.textContent() || '').trim())
console.log('buttons:', labels.join(' | ').slice(0, 200))
await page.keyboard.press('Escape')
await page.waitForTimeout(300)

// pinned filter via UI text
for (const t of tabs) {
  const l = (await t.textContent() || '').trim()
  if (l === '置顶' || l === 'Pinned' || /pin/i.test(l)) { await t.click(); break }
}
await page.waitForTimeout(400)
await shot('06-pinned-filter')

// image filter
const all = await page.$$('[data-i18n], button')
for (const t of tabs) {
  const l = (await t.textContent() || '').trim()
  if (l === '图片' || l === 'Image') { await t.click(); break }
}
await page.waitForTimeout(500)
await shot('07-image-filter')

// dark theme toggle
const themeBtn = await page.$('#themeBtn') ? null : null
// panel is inside page? theme button id exists on landing page; panel may have its own
await page.evaluate(() => { const b = document.querySelector('#themeBtn'); if (b) b.click() })
await page.waitForTimeout(400)
await shot('08-theme')

await browser.close()
console.log('DONE')
