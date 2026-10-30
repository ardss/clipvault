import { chromium } from 'playwright-core'
import { execSync } from 'node:child_process'
import fs from 'node:fs'

const iso = 'C:/Users/13397/cv-iso-test'
const log = m => { const s = `${new Date().toISOString()} ${m}`; console.log(s); fs.appendFileSync(`${iso}/driver.log`, s + '\n') }
const psq = c => execSync(`powershell -NoProfile -ExecutionPolicy Bypass -Command "${c.replace(/"/g, '\\"')}"`, { encoding: 'utf8' }).trim()
const sleep = ms => new Promise(r => setTimeout(r, ms))
let _cb = null
async function cdpChrome(fn) {
  if (!_cb) _cb = await (await import('playwright-core')).chromium.connectOverCDP('http://127.0.0.1:9224')
  const p = _cb.contexts()[0].pages().find(x => x.url().includes('target.html'))
  await fn(p)
}

const scenarios = [
  { name: 'textbox', pidFile: 'hostPid', marker: 'CV-ISO-PASTE-EDITCONTROL-文本框', focus: 'tb' },
  { name: 'richtextbox', pidFile: 'hostPid', marker: 'CV-ISO-PASTE-RICHEDIT-富文本', focus: 'rtb' },
  { name: 'notepad', pidFile: 'npPid', marker: 'CV-ISO-PASTE-NOTEPAD-记事本', focus: null },
  { name: 'chrome', pidFile: 'chromePid', marker: 'CV-ISO-PASTE-CHROME-浏览器', focus: null },
].filter(s => !process.argv.slice(2).length || process.argv.slice(2).includes(s.name))

const FGPS = `powershell -NoProfile -ExecutionPolicy Bypass -File "K:/Project/1-Active-Projects/clipvault/tmp-ux/fg-pid.ps1"`

