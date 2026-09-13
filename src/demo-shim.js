// Browser demo shim — lets the real panel UI run on the public website.
// Active ONLY when the Tauri runtime is absent; inside the app this file is a no-op.
// Data lives in localStorage (key cv-demo-clips.v1), like pickdone's demo-shim.
import seedShot1 from './assets/seed-shot1.png'
import seedShot2 from './assets/seed-shot2.png'

(function () {
  if (window.__TAURI_INTERNALS__) return

  const LS_KEY = 'cv-demo-clips.v1'
  const LS_SET = 'cv-demo-settings.v1'

  const HOUR = 3600000
  const now = Date.now()
  const startOfToday = new Date(); startOfToday.setHours(0, 0, 0, 0)
  const t0 = startOfToday.getTime()

  function seedRows() {
    return [
      { id: 'd1', kind: 'text', preview: '会议纪要已发你邮箱，下午三点前确认一下行动项', content: '会议纪要已发你邮箱，下午三点前确认一下行动项', pinned: false, created_at: now - 0.2 * HOUR, paste_count: 3 },
      { id: 'd2', kind: 'image', preview: '', content: '', image_path: seedShot1, pinned: false, created_at: now - 0.6 * HOUR, paste_count: 1 },
      { id: 'd3', kind: 'text', preview: 'SELECT id, kind, preview FROM clips WHERE pinned = 1;', content: 'SELECT id, kind, preview FROM clips WHERE pinned = 1;', pinned: false, created_at: now - 1.1 * HOUR, paste_count: 2 },
      { id: 'd4', kind: 'file', preview: '4 个文件：周报.docx、数据.xlsx、架构图.png、README.md', content: '', pinned: false, created_at: now - 2 * HOUR, paste_count: 0 },
      { id: 'd5', kind: 'link', preview: 'https://github.com/ardss/clipvault', content: 'https://github.com/ardss/clipvault', pinned: false, created_at: now - 3 * HOUR, paste_count: 5 },
      { id: 'd6', kind: 'html', preview: 'ClipVault — 复制过的一切，都在', content: 'ClipVault — 复制过的一切，都在', pinned: false, created_at: now - 5 * HOUR, paste_count: 1 },
      { id: 'd7', kind: 'text', preview: 'git rebase -i HEAD~3', content: 'git rebase -i HEAD~3', pinned: true, created_at: now - 7 * HOUR, paste_count: 8 },
      { id: 'd8', kind: 'image', preview: '', content: '', image_path: seedShot2, pinned: false, created_at: t0 - 10 * HOUR, paste_count: 0 },
      { id: 'd9', kind: 'text', preview: '收件人：me@example.com / 工牌号 13397', content: '收件人：me@example.com / 工牌号 13397', pinned: true, created_at: t0 - 26 * HOUR, paste_count: 4 },
      { id: 'd10', kind: 'text', preview: '公交卡充值 50 元成功，余额 86.5 元', content: '公交卡充值 50 元成功，余额 86.5 元', pinned: false, created_at: t0 - 30 * HOUR, paste_count: 0 },
      { id: 'd11', kind: 'text', preview: 'function debounce(fn, ms) { let t; return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms) } }', content: 'function debounce(fn, ms) { let t; return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms) } }', pinned: false, created_at: t0 - 52 * HOUR, paste_count: 2 },
    ]
  }

  function load() {
    try {
      const rows = JSON.parse(localStorage.getItem(LS_KEY))
      if (Array.isArray(rows)) return rows
    } catch (e) { /* ignore */ }
    const rows = seedRows()
    save(rows)
    return rows
  }
  function save(rows) {
    try { localStorage.setItem(LS_KEY, JSON.stringify(rows)) } catch (e) { /* ignore */ }
  }
  function loadSettings() {
    try { return JSON.parse(localStorage.getItem(LS_SET)) } catch (e) { return null }
  }

  function isLink(s) { return /^https?:\/\//i.test(s || '') }
  function listClips(args) {
    const f = (args && args.filter) || 'all'
    const q = ((args && args.query) || '').toLowerCase()
    let rows = load()
    if (f === 'text') rows = rows.filter(r => r.kind === 'text')
    else if (f === 'image') rows = rows.filter(r => r.kind === 'image')
    else if (f === 'link') rows = rows.filter(r => r.kind === 'link' || isLink(r.content))
    else if (f === 'file') rows = rows.filter(r => r.kind === 'file')
    else if (f === 'pinned') rows = rows.filter(r => r.pinned)
    if (q) rows = rows.filter(r => (r.preview || '').toLowerCase().includes(q) || (r.content || '').toLowerCase().includes(q))
    return rows.slice().sort((a, b) => b.created_at - a.created_at)
  }
  function stats() {
    const rows = load()
    const today = rows.filter(r => r.created_at >= t0).length
    const daily = []
    for (let i = 6; i >= 0; i--) {
      const dayStart = t0 - i * 86400000
      const key = new Date(dayStart).toISOString().slice(0, 10)
      daily.push([key, rows.filter(r => r.created_at >= dayStart && r.created_at < dayStart + 86400000).length])
    }
    const top = rows.filter(r => r.paste_count > 0).sort((a, b) => b.paste_count - a.paste_count)
      .slice(0, 5).map(r => [r.preview.slice(0, 40) || '(图片)', r.paste_count])
    return {
      total: rows.length, today,
      text: rows.filter(r => r.kind === 'text').length,
      image: rows.filter(r => r.kind === 'image').length,
      link: rows.filter(r => r.kind === 'link' || isLink(r.content)).length,
      file: rows.filter(r => r.kind === 'file').length,
      pinned: rows.filter(r => r.pinned).length,
      paste_total: rows.reduce((s, r) => s + (r.paste_count || 0), 0),
      daily, top,
    }
  }
  function addClip(preview, kind) {
    const rows = load()
    rows.push({ id: 'demo_' + Date.now(), kind: kind || 'text', preview, content: preview, pinned: false, created_at: Date.now(), paste_count: 0 })
    save(rows)
    fire('clips-changed', null)
  }

  // event plumbing: api's listen() registers callbacks through transformCallback,
  // stored as window['_'+id]; keep handler ids per event so we can fire them.
  const eventHandlers = {}
  let cbSeq = 100
  function fire(event, payload) {
    for (const hid of eventHandlers[event] || []) {
      const cb = window['_' + hid]
      if (cb) cb({ event, id: 0, payload })
    }
  }

  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    transformCallback(cb) {
      const id = ++cbSeq
      Object.defineProperty(window, '_' + id, { value: cb, configurable: true })
      return id
    },
    convertFileSrc(p) { return p },
    invoke(cmd, args = {}) {
      return new Promise((resolve) => {
        switch (cmd) {
          case 'heartbeat': return resolve(Date.now())
          case 'list_clips': return resolve(listClips(args))
          case 'stats': return resolve(stats())
          case 'get_settings': return resolve(loadSettings() || { history_limit: 1000, panel_height: 540, autostart: false })
          case 'set_settings': try { localStorage.setItem(LS_SET, JSON.stringify(args.settings)) } catch (e) {} return resolve(null)
          case 'toggle_pin': {
            const rows = load(); const r = rows.find(x => x.id === args.id)
            if (r) { r.pinned = !r.pinned; save(rows) }
            return resolve(null)
          }
          case 'delete_clip': {
            const rows = load().filter(x => x.id !== args.id); save(rows)
            return resolve(null)
          }
          case 'paste_clip': {
            const rows = load(); const r = rows.find(x => x.id === args.id)
            if (r) { r.paste_count = (r.paste_count || 0) + 1; save(rows) }
            return resolve({ demo: true }) // the site shows its own hint for this
          }
          case 'plugin:window|get_all_windows': return resolve([])
          case 'plugin:event|listen': {
            const ev = args.event
            const hid = args.handler
            ;(eventHandlers[ev] = eventHandlers[ev] || new Set()).add(hid)
            return resolve(++cbSeq)
          }
          case 'plugin:event|unlisten':
          case 'plugin:event|emit':
          case 'save_panel_height':
          case 'hide_panel_cmd':
          case 'report_error':
            return resolve(null)
          default:
            console.warn('[cv-demo-shim] unhandled command:', cmd)
            return resolve(null)
        }
      })
    },
  }

  // hooks the parent website can call (same origin)
  window.cvDemo = { addClip, reset() { localStorage.removeItem(LS_KEY); load(); fire('clips-changed', null) } }
})()
