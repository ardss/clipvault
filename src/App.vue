<script setup>
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen, emit } from '@tauri-apps/api/event'
import { emitTo } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi'
import { shouldFetchFullContent } from './modules/openPolicy.js'
import { createI18nStore, createThemeStore, sensitiveTextComputed, excludedAppsComputed } from './modules/settingsStore.js'
import { LANGS } from './modules/i18n.js'
import { reduce as reduceClearArmed, INITIAL_STATE, ARM_TIMEOUT_MS } from './modules/confirmArmed.js'
import { pushUndo, popUndo, buildUndoCommands, resetUndo } from './modules/groupActions.js'

const isZoomWin =
  getCurrentWindow().label === 'zoom' ||
  new URLSearchParams(window.location.search).has('zoom')

// surface JS errors to the backend log — a blank panel must never be a mystery
window.addEventListener('error', () => {
  // Clipboard content can appear inside thrown errors or source URLs. Log only the event class.
  invoke('report_error', { msg: 'js-error' }).catch(() => {})
})
window.addEventListener('unhandledrejection', () => {
  invoke('report_error', { msg: 'unhandled-rejection' }).catch(() => {})
})

const { lang, t, toggleLang, langLabel } = createI18nStore()
function setLang(l) {
  if (LANGS.includes(l)) { lang.value = l }
}

const { theme, toggleTheme } = createThemeStore()

const health = ref('...')
const beatMs = ref(0)
const filter = ref('all')
const query = ref('')
const clips = ref([])
const showStats = ref(false)
const stats = ref(null)
const showSettings = ref(false)
const showClearConfirm = ref(false)
const settings = ref({ history_limit: 1000, panel_height: 540, autostart: false, image_capture: true, excluded_apps: [], auto_resume_minutes: 0 })

// Font size: 4 steps (100% / 110% / 125% / 150%) applied as CSS zoom on the
// panel root. Persisted as a UI pref (localStorage) like language/theme.
const FONT_STEPS = [1.0, 1.1, 1.25, 1.5]
const fontStep = ref(Math.min(3, Math.max(0, parseInt(localStorage.getItem('cv-fontstep') || '0', 10) || 0)))
const fontZoom = computed(() => FONT_STEPS[fontStep.value])
function setFontStep(v) {
  fontStep.value = v
  localStorage.setItem('cv-fontstep', String(v))
}
const toast = ref('')
let toastTimer = null
let searchTimer = null
const selected = ref(-1)

// ---------- 0.2 three-state panel (hidden / expanded / pinned) ----------
// pinned mirrors settings.panel_pinned; the Rust atom (PANEL_PINNED) gates
// mouse-leave auto-hide and the Alt+V hide branch.
const pinned = ref(false)
async function togglePinned() {
  pinned.value = !pinned.value
  try {
    await invoke('set_panel_pinned', { on: pinned.value })
    if (settings.value) settings.value.panel_pinned = pinned.value
    showToast(pinned.value ? t('pinnedOn') : t('pinnedOff'))
  } catch (e) { console.error(e) }
}

// ---------- 0.2 auto-hide suppression channel ----------
// TRUE while search is focused / stats or settings page open / clear-confirm
// modal open / any drag session runs — the prototype rule: "搜索与筛选期间
// 不自动收起". Feeds the Rust AUTO_HIDE_SUPPRESSED atom via invoke.
const searchFocused = ref(false)
const dragOutActive = ref(false)
const mergeDragActive = ref(false)
const autoHideSuppressed = computed(() =>
  searchFocused.value || showStats.value || showSettings.value ||
  showClearConfirm.value || dragOutActive.value || mergeDragActive.value
)
watch(autoHideSuppressed, (on) => {
  invoke('set_auto_hide_suppressed', { on }).catch(() => {})
})

// ---------- 0.2 group UI state ----------
const expandedGroups = ref(new Set())
function toggleGroup(gid) {
  const s = new Set(expandedGroups.value)
  s.has(gid) ? s.delete(gid) : s.add(gid)
  expandedGroups.value = s
}
// undo toast state — the toast carries an 撤销 button while an action is on the stack
const canUndo = ref(false)

