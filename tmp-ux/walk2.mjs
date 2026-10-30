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

// 1. search with focus
const inp = await page.$('input[placeholder*="搜索"], input[type="search"], input')
await inp.click()
await page.keyboard.type('会议')
await page.waitForTimeout(500)
await shot('02-search-meeting')

// 2. search no result empty state
await inp.fill('')
await inp.click(); await inp.fill('')
await page.keyboard.type('zzzz不存在')
await page.waitForTimeout(500)
await shot('02b-search-empty')

// 3. back to all, keyboard nav + enter
await inp.fill('')
await page.keyboard.press('Escape')
await page.waitForTimeout(400)
await page.keyboard.press('ArrowDown'); await page.keyboard.press('ArrowDown')
await page.waitForTimeout(200)
await shot('03-arrow-select-2down')
await page.keyboard.press('Enter')
await page.waitForTimeout(400)
await shot('04-after-enter')
// status/toast after enter?

// 4. delete a row with Del key
await page.keyboard.press('Delete')
await page.waitForTimeout(400)
await shot('04b-after-del')

// 5. image tab
const imgTab = await page.$$eval('button', bs => bs.map((b,i)=>[i,(b.textContent||'').trim()]))
const idx = imgTab.find(x => x[1] === '图片')
await (await page.$$('button'))[idx[0]].click()
await page.waitForTimeout(500)
await shot('07-image-filter')

// 6. settings gear
const gear = await page.$('button[aria-label*="设置"], .gear, #setBtn')
if (gear) await gear.click()
else {
  // find button near top-right: settings likely an svg button
  const bs = await page.$$('button')
  await bs[bs.length - 1].click()
}
await page.waitForTimeout(400)
await shot('08-settings')

await browser.close()
console.log('DONE')