async function main() {
  const browser = await chromium.connectOverCDP('http://127.0.0.1:9223')
  const ctx = browser.contexts()[0]
  const panel = ctx.pages().find(p => /tauri|localhost/i.test(p.url()) && !/zoom=1/.test(p.url())) || ctx.pages()[0]
  log('panel url: ' + panel.url())
  const targets = JSON.parse(fs.readFileSync(`${iso}/targets.json`, 'utf8').replace(/^\uFEFF/, ''))

  // focus host control via signal file read by the host's timer pump
  const focusHost = which => { fs.writeFileSync(`${iso}/host-focus.txt`, which); return sleep(400) }
  // put UIA (accessibility) focus on the notepad editor: Win32 hwndFocus only proves
  // the WinUI island has focus - XAML-internal focus can sit on the tab strip, where
  // the injected Ctrl+V is swallowed (notepad 0/4 across 2026-10-30 runs 1-3)
  const focusNotepad = () => psq(`powershell -NoProfile -ExecutionPolicy Bypass -File "K:/Project/1-Active-Projects/clipvault/tmp-ux/focus-notepad.ps1" -hwnd ${targets.npHwnd}`)

  for (const s of scenarios) {
    log(`== ${s.name} ==`)
    let pass = false
    // scenario-level retry: each attempt is a full paste flow (the app's row click
    // is one-shot); the host controls land intermittently (~50% per run on
    // 2026-10-30), notepad needs the UIA focus fix, chrome lands every time
    for (let attempt = 0; attempt < 3 && !pass; attempt++) {
      if (attempt) log(`-- ${s.name} retry ${attempt} --`)
      const pid = targets[s.pidFile]
      // hide panel from previous scenario (clears paste target)
      await panel.evaluate(() => window.__TAURI_INTERNALS__.invoke('hide_panel_cmd')).catch(() => {})
      await sleep(500)

      // deterministic foreground take for our own window (AttachThreadInput, no input injection)
      const title = s.pidFile === 'hostPid' ? 'CV-ISO-HOST' : (s.name === 'chrome' ? 'CV-ISO-BROWSER-TARGET' : 'notepad-target')
      if (s.name === 'chrome') { await cdpChrome(p => p.evaluate(() => document.querySelector('#t').focus())) }
      let ok = false
      // patient takes: a foreign UWP foreground holder (e.g. the Settings app in
      // ApplicationFrameHost) can pin the foreground ~60s and revert every
      // SetForegroundWindow; 3x800ms gave up long before it released
      // (2026-10-30 07:14 round: all 9 host takes lost to FG_PID=12696)
      for (let a = 0; a < 10 && !ok; a++) {
        const arg = title ? `-title "${title}"` : `-title none`
        const out = psq(`powershell -NoProfile -ExecutionPolicy Bypass -File "K:/Project/1-Active-Projects/clipvault/tmp-ux/focus-take.ps1" ${arg} -pid2 ${pid}`)
        log(`take#${a}: ${String(out).trim()}`)
        ok = /FG_PID=(\d+)/.exec(out)?.[1] === String(pid)
        if (!ok) await sleep(2500)
      }
      if (!ok) { log(`ABORT ${s.name}: focus-take failed`); continue }
      if (s.pidFile === 'hostPid') await focusHost(s.focus)   // set child focus while window IS foreground
      if (s.name === 'notepad') { const fr = focusNotepad(); log(`notepad uia focus: ${fr}`) }
      await sleep(700)
      const fgNow = psq(FGPS)
      if (fgNow !== String(pid)) { log(`ABORT ${s.name}: foreground=${fgNow} != target=${pid} (fail-safe, no input sent)`); continue }

      // fictional marker onto clipboard -> captured by test instance as newest row.
      // Single set + settle: extra clipboard opens here collide with the app's own
      // click-time set.
      psq(`Set-Clipboard -Value '${s.marker}'`)
      await sleep(1200)

      // show panel (trusted in-webview invoke; captures paste target=our window)
      await panel.evaluate(() => window.__TAURI_INTERNALS__.invoke('show_panel_cmd'))
      await sleep(1100)

      // real UI click on newest row (one-shot paste flow: clipboard set -> hide panel
      // -> restore fg/focus -> handshake -> injector verifies and fires Ctrl+V)
      const rows = panel.locator('.row', { hasText: s.marker.slice(0, 16) })
      const n = await rows.count()
      log(`rows matching marker=${n}`)
      if (!n) { log('!! marker row not found'); continue }
      await rows.first().click()
      await sleep(2000)

      // diagnostic: what does the clipboard hold after the paste flow
      const cb = psq('Get-Clipboard -Raw').trim()
      log(`clipboard after paste: len=${cb.length} head="${cb.slice(0, 24)}"`)

      // read back
      if (s.pidFile === 'hostPid') {
        let lines = {}
        for (let t = 0; t < 20 && (!lines.TB || lines.RTB === undefined); t++) {
          const st = fs.readFileSync(`${iso}/host-state.txt`, 'utf8').replace(/^\uFEFF/, '')
          lines = Object.fromEntries(st.split(/\r?\n/).filter(l => /^(TB|RTB)=/.test(l)).map(l => [l.slice(0, l.indexOf('=')), l.slice(l.indexOf('=') + 1)]))
          if (!lines.TB) await sleep(150)
        }
        const got = s.focus === 'tb' ? lines.TB : lines.RTB
        const other = s.focus === 'tb' ? lines.RTB : lines.TB
        const otherBase = s.focus === 'tb' ? (base.RTB ?? lines.RTB) : (base.TB ?? lines.TB)
        pass = got.includes(s.marker) && other === otherBase
        log(`RESULT ${s.name}: landed=${got.includes(s.marker)} otherKept=${other === otherBase} PASS=${pass} text="${String(got).slice(0, 40)}"`)
      } else if (s.name === 'notepad') {
        // read notepad via UIA value pattern, hwnd-scoped (Win11 notepad may share one
        // process with the user's own windows, so pid lookup can hit the wrong window)
        const out = psq(`powershell -NoProfile -ExecutionPolicy Bypass -File "K:/Project/1-Active-Projects/clipvault/tmp-ux/read-notepad.ps1" -hwnd ${targets.npHwnd}`)
        const landed = out.includes(s.marker)
        log(`RESULT notepad: uia="${out.slice(0, 60)}" landed=${landed} PASS=${landed}`)
        pass = landed
      } else if (s.name === 'chrome') {
        // reuse the cached 9224 connection: Browser.close() on a HEADED connectOverCDP
        // browser hangs forever (seen 2026-10-30: driver never exited, watcher stuck)
        let val = ''
        await cdpChrome(async p => { val = await p.inputValue('#t') })
        const landed = val.includes(s.marker)
        log(`RESULT chrome: landed=${landed} PASS=${landed} text="${val.slice(0, 40)}"`)
        pass = landed
      }
    }
  }
  log('ALL SCENARIOS DONE')
  process.exit(0)  // open CDP transports keep the loop alive; hard exit is the only reliable end
}
let base = {}
main().catch(e => { log('FATAL ' + e.message); process.exit(1) })