function clipKindLabel(c) {
  if (c.kind === 'image') return t('image')
  if (c.kind === 'file') return t('file')
  if (isLinkClip(c)) return t('link')
  if (isCodeClip(c)) return t('code')
  return t('text')
}
function isLinkClip(c) {
  return c.kind === 'text' && /^https?:\/\//i.test((c.preview || '').trim())
}
function hostOf(c) {
  const m = (c.preview || '').trim().match(/^https?:\/\/([^/\s]+)/i)
  return m ? m[1] : ''
}
function isCodeClip(c) {
  if (c.kind !== 'text' || isLinkClip(c)) return false
  const s = c.preview || ''
  if (/^https?:\/\//i.test(s.trim())) return false
  return /\{[\s\S]*\}/.test(s) || /;\s*$/m.test(s) || /^\s{2,}\S/m.test(s) ||
    /^\s*(fn |def |function |class |import |using |package |const |let |var |#include)/m.test(s)
}

function showToast(msg) {
  toast.value = msg
  clearTimeout(toastTimer)
  toastTimer = setTimeout(() => (toast.value = ''), 2600)
}

function groupOf(c) {
  const day = 86400000
  const startOfToday = new Date(); startOfToday.setHours(0, 0, 0, 0)
  const t0 = startOfToday.getTime()
  if (c.created_at >= t0) return t('dayToday')
  if (c.created_at >= t0 - day) return t('dayYesterday')
  return t('dayEarlier')
}
const grouped = computed(() => {
  // pinned rows sort first and can be older than today's rows; giving them
  // their own section keeps the 今天/昨天 date groups from repeating
  const pinLabel = t('pin')
  const out = []
  let cur = null
  const push = (label, c) => {
    if (!cur || cur.label !== label) { cur = { label, items: [] }; out.push(cur) }
    cur.items.push(c)
  }
  for (const c of clips.value) {
    if (c.pinned && filter.value !== 'pinned') push(pinLabel, c)
    else push(groupOf(c), c)
  }
  return out
})

let reqId = 0
function preserveSelection(prevId) {
  if (prevId == null) return
  const idx = flat.value.findIndex((c) => c.id === prevId)
  selected.value = idx >= 0 ? idx : Math.min(selected.value, flat.value.length - 1)
}
async function refresh() {
  const prevId = flat.value[selected.value]?.id ?? null
  const id = ++reqId
  const r = await invoke('list_clips', { filter: filter.value, query: query.value.trim() })
  if (id !== reqId) return // a newer request already superseded this one
  clips.value = r
  selected.value = -1
  preserveSelection(prevId)
}
function setFilter(f) { filter.value = f; refresh() }
function clearSearch() { query.value = ''; refresh() }
function onSearch() {
  clearTimeout(searchTimer)
  searchTimer = setTimeout(refresh, 120)
}

async function openStats() {
  showSettings.value = false
  showStats.value = true
  // switching to the stats page invalidates a pending destructive
  // confirmation (incident #10: armed state must not survive a page switch)
  disarmClear('switch-page')
  stats.value = await invoke('stats')
}
function closeOverlays() {
  showStats.value = false
  showSettings.value = false
  // Dismissing settings invalidates a pending destructive confirmation.
  disarmClear('panel-close')
  clearTimeout(clearTimer)
}
const dailyMax = computed(() => Math.max(1, ...(stats.value?.daily.map((d) => d[1]) ?? [1])))

let settingsLoaded = false
let saveTimer = null
watch(settings, () => {
  if (!settingsLoaded) return
  clearTimeout(saveTimer)
  saveTimer = setTimeout(async () => {
    try {
      await invoke('set_settings', { settings: settings.value })
      showToast(t('saved'))
    } catch (e) {
      showToast(String(e ?? e.message ?? t('pasteFail')))
      console.error(e)
    }
  }, 500)
}, { deep: true })
// clear-all is destructive: the button must be clicked twice within 3s.
// Decision logic lives in src/modules/confirmArmed.js (pure reducer, incident
// #10 regression coverage); this side owns the ref and the expiry timer.
const clearState = ref({ ...INITIAL_STATE })
const clearArmed = computed(() => clearState.value.armed)
let clearTimer = null
function disarmClear(event) {
  clearTimeout(clearTimer)
  clearState.value = reduceClearArmed(clearState.value, event)
}
// losing focus (panel hidden by the Rust side / user alt-tabs) must
// invalidate a pending destructive confirmation too (incident #10) — the 3s
// disarm timer alone can be throttled while a hidden webview never gets
// timers serviced
// losing focus (panel hidden by the Rust side / user alt-tabs) must
// invalidate a pending destructive confirmation too (incident #10) — the 3s
// disarm timer alone can be throttled while a hidden webview never gets
// timers serviced
function onBlurDisarm() { disarmClear('blur'); searchFocused.value = false }
// wipe is behind a real modal: a double-click on the button can no longer
// destroy history (the armed-click state machine stays for other callers but
// no longer gates deletion)
function onClearAll() {
  disarmClear('open-modal')
  showClearConfirm.value = true
}
async function confirmClearAll() {
  showClearConfirm.value = false
  try {
    const n = await invoke('clear_history')
    ++zoomRequestId
    latestZoomPayload = null
    hideZoomWin().catch(() => {})
    showToast(t('cleared').replace('{n}', String(n)))
    refresh()
  } catch (e) { console.error(e) }
}
function cancelClearAll() {
  showClearConfirm.value = false
}
// keywords live in settings as a string array; the UI edits one comma-separated string
const sensitiveText = sensitiveTextComputed(settings)
const excludedApps = excludedAppsComputed(settings)
async function openSettings() {
  closeOverlays()
  showSettings.value = true
  settingsLoaded = false
  settings.value = await invoke('get_settings')
  await nextTick()
  settingsLoaded = true
}
// keyboard-nav order: a collapsed group contributes its first member as the
// representative; an expanded group contributes every member
const flat = computed(() => {
  const out = []
  for (const sec of sections.value) {
    for (const b of sec.blocks) {
      if (b.type === 'clip') out.push(b.clip)
      else if (expandedGroups.value.has(b.groupId)) out.push(...b.members)
      else out.push(b.members[0])
    }
  }
  return out
})
watch([clips, beatMs], () => {
  health.value = `${clips.value.length} items · ipc ${beatMs.value}ms`
}, { immediate: true })

function thumb(c) {
  if (!c.image_path) return ''
  // thumbnails are <hash>_t.png; older entries may only have the full image
  return convertFileSrc(c.image_path.replace(/\.png$/, '_t.png'))
}
const broken = ref(new Set())
function markBroken(c) { const s = new Set(broken.value); s.add(c.id); broken.value = s }
function thumbFallback(c, e) {
  // second failure (the full-size image is also gone) => genuinely broken;
  // retrying forever would spam console errors on every frame
  if (c.image_path && !e.target.dataset.fbk) {
    e.target.dataset.fbk = '1'
    e.target.src = convertFileSrc(c.image_path)
  } else { markBroken(c) }
}

async function onPin(c, e) { e.stopPropagation(); await invoke('toggle_pin', { id: c.id }); refresh() }
async function onDelete(c, e) {
  e.stopPropagation()
  await invoke('delete_clip', { id: c.id })
  ++zoomRequestId
  latestZoomPayload = null
  hideZoomWin().catch(() => {})
  refresh()
}
async function onClick(c) {
  if (clickSuppressed) { clickSuppressed = false; return } // end of a drag gesture — never paste
  if (dragOutActive.value || mergeDragActive.value) return
  try { const r = await invoke('paste_clip', { id: c.id })
    if (r && r.demo) showToast(t('demoPasted'))
  }
  catch (err) {
    const msg = String(err ?? '')
    if (msg.includes('FOCUS_LOST')) showToast(t('focusLost'))
    else showToast(t('pasteFail'))
  }
}

const zoom = ref(null)
const zoomData = ref(null)
// The hover payload is kept in memory only. Persisting clipboard text in
// localStorage left a second, undeletable copy after clearing history.
let latestZoomPayload = null
let zoomRequestId = 0
let hideTimer = null
// idle reaper for the zoom window (audit P2-6): it costs ~50MB parked all
// day; 10 minutes without a hover closes (destroys) it — ensureZoomWin
// recreates it idempotently on the next hover
let lastZoomUse = 0
function cancelHide() { clearTimeout(hideTimer) }
// closing the popup is owned by the Rust cursor watchdog; the frontend
// never hides it on hover events (event-timing races caused false closes)
function scheduleHide() {}
async function openZoom(c, e) {
  const requestId = ++zoomRequestId
  clearTimeout(hideTimer)
  // only when the one-line preview actually truncates (or it's an image thumb)
  if (c.kind !== 'image') {
    const el = e.currentTarget.querySelector('.preview')
    if (el && el.scrollWidth <= el.clientWidth + 1) {
      hideZoomWin() // a stale preview from a previous row must not linger
      return
    }
  }
  const r = e.currentTarget.getBoundingClientRect()
  const ZW = 340
  // panel-relative -> screen coords (logical px)
  let x = window.screenX + window.innerWidth + 10
  if (x + ZW > window.screen.width) x = window.screenX - ZW - 10
  let top = window.screenY + r.top - 8
  top = Math.max(8, Math.min(top, window.screen.height - 470))
  const payload = c.kind === 'image'
    ? { type: 'image', src: thumb(c) }
    : { type: 'text', text: (c.content || c.preview || '').trim() }
  // the list query truncates content at 4096 chars — fetch the full body
  // for long texts before showing the zoom window. Oversized (>256KB) copies
  // keep the full text in a side file: their content column is only a
  // 2000-char preview, so text_path must trigger the fetch too.
  if (shouldFetchFullContent(c)) {
    try { payload.text = ((await invoke('clip_content', { id: c.id })) || '').trim() } catch {}
  }
  if (requestId !== zoomRequestId) return // older async hover must not replace the newest
  latestZoomPayload = payload
  const zw = await ensureZoomWin()
  if (!zw || requestId !== zoomRequestId) return
  // The first event may precede zoom's listener; zoom requests a replay
  // once mounted. Neither path persists the clipboard content to disk.
  await emitTo('zoom', 'zoom-data', payload).catch(() => {})
  if (requestId !== zoomRequestId) return
  await zw.setPosition(new LogicalPosition(x, top))
  await zw.show()
  lastZoomUse = Date.now()
}
// the zoom window costs ~50MB sitting hidden all day — created on first
// hover via a Rust command (fixed config; no webview-creation permission
// granted to the webview)
async function ensureZoomWin() {
  await invoke('create_zoom').catch(() => {})
  return WebviewWindow.getByLabel('zoom')
}
function rowEnter(c, e) {
  // hover zoom must not fire mid-gesture (drag-out or merge drag)
  if (dragOutActive.value || mergeDragActive.value || cardPress) return
  openZoom(c, e)
}
function rowLeave() { scheduleHide() }
function startResize(e) {
  const startY = e.screenY
  const startH = window.innerHeight
  let raf = null
  const onMove = (ev) => {
    if (raf) return
    raf = requestAnimationFrame(() => {
      raf = null
      const h = Math.max(360, Math.min(900, startH + (ev.screenY - startY)))
      getCurrentWindow().setSize(new LogicalSize(380, h))
    })
  }
  const onUp = () => {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}
async function hideZoomWin() {
  const zw = await WebviewWindow.getByLabel('zoom')
  if (zw) await zw.hide()
}

// ---------- 0.2 card gestures: native OLE drag-out + in-panel merge drag ----------
// One gesture, two outcomes (prototype parity):
//  - mousedown + move past 4px with the pointer leaving the panel window
//    → invoke('start_drag_out') (real OLE DoDragDrop; text/html kinds; the
//    Rust side owns the modal loop). Cancelled drags must NOT paste, so the
//    trailing click is suppressed.
//  - mousedown + move past 4px staying inside the panel → ghost drag; hovering
//    another card for 600ms arms a merge (drop-ring), mouseup merges.
const DRAG_OUT_PX = 4
const MERGE_DWELL_MS = 600
const mergeDrag = ref({ targetId: null, dwellPct: 0 })
let cardPress = null
let dwellRaf = null
let clickSuppressed = false

function onCardPointerDown(c, e) {
  if (e.button !== 0) return
  if (e.target.closest('button, input, .child')) return
  cardPress = { id: c.id, x0: e.clientX, y0: e.clientY, mode: null, ghost: null, targetId: null, dwellStart: 0, dwellPct: 0 }
  try { e.currentTarget.setPointerCapture(e.pointerId) } catch { /* capture is best-effort */ }
  window.addEventListener('pointermove', onCardPointerMove)
  window.addEventListener('pointerup', onCardPointerUp)
  window.addEventListener('pointercancel', onCardPointerCancel)
}
function pressCleanup() {
  window.removeEventListener('pointermove', onCardPointerMove)
  window.removeEventListener('pointerup', onCardPointerUp)
  window.removeEventListener('pointercancel', onCardPointerCancel)
  if (cardPress?.ghost) cardPress.ghost.remove()
  clearDwellRing()
  if (dwellRaf) { cancelAnimationFrame(dwellRaf); dwellRaf = null }
  cardPress = null
  mergeDragActive.value = false
  mergeDrag.value = { targetId: null, dwellPct: 0 }
}
function onCardPointerCancel() { pressCleanup() }
function onCardPointerUp() {
  const press = cardPress
  if (press && press.mode === 'merge' && press.targetId != null && press.dwellPct >= 100) {
    const sourceId = press.id
    const targetId = press.targetId
    pressCleanup()
    doMerge(sourceId, targetId)
    return
  }
  pressCleanup()
}
function onCardPointerMove(e) {
  const press = cardPress
  if (!press || press.mode === 'dragout') return
  if (!press.mode) {
    if (Math.hypot(e.clientX - press.x0, e.clientY - press.y0) < DRAG_OUT_PX) return
    // gesture becomes real — suppress the trailing click-paste and auto-hide
    press.mode = 'merge'
    clickSuppressed = true
    mergeDragActive.value = true
  }
  if (e.clientX < 0 || e.clientY < 0 || e.clientX > window.innerWidth || e.clientY > window.innerHeight) {
    // pointer left the panel → hand the gesture to the native OLE session
    press.mode = 'dragout'
    startDragOut(press.id, press)
    return
  }
  if (!press.ghost) {
    press.ghost = document.createElement('div')
    press.ghost.className = 'drag-ghost'
    const c = clips.value.find((x) => x.id === press.id)
    press.ghost.textContent = (c?.preview || '…').slice(0, 60)
    document.body.appendChild(press.ghost)
  }
  press.ghost.style.left = (e.clientX + 14) + 'px'
  press.ghost.style.top = (e.clientY + 10) + 'px'
  updateMergeTarget(e.clientX, e.clientY, press)
}
async function startDragOut(id, press) {
  dragOutActive.value = true
  if (press.ghost) { press.ghost.remove(); press.ghost = null }
  pressCleanup()
  try {
    const outcome = await invoke('start_drag_out', { id })
    if (outcome === 'cancelled') clickSuppressed = true // released without dropping: no paste
  } catch (err) {
    console.error(err)
    showToast(t('dragOutFail'))
  } finally {
    dragOutActive.value = false
    clickSuppressed = true
  }
}
function updateMergeTarget(px, py, press) {
  let best = null
  let bestD = Infinity
  document.querySelectorAll('.card[data-id]').forEach((el) => {
    const id = Number(el.dataset.id)
    if (id === press.id) return
    const r = el.getBoundingClientRect()
    if (px >= r.left - 8 && px <= r.right + 8 && py >= r.top - 8 && py <= r.bottom + 8) {
      const d = Math.hypot(px - (r.left + r.width / 2), py - (r.top + r.height / 2))
      if (d < bestD) { bestD = d; best = { el, id } }
    }
  })
  if (!best || best.id !== press.targetId) {
    clearDwellRing()
    if (dwellRaf) { cancelAnimationFrame(dwellRaf); dwellRaf = null }
    press.targetId = best ? best.id : null
    if (!best) { mergeDrag.value = { targetId: null, dwellPct: 0 }; return }
    best.el.classList.add('drop-ring')
    press.dwellStart = performance.now()
    press.dwellPct = 0
    mergeDrag.value = { targetId: best.id, dwellPct: 0 }
    const step = () => {
      const pct = Math.min(100, ((performance.now() - press.dwellStart) / MERGE_DWELL_MS) * 100)
      press.dwellPct = pct
      mergeDrag.value = { targetId: press.targetId, dwellPct: pct }
      best.el.style.setProperty('--dwell', pct)
      if (pct < 100 && cardPress === press) dwellRaf = requestAnimationFrame(step)
    }
    dwellRaf = requestAnimationFrame(step)
  }
}
function clearDwellRing() {
  document.querySelectorAll('.card.drop-ring').forEach((el) => {
    el.classList.remove('drop-ring')
    el.style.removeProperty('--dwell')
  })
}

// ---------- 0.2 merge / split / ungroup + one-step undo ----------
async function doMerge(sourceId, targetId) {
  pushUndo({ type: 'merge', sourceId, targetId })
  canUndo.value = true
  try {
    await invoke('merge_clips', { sourceId, targetId })
    showToast(t('mergeDone'))
  } catch (err) { console.error(err); showToast(t('pasteFail')) }
  refresh()
}
async function doSplit(c, e) {
  if (e) e.stopPropagation()
  const siblings = clips.value.filter((x) => x.group_id != null && x.group_id === c.group_id && x.id !== c.id)
  pushUndo({ type: 'split', id: c.id, siblingId: siblings[0]?.id ?? null })
  canUndo.value = true
  try {
    await invoke('split_clip', { id: c.id })
    showToast(t('splitDone'))
  } catch (err) { console.error(err); showToast(t('pasteFail')) }
  refresh()
}
async function doUngroup(block, e) {
  if (e) e.stopPropagation()
  pushUndo({ type: 'ungroup', memberIds: block.members.map((m) => m.id) })
  canUndo.value = true
  try {
    await invoke('ungroup', { groupId: block.groupId })
    showToast(t('ungroupDone'))
  } catch (err) { console.error(err); showToast(t('pasteFail')) }
  refresh()
}
async function onUndo() {
  const action = popUndo()
  canUndo.value = false
  for (const c of buildUndoCommands(action)) {
    try { await invoke(c.cmd, c.args) } catch (err) { console.error(err) }
  }
  showToast(t('undoDone'))
  refresh()
}

// ---------- 0.2 card view: group blocks inside the date sections ----------
function sectionBlocks(items) {
  const out = []
  for (let i = 0; i < items.length;) {
    const c = items[i]
    if (c.group_id != null) {
      const members = []
      let j = i
      while (j < items.length && items[j].group_id === c.group_id) { members.push(items[j]); j++ }
      out.push({ type: 'group', groupId: c.group_id, members })
      i = j
    } else {
      out.push({ type: 'clip', clip: c })
      i++
    }
  }
  return out
}
const sections = computed(() => grouped.value.map((g) => ({ label: g.label, blocks: sectionBlocks(g.items) })))
function isGroupSelected(b) {
  const s = flat.value[selected.value]
  return !!s && !expandedGroups.value.has(b.groupId) && b.members.some((m) => m.id === s.id)
}

function onKey(e) {
  const inField = e.target instanceof HTMLInputElement && e.target.type === 'text'
  // typing in search: only Delete is dangerous (deletes the selected clip);
  // arrows/Enter stay live for list navigation
  if (e.key === 'Escape') {
    if (showClearConfirm.value) { cancelClearAll(); return }
    if (showStats.value || showSettings.value) { disarmClear('escape'); closeOverlays(); return }
    if (query.value) { clearSearch(); return }
    invoke('hide_panel_cmd')
    return
  }
  if (showStats.value || showSettings.value) return
  // during a merge/drag-out gesture the arrow keys belong to nothing — block
  // them so the gesture never fights list navigation
  if ((mergeDragActive.value || dragOutActive.value) &&
      (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter' || e.key === 'Delete')) return
  if (inField && e.key === 'Delete') return
  if (e.key === 'ArrowDown') { e.preventDefault(); selected.value = Math.min(flat.value.length - 1, selected.value + 1) }
  else if (e.key === 'ArrowUp') { e.preventDefault(); selected.value = Math.max(0, selected.value - 1) }
  else if (e.key === 'Enter') { if (selected.value >= 0) onClick(flat.value[selected.value]) }
  else if (e.key === 'Delete') {
    if (selected.value >= 0) {
      const idx = selected.value
      onDelete(flat.value[idx], { stopPropagation() {} })
      nextTick(() => { selected.value = Math.min(idx, flat.value.length - 1) })
    }
  }
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    nextTick(() => document.querySelector('.row.sel')?.scrollIntoView({ block: 'nearest' }))
  }
}

let unlisteners = []
let zoomHoverCleanup = null
onMounted(async () => {
  // Clean up sensitive previews persisted by older releases (main + zoom share origin).
  localStorage.removeItem('cv-zoom-data')
  if (isZoomWin) {
    // this window is the hover preview: render payload, report hover state
    invoke('register_zoom').catch(console.error)
    unlisteners.push(await listen('zoom-data', (e) => { zoomData.value = e.payload }))
    // Ask the already-mounted main panel to replay the most recent hover.
    // This avoids a persistent localStorage copy of potentially private text.
    await emit('zoom-request').catch(() => {})
    const onEnter = () => emit('zoom-hover')
    const onLeave = () => emit('zoom-leave')
    window.addEventListener('mouseenter', onEnter)
    window.addEventListener('mouseleave', onLeave)
    zoomHoverCleanup = () => {
      window.removeEventListener('mouseenter', onEnter)
      window.removeEventListener('mouseleave', onLeave)
    }
    return
  }
  unlisteners.push(await listen('zoom-request', () => {
    if (latestZoomPayload) emitTo('zoom', 'zoom-data', latestZoomPayload).catch(() => {})
  }))
  { // persist panel height after edge/grip resize (debounced)
    let rt = null
    const unresize = await getCurrentWindow().listen('tauri://resize', ({ payload }) => {
      clearTimeout(rt)
      rt = setTimeout(() => {
        const h = payload.height / window.devicePixelRatio
        if (h >= 360 && h <= 900) invoke('save_panel_height', { h }).catch(console.error)
      }, 400)
    })
    unlisteners.push(unresize)
  }
  { // trailing debounce: terminal Ctrl+X bursts emit dozens of events/sec
    let rt = null
    const debounced = () => { clearTimeout(rt); rt = setTimeout(refresh, 120) }
    unlisteners.push(await listen('clips-changed', debounced))
  }
  unlisteners.push(await listen('panel-shown', () => {
    refresh()
    nextTick(() => document.querySelector('.search')?.focus())
  }))
  window.addEventListener('keydown', onKey)
  window.addEventListener('blur', onBlurDisarm)
  invoke('get_settings').then((st) => {
    settings.value = st
    pinned.value = !!st.panel_pinned
  }).catch(() => {})
  refresh()
  if (!isZoomWin) {
    // response-direction watchdog: if a heartbeat round-trip stops resolving
    // (IPC broken after display sleep — requests still go out, responses
    // never come back), reload the page to restore the pipe
    let pending = false
    const beat = () => {
      const t0 = performance.now()
      return invoke('heartbeat')
        .catch(() => {})
        .finally(() => {
          pending = false
          beatMs.value = Math.round(performance.now() - t0)
        })
    }
    setInterval(() => {
      if (pending) {
        console.warn('clipboard panel IPC stalled — reloading')
        location.reload()
        return
      }
      pending = true
      beat()
    }, 5000)
    beat()
    // zoom-window idle reaper (10 min without use → destroy, ~50MB back)
    setInterval(() => {
      if (!lastZoomUse || Date.now() - lastZoomUse < 10 * 60 * 1000) return
      const zw = WebviewWindow.getByLabel('zoom')
      if (zw) zw.close().catch(() => {})
      lastZoomUse = 0
    }, 60 * 1000)
  }
})
onBeforeUnmount(() => {
  unlisteners.forEach((u) => u())
  if (zoomHoverCleanup) zoomHoverCleanup()
  window.removeEventListener('keydown', onKey)
  window.removeEventListener('blur', onBlurDisarm)
})
</script>

<template>
  <div v-if="isZoomWin" class="zoomwin">
    <img v-if="zoomData && zoomData.type === 'image'" :src="zoomData.src" />
    <pre v-else-if="zoomData">{{ zoomData.text }}</pre>
  </div>
  <div v-else class="panel" :style="{ zoom: fontZoom }">
    <div class="topbar">
      <span class="dot"></span>
      <span class="brand">ClipVault</span>
      <button class="icon-btn pin-panel" :class="{ on: pinned }" :aria-pressed="pinned"
              :title="pinned ? t('unpinPanel') : t('pinPanel')" @click="togglePinned">
        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></svg>
      </button>
      <button v-if="canUndo" class="undo-chip" :title="t('undo')" @click="onUndo">{{ t('undo') }}</button>
      <button v-if="settings && settings.paused" class="paused-badge" @click="openSettings()" :title="t('setPauseHint')">{{ t('pausedTag') }}</button>
      <span class="hint">{{ t('copyHint') }}</span>
      <button class="icon-btn" :title="t('statsTitle')" @click="showStats ? closeOverlays() : openStats()">
        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 3v18h18"/><path d="M18 17V9"/><path d="M13 17V5"/><path d="M8 17v-3"/></svg>
      </button>
      <button class="icon-btn" :title="t('settingsTitle')" @click="showSettings ? closeOverlays() : openSettings()">
        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 7h-9"/><path d="M14 17H5"/><circle cx="17" cy="17" r="3"/><circle cx="7" cy="7" r="3"/></svg>
      </button>
      <button class="icon-btn" :title="t('theme')" @click="toggleTheme">
        <svg v-if="theme==='dark'" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>
        <svg v-else viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/></svg>
      </button>
      <select class="icon-btn lang-btn" :title="t('lang')" :value="lang" @change="setLang($event.target.value)"
        :aria-label="t('lang')">
  <option value="zh">中文</option><option value="en">EN</option><option value="ja">日本語</option>
  <option value="ko">한국어</option><option value="es">ES</option><option value="fr">FR</option><option value="de">DE</option>
</select>
    </div>

    <div class="search-row" v-if="!showStats && !showSettings">
      <input class="search" v-model="query" @input="onSearch" @focus="searchFocused = true" @blur="searchFocused = false" :placeholder="t('search')" />
      <button v-if="query" class="search-clear" @click="clearSearch" :title="t('clear')" :aria-label="t('clear')">
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
      </button>
    </div>

    <div class="tabs" v-if="!showStats && !showSettings">
      <button v-for="f in ['all','text','image','link','file','pinned']" :key="f"
              class="tab" :class="{ active: filter === f }" @click="setFilter(f)">
        {{ t(f) }}
      </button>
    </div>

    <div class="list" role="listbox" :aria-label="t('all')"
         :aria-activedescendant="selected >= 0 && flat[selected] ? 'clip-' + flat[selected].id : undefined"
         v-if="!showStats && !showSettings">
      <div v-if="clips.length === 0" class="empty">
      <div class="empty-title">{{ query.trim() ? t('noMatch') : t('empty') }}</div>
      <div class="empty-hint" v-if="query.trim()">{{ t('noMatchHint') }}</div>
      <div class="empty-hint" v-else-if="settings && settings.paused">{{ t('setPauseHint') }}</div>
      <div class="empty-hint" v-else>{{ t('emptyHint') }}</div>
      </div>
      <template v-for="sec in sections" :key="sec.label">
        <div class="group-label" role="presentation">{{ sec.label }}</div>
        <template v-for="b in sec.blocks" :key="b.type === 'clip' ? b.clip.id : 'g' + b.groupId">
          <!-- plain card -->
          <div v-if="b.type === 'clip'" class="card" :data-id="b.clip.id"
               role="option" :id="'clip-' + b.clip.id"
               :aria-selected="flat[selected] && flat[selected].id === b.clip.id"
               :aria-label="b.clip.kind === 'image' ? b.clip.preview : undefined"
               :class="{ sel: flat[selected] && flat[selected].id === b.clip.id,
                         pinnedcard: b.clip.pinned, drop: mergeDrag.targetId === b.clip.id }"
               @pointerdown="onCardPointerDown(b.clip, $event)"
               @click="onClick(b.clip)"
               @mouseenter="rowEnter(b.clip, $event)" @mouseleave="rowLeave">
            <div class="card-top">
              <span class="kind-badge" :class="'k-' + (isLinkClip(b.clip) ? 'link' : b.clip.kind)">{{ clipKindLabel(b.clip) }}</span>
              <span v-if="isLinkClip(b.clip) && hostOf(b.clip)" class="host-badge">{{ hostOf(b.clip) }}</span>
              <span v-if="b.clip.pinned" class="pin-flag" :title="t('pin')">📌</span>
              <span class="spacer"></span>
              <button class="icon-btn pin" :class="{ on: b.clip.pinned }" @click="onPin(b.clip, $event)"
                      :title="b.clip.pinned ? t('unpin') : t('pin')">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></svg>
              </button>
              <button class="icon-btn del" @click="onDelete(b.clip, $event)" :title="t('del')">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><path d="M10 11v6"/><path d="M14 11v6"/></svg>
              </button>
            </div>
            <img v-if="b.clip.kind === 'image' && !broken.has(b.clip.id)" class="thumb" :src="thumb(b.clip)" loading="lazy" decoding="async" @error="thumbFallback(b.clip, $event)" />
            <span v-else-if="b.clip.kind === 'image'" class="thumb broken">{{ t('expired') }}</span>
            <div v-else-if="isCodeClip(b.clip)" class="card-body code preview">{{ (b.clip.preview || '').replace(/^\s+/, '') }}</div>
            <span v-else class="card-body preview">{{ (b.clip.preview || '').replace(/^\s+/, '') }}</span>
            <span v-if="mergeDrag.targetId === b.clip.id" class="drop-hint">{{ t('mergeDrop') }}</span>
          </div>
          <!-- group card (non-destructive logical grouping) -->
          <div v-else class="card group" :class="{ sel: isGroupSelected(b), drop: mergeDrag.targetId === b.members[0]?.id }"
               role="presentation" @click="toggleGroup(b.groupId)">
            <div class="card-top">
              <span class="kind-badge k-group">{{ t('group') }}</span>
              <span class="group-count">{{ t('groupItems').replace('{n}', String(b.members.length)) }}</span>
              <span class="spacer"></span>
              <button class="icon-btn del" @click.stop="doUngroup(b, $event)" :title="t('ungroup')">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><path d="M10 11v6"/><path d="M14 11v6"/></svg>
              </button>
            </div>
            <div class="stack-thumbs">
              <span v-for="m in b.members.slice(0, 3)" :key="m.id">{{ (m.preview || '').slice(0, 12) }}</span>
            </div>
            <div class="group-hint">{{ expandedGroups.has(b.groupId) ? t('groupCollapse') : t('groupExpand') }}</div>
            <div v-if="expandedGroups.has(b.groupId)" class="group-children">
              <div v-for="m in b.members" :key="m.id" class="child"
                   role="option" :id="'clip-' + m.id"
                   :aria-selected="flat[selected] && flat[selected].id === m.id"
                   :class="{ sel: flat[selected] && flat[selected].id === m.id }"
                   @click.stop="onClick(m)">
                <span class="kind-badge" :class="'k-' + (isLinkClip(m) ? 'link' : m.kind)">{{ clipKindLabel(m) }}</span>
                <span class="child-body">{{ (m.preview || '').replace(/^\s+/, '') }}</span>
                <button class="icon-btn pin" :class="{ on: m.pinned }" @click.stop="onPin(m, $event)" :title="m.pinned ? t('unpin') : t('pin')">
                  <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></svg>
                </button>
                <button class="icon-btn del" @click.stop="doSplit(m, $event)" :title="t('dragOut')">
                  <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m16 3 4 4-4 4"/><path d="M20 7H5"/><path d="M8 21l-4-4 4-4"/><path d="M4 17h15"/></svg>
                </button>
              </div>
            </div>
          </div>
        </template>
      </template>
    </div>

    <div class="list" v-else-if="showStats">
      <div v-if="stats" class="stats">
        <div class="stat-grid">
          <div class="stat-card"><div class="num">{{ stats.total }}</div><div class="lbl">{{ t('statTotal') }}</div></div>
          <div class="stat-card"><div class="num">{{ stats.today }}</div><div class="lbl">{{ t('statToday') }}</div></div>
          <div class="stat-card"><div class="num">{{ stats.text }}</div><div class="lbl">{{ t('statText') }}</div></div>
          <div class="stat-card"><div class="num">{{ stats.image }}</div><div class="lbl">{{ t('statImage') }}</div></div>
          <div class="stat-card"><div class="num">{{ stats.link }}</div><div class="lbl">{{ t('statLink') }}</div></div>
          <div class="stat-card"><div class="num">{{ stats.file }}</div><div class="lbl">{{ t('statFile') }}</div></div>
          <div class="stat-card"><div class="num">{{ stats.pinned }}</div><div class="lbl">{{ t('statPinned') }}</div></div>
          <div class="stat-card"><div class="num">{{ stats.paste_total }}</div><div class="lbl">{{ t('statPastes') }}</div></div>
        </div>
        <div class="sec">{{ t('statDaily') }}</div>
        <div class="bars">
          <div class="bar-col" v-for="d in stats.daily" :key="d[0]">
            <div class="bar" :style="{ height: (d[1] / dailyMax * 72) + 'px' }" :title="d[0] + ': ' + d[1]"></div>
            <div class="bar-lbl">{{ d[0].slice(5) }}</div>
          </div>
        </div>
        <div class="sec">{{ t('statTop') }}</div>
        <div class="top-row" v-for="tp in stats.top" :key="tp[0]">
          <span class="preview">{{ tp[0] }}</span>
          <span class="cnt">{{ tp[1] }}{{ t('times') }}</span>
        </div>
      </div>
    </div>

    <div class="list" v-else>
      <div class="settings">
        <div class="group-label">{{ t('settingsTitle') }}</div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">{{ t('setLimit') }}</span>
            <span class="set-value">{{ settings.history_limit }}</span>
          </div>
          <input class="set-slider" type="range" v-model.number="settings.history_limit" min="50" max="5000" step="50" />
          <div class="set-hint">{{ t('setLimitHint') }}</div>
        </div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">{{ t('setAuto') }}</span>
            <label class="switch">
              <input type="checkbox" v-model="settings.autostart" :aria-label="t('setAuto')" />
              <span class="track"><span class="knob"></span></span>
            </label>
          </div>
          <div class="set-hint">{{ t('setAutoHint') }}</div>
        </div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">{{ t('setPause') }}</span>
            <label class="switch">
              <input type="checkbox" v-model="settings.paused" :aria-label="t('setPause')" />
              <span class="track"><span class="knob"></span></span>
            </label>
          </div>
          <div class="set-hint">{{ t('setPauseHint') }}</div>
        </div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">{{ t('setEdgePeek') }}</span>
            <label class="switch">
              <input type="checkbox" v-model="settings.edge_peek_enabled" :aria-label="t('setEdgePeek')" />
              <span class="track"><span class="knob"></span></span>
            </label>
          </div>
          <div class="set-hint">{{ t('setEdgePeekHint') }}</div>
        </div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">{{ t('setPanelPinned') }}</span>
            <label class="switch">
              <input type="checkbox" v-model="settings.panel_pinned" :aria-label="t('setPanelPinned')"
                     @change="pinned = settings.panel_pinned" />
              <span class="track"><span class="knob"></span></span>
            </label>
          </div>
          <div class="set-hint">{{ t('setPanelPinnedHint') }}</div>
        </div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">{{ t('setImage') }}</span>
            <label class="switch">
              <input type="checkbox" v-model="settings.image_capture" :aria-label="t('setImage')" />
              <span class="track"><span class="knob"></span></span>
            </label>
          </div>
          <div class="set-hint">{{ t('setImageHint') }}</div>
        </div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">{{ t('setExclude') }}</span>
          </div>
          <input class="set-input" v-model="excludedApps" :placeholder="t('setExcludePh')"
                 spellcheck="false" :aria-label="t('setExclude')" />
          <div class="set-hint">{{ t('setExcludeHint') }}</div>
        </div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">{{ t('setResume') }}</span>
            <span class="set-value">{{ settings.auto_resume_minutes }}</span>
          </div>
          <input class="set-slider" type="range" v-model.number="settings.auto_resume_minutes" min="0" max="60" step="5" />
          <div class="set-hint">{{ t('setResumeHint') }}</div>
        </div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">{{ t('clearAll') }}</span>
            <button class="clear-btn" @click="onClearAll">{{ t('clearBtn') }}</button>
          </div>
          <div class="set-hint">{{ t('clearAllHint') }}</div>
        </div>
        <div class="group-label" style="margin-top: 14px;">{{ t('shortcut') }}</div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">ClipVault</span>
            <input class="set-input" v-model="settings.hotkey" :placeholder="t('setHotkey')"
                   spellcheck="false" :aria-label="t('setHotkey')" />
          </div>
          <div class="set-hint">{{ t('setHotkeyHint') }}</div>
        </div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">{{ t('setFont') }}</span>
            <span class="set-value">{{ ['100%', '110%', '125%', '150%'][fontStep] }}</span>
          </div>
          <input class="set-slider" type="range" min="0" max="3" step="1"
                 :value="fontStep" @input="setFontStep(parseInt($event.target.value, 10))"
                 :aria-label="t('setFont')" />
          <div class="set-hint">{{ t('setFontHint') }}</div>
        </div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">{{ t('setSensitive') }}</span>
            <label class="switch">
              <input type="checkbox" v-model="settings.sensitive_filter_enabled" :aria-label="t('setSensitive')" />
              <span class="track"><span class="knob"></span></span>
            </label>
          </div>
          <input class="set-input" v-model="sensitiveText" :placeholder="t('setSensitivePh')"
                 spellcheck="false" :aria-label="t('setSensitive')" />
          <div class="set-hint">{{ t('setSensitiveHint') }}</div>
        </div>
        <div class="about">{{ t('about') }} ClipVault v0.1.11 · MIT</div>
      </div>
    </div>

    <div v-if="!isZoomWin" class="health">{{ health }}</div>
    <div v-if="!isZoomWin" class="grip" @mousedown.prevent="startResize($event)" aria-hidden="true">
      <span></span>
    </div>

  <transition name="fade">
    <div v-if="showClearConfirm" class="modal-backdrop" @click.self="cancelClearAll">
      <div class="modal" role="alertdialog" :aria-label="t('clearAll')">
        <div class="modal-title">{{ t('clearTitle') }}</div>
        <div class="modal-body">{{ t('clearAllHint') }}</div>
        <div class="modal-actions">
          <button class="modal-btn" @click="cancelClearAll">{{ t('modalCancel') }}</button>
          <button class="modal-btn modal-danger" @click="confirmClearAll">{{ t('modalDelete') }}</button>
          </div>
        </div>
      </div>
    </transition>
    <transition name="fade">
      <div v-if="toast" class="toast" role="status">{{ toast }}</div>
  </transition>
  </div>
</template>

<style scoped>
.panel { display: flex; flex-direction: column; height: 100vh; background: var(--bg); }
.topbar { display: flex; align-items: center; gap: 8px; padding: 8px 10px 4px; }
.dot { width: 9px; height: 9px; border: 2px solid var(--fg); border-radius: 3px; }
.brand { font-weight: 700; font-size: 13px; font-family: 'Noto Serif SC', 'Source Han Serif SC', SimSun, serif; letter-spacing: .02em; }
.hint { flex: 1; min-width: 40px; color: var(--fg-dim); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.icon-btn { border: 0; background: transparent; color: var(--fg-dim); width: 26px; height: 26px; border-radius: 6px; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; }
.icon-btn:hover { background: var(--bg-hover); color: var(--fg); }
.lang-btn { width: auto; padding: 0 4px; font-size: 11px; cursor: pointer; background: var(--bg-2); color: var(--fg); border: 1.5px solid var(--border); }
.lang-btn option { background: var(--bg); color: var(--fg); }
.modal-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,.45); display: flex; align-items: center; justify-content: center; z-index: 60; }
.modal { background: var(--bg); color: var(--fg); border: 1.5px solid var(--border); border-radius: 10px; padding: 16px 18px; width: min(320px, 86%); box-shadow: 0 12px 40px rgba(0,0,0,.35); }
.modal-title { font-weight: 700; font-size: 14px; margin-bottom: 8px; }
.modal-body { font-size: 12px; color: var(--fg-dim); line-height: 1.6; margin-bottom: 14px; }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; }
.modal-btn { border: 1.5px solid var(--border); background: var(--bg-2); color: var(--fg); border-radius: 6px; padding: 5px 12px; font-size: 12px; cursor: pointer; }
.modal-btn:hover { background: var(--bg-hover); }
.modal-danger { border-color: #c0392b; background: #c0392b; color: #fff; }
.modal-danger:hover { background: #a93226; }
.search-row { padding: 4px 10px; position: relative; }
.search-clear { position: absolute; right: 18px; top: 50%; transform: translateY(-50%); border: 0; background: transparent; color: var(--fg-dim); width: 22px; height: 22px; border-radius: 6px; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; }
.search-clear:hover { background: var(--bg-hover); color: var(--fg); }
.search { width: 100%; padding: 7px 30px 7px 10px; background: var(--bg-2); color: var(--fg); border: 1.5px solid var(--border); border-radius: 5px; outline: none; font-size: 13px; }
.search:focus { border-color: var(--accent); caret-color: var(--accent); }
.tabs { display: flex; gap: 4px; padding: 4px 10px; flex-wrap: wrap; }
.tab { border: 0; background: transparent; color: var(--fg-dim); padding: 4px 12px; border-radius: 999px; cursor: pointer; font-size: 12px; }
.tab:hover { background: var(--bg-hover); }
.tab.active { background: var(--fg); color: var(--bg); }
.list { flex: 1; overflow-y: auto; padding: 2px 6px 8px; display: flex; flex-direction: column; gap: 6px; }
.empty { color: var(--fg-dim); text-align: center; margin-top: 56px; }
.empty-title { font-size: 15px; font-weight: 700; font-family: 'Noto Serif SC', 'Source Han Serif SC', SimSun, serif; }
.empty-hint { font-size: 11px; margin-top: 6px; opacity: .8; padding: 0 30px; }
.group-label { font-size: 10px; color: var(--fg-dim); padding: 8px 8px 2px; }

/* ---------- 0.2 cards (prototype cv02-prototype.html) ---------- */
.card {
  position: relative; border-radius: 10px; padding: 8px 10px; cursor: grab;
  background: var(--glass-2); border: 1px solid var(--glass-border);
  transition: transform .18s var(--spring), box-shadow .2s, border-color .2s, background .2s;
  touch-action: none;
}
@keyframes cardin { from { opacity: 0; transform: translateY(3px); } to { opacity: 1; transform: none; } }
.card { animation: cardin 120ms var(--spring); }
.card:hover { transform: translateY(-1px); border-color: rgba(90, 162, 232, .45); background: var(--glass-hover); }
.card.sel { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent-dim) inset; }
.card.pinnedcard { border-left: 3px solid var(--pin); }
.card:active { cursor: grabbing; }
/* 600ms dwell ring (merge arm): conic sweep driven by --dwell (0-100) */
.card.drop-ring { border-color: var(--accent) !important;
  background-image: conic-gradient(from -90deg, rgba(90, 162, 232, .35) calc(var(--dwell, 0) * 1%), transparent 0); }
.drop-hint { position: absolute; right: 8px; bottom: 6px; font-size: 10px; color: var(--glass-accent-ink); }
.card-top { display: flex; align-items: center; gap: 6px; margin-bottom: 4px; }
.kind-badge { flex-shrink: 0; font-size: 10px; padding: 1px 6px; border-radius: 5px; background: var(--glass-border); color: var(--fg-dim); }
.kind-badge.k-image { background: rgba(90, 162, 232, .18); color: var(--glass-accent-ink); }
.kind-badge.k-link { background: rgba(163, 126, 231, .18); color: #c9b1f2; }
.kind-badge.k-file { background: rgba(94, 182, 130, .18); color: #9fd8b5; }
.kind-badge.k-group { background: rgba(227, 197, 62, .18); color: #ecdf94; }
.host-badge { font-size: 10px; padding: 1px 6px; border-radius: 5px; background: var(--glass-border); color: var(--fg-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 45%; }
.pin-flag { font-size: 10px; }
.card .spacer { flex: 1; }
.card-top .icon-btn { width: 22px; height: 22px; opacity: 0; transition: opacity .15s; }
.card:hover .card-top .icon-btn, .card.sel .card-top .icon-btn { opacity: 1; }
.card-body { font-size: 12px; line-height: 1.45; color: var(--fg); max-height: 40px; overflow: hidden; flex: 1; min-width: 0; white-space: nowrap; text-overflow: ellipsis; }
.card-body.code { font-family: 'Cascadia Code', Consolas, monospace; font-size: 11px; white-space: pre-wrap; background: rgba(0, 0, 0, .18); border-radius: 6px; padding: 6px 8px; max-height: 58px; }
.thumb { height: 44px; max-width: 100%; width: auto; object-fit: cover; border-radius: 8px; border: 1px solid var(--border); }
.thumb.broken { display: inline-flex; align-items: center; justify-content: center;
  width: 64px; height: 44px; padding: 0 8px; font-size: 10px;
  color: var(--fg-dim); background: var(--glass-2); border: 1px dashed var(--border); border-radius: 8px; }
/* group card */
.card.group { background: rgba(227, 197, 62, .07); border-color: rgba(227, 197, 62, .35); }
.group-count { font-size: 11px; font-weight: 600; }
.group-hint { font-size: 10px; color: var(--fg-dim); }
.stack-thumbs { display: flex; gap: 4px; margin: 2px 0 4px; }
.stack-thumbs span { height: 26px; flex: 1; border-radius: 6px; background: var(--glass-2);
  border: 1px solid var(--glass-border); font-size: 9px; color: var(--fg-dim);
  display: flex; align-items: center; justify-content: center; overflow: hidden; padding: 0 4px; white-space: nowrap; }
.group-children { display: flex; flex-direction: column; gap: 5px; margin-top: 6px; }
.child { border-radius: 8px; padding: 5px 8px; font-size: 11px; background: var(--glass-2);
  border: 1px solid var(--glass-border); cursor: pointer; display: flex; gap: 6px; align-items: center; }
.child:hover { border-color: rgba(90, 162, 232, .45); }
.child.sel { border-color: var(--accent); }
.child .child-body { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--fg); }
.child .icon-btn { width: 20px; height: 20px; opacity: 0; transition: opacity .15s; }
.child:hover .icon-btn { opacity: 1; }
/* drag ghost (in-panel merge drag) */
.drag-ghost { position: fixed; z-index: 999; pointer-events: none; width: 220px;
  border-radius: 10px; padding: 9px 10px; font-size: 12px;
  background: var(--glass); border: 1px solid rgba(90, 162, 232, .6);
  box-shadow: 0 16px 40px rgba(0, 0, 0, .35); color: var(--fg); transform: scale(1.03) rotate(-1.2deg);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.icon-btn.pin.on { color: var(--pin); }
.icon-btn.del:hover { color: var(--danger); }
.icon-btn.pin-panel.on { color: var(--accent); background: var(--glass-hover); }
.undo-chip { border: 1.5px solid var(--accent); color: var(--glass-accent-ink); background: transparent;
  border-radius: 999px; padding: 1px 10px; font-size: 11px; cursor: pointer; font-weight: 600; flex-shrink: 0; }
.undo-chip:hover { background: var(--glass-hover); }
.stats { padding: 4px 8px; }
.stat-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
.stat-card { background: var(--bg-2); border: 1px solid var(--border); border-radius: 6px; padding: 8px 4px; text-align: center; }
.stat-card .num { font-size: 16px; font-weight: 600; color: var(--accent); }
.stat-card .lbl { font-size: 10px; color: var(--fg-dim); margin-top: 2px; }
.sec { margin: 12px 2px 6px; font-size: 11px; color: var(--fg-dim); }
.bars { display: flex; align-items: flex-end; gap: 8px; height: 92px; padding: 0 4px; }
.bar-col { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: flex-end; gap: 4px; height: 100%; }
.bar { width: 100%; max-width: 26px; background: var(--accent); border-radius: 4px 4px 0 0; min-height: 2px; opacity: .85; }
.bar-lbl { font-size: 9px; color: var(--fg-dim); }
.top-row { display: flex; align-items: center; gap: 8px; padding: 4px 2px; }
.preview { min-width: 0; font-size: 12px; line-height: 1.45; }
.top-row .preview { flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; -webkit-line-clamp: 1; }
.top-row .cnt { color: var(--fg-dim); font-size: 11px; }
.settings { padding: 6px 10px 12px; display: flex; flex-direction: column; gap: 8px; }
.set-card { background: var(--bg-2); border: 1px solid var(--border); border-radius: 8px; padding: 10px 12px; }
.set-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
.set-name { font-size: 12px; font-weight: 600; }
.set-value { font-size: 12px; font-variant-numeric: tabular-nums; color: var(--accent); font-weight: 600; }
.set-hint { font-size: 10.5px; color: var(--fg-dim); margin-top: 6px; line-height: 1.4; }
.set-slider { width: 100%; margin-top: 8px; accent-color: var(--accent); height: 4px; cursor: pointer; }
.set-input { width: 100%; margin-top: 8px; padding: 6px 8px; background: var(--bg); color: var(--fg); border: 1px solid var(--border); border-radius: 6px; outline: none; font-size: 12px; }
.set-input:focus { border-color: var(--accent); caret-color: var(--accent); }
.set-head .set-input { width: auto; min-width: 120px; margin-top: 0; text-align: right; }
.paused-badge { flex-shrink: 0; white-space: nowrap; border: 1.5px solid var(--danger); color: var(--danger); background: transparent; border-radius: 999px; padding: 1px 10px; font-size: 11px; cursor: pointer; font-weight: 600; }
.paused-badge:hover { background: var(--danger); color: var(--danger-ink); }
.clear-btn { border: 1.5px solid var(--danger); color: var(--danger); background: transparent; border-radius: 5px; padding: 4px 12px; font-size: 12px; cursor: pointer; font-weight: 600; }
.clear-btn:hover { background: var(--danger); color: var(--danger-ink); }
.clear-btn.armed { background: var(--danger); color: var(--danger-ink); animation: pulse 1s ease-in-out infinite; }
@keyframes pulse { 50% { opacity: .6; } }
@media (prefers-reduced-motion: reduce) {
  .card { animation: none; transition: none; }
  .card-top .icon-btn, .child .icon-btn { transition: none; }
  .clear-btn.armed { animation: none; }
}
.switch { position: relative; display: inline-block; cursor: pointer; }
.switch input { position: absolute; opacity: 0; inset: 0; margin: 0; cursor: pointer; }
.track { display: block; width: 36px; height: 20px; border-radius: 999px; background: var(--border); transition: background .15s ease; position: relative; }
.knob { position: absolute; top: 2px; left: 2px; width: 16px; height: 16px; border-radius: 50%; background: var(--bg); border: 1px solid var(--border); transition: transform .15s ease; box-shadow: 0 1px 2px var(--accent-dim); }
.switch input:checked + .track { background: var(--accent); }
.switch input:checked + .track .knob { transform: translateX(16px); }
.switch input:focus-visible + .track { outline: 2px solid var(--accent); outline-offset: 2px; }
.kbd { font-family: inherit; font-size: 11px; background: var(--bg); border: 1px solid var(--border); border-bottom-width: 2px; border-radius: 5px; padding: 2px 8px; color: var(--fg); }
.about { text-align: center; font-size: 10px; color: var(--fg-dim); margin-top: 10px; }
.zoomwin { width: 100vw; height: 100vh; overflow: auto; background: var(--bg-2); border: 1px solid var(--border); border-radius: 10px; padding: 10px; }
.zoomwin img { max-width: 100%; display: block; border-radius: 6px; margin: 0 auto; }
.zoomwin pre { margin: 0; white-space: pre-wrap; word-break: break-all; font-family: inherit; font-size: 12px; line-height: 1.5; user-select: text; cursor: text; }
.pop-enter-active, .pop-leave-active { transition: opacity .12s ease; }
.pop-enter-from, .pop-leave-to { opacity: 0; }
.health { font-size: 9px; color: var(--fg-dim); text-align: right; padding: 0 10px 2px; opacity: .85; flex-shrink: 0; }
.grip { height: 14px; display: flex; align-items: center; justify-content: center; cursor: ns-resize; flex-shrink: 0; }
.grip span { width: 36px; height: 4px; border-radius: 2px; background: var(--border); }
.grip:hover span { background: var(--fg-dim); }
.toast { position: fixed; bottom: 14px; left: 50%; transform: translateX(-50%); background: var(--fg); color: var(--bg); padding: 7px 16px; border-radius: 5px; font-size: 12px; max-width: 90%; z-index: 60; font-family: 'Cascadia Code', Consolas, monospace; }
.fade-enter-active, .fade-leave-active { transition: opacity .2s; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
::-webkit-scrollbar { width: 8px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--fg-dim); border-radius: 4px; }
::-webkit-scrollbar-thumb:hover { background: var(--fg); }
</style>
