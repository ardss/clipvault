import { chromium } from 'playwright-core'
import fs from 'node:fs'

const exe = 'C:/Users/13397/AppData/Local/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-win64/chrome-headless-shell.exe'
const out = 'K:/Project/1-Active-Projects/clipvault/tmp-ux'
const browser = await chromium.launch({ executablePath: exe, headless: true })
const page = await browser.newPage({ viewport: { width: 380, height: 620 } })
await page.goto('http://localhost:5199/', { waitUntil: 'networkidle' })
await page.waitForTimeout(900)
const shot = async n => { await page.screenshot({ path: `${out}/${n}.png` }); console.log('shot', n) }

await shot('10-after-fix-list')

// search no-match new copy
const inp = await page.$('input.search') || await page.$('.search')
await inp.click()
await page.keyboard.type('zzzz不存在')
await page.waitForTimeout(500)
await shot('11-after-fix-nomatch')

await browser.close()
console.log('DONE')
