<script setup>
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen, emit } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi'

const isZoomWin = getCurrentWindow().label === 'zoom'

// surface JS errors to the backend log — a blank panel must never be a mystery
window.addEventListener('error', (e) => {
  invoke('report_error', { msg: `${String(e.message)} @${e.filename}:${e.lineno}` }).catch(() => {})
})
window.addEventListener('unhandledrejection', (e) => {
  invoke('report_error', { msg: `unhandled: ${String(e.reason)}` }).catch(() => {})
})

const i18n = {
  zh: {
    all:'全部', text:'文本', image:'图片', pinned:'置顶', link:'链接', file:'文件',
    search:'搜索剪贴板…', empty:'暂无内容', emptyHint:'复制任意内容（文本/图片/文件）后自动出现在这里',
    copyHint:'点击条目粘贴 · ↑↓ 选择 · Enter 粘贴 · Del 删除', theme:'主题', lang:'语言',
    statsTitle:'统计', settingsTitle:'设置',
    statTotal:'总条目', statText:'文本', statImage:'图片', statLink:'链接', statFile:'文件',
    statPinned:'置顶', statToday:'今日新增', statPastes:'累计粘贴', statDaily:'近 7 天复制量',
    statTop:'最常粘贴 Top 5', times:'次',
    setLimit:'历史上限', setLimitHint:'超出后自动清理最旧记录（置顶除外）', setAuto:'开机自动启动', setAutoHint:'登录 Windows 后在后台静默运行', shortcut:'呼出快捷键', about:'关于', save:'保存', saved:'设置已保存',
    focusLost:'已复制到剪贴板，但无法聚焦原窗口，请手动 Ctrl+V', pasteFail:'粘贴失败，内容仍在剪贴板', dayToday:'今天', dayYesterday:'昨天', dayEarlier:'更早',
  },
  en: {
    all:'All', text:'Text', image:'Image', pinned:'Pinned', link:'Links', file:'Files',
    search:'Search clipboard…', empty:'Nothing here yet', emptyHint:'Copy anything (text / image / files) and it shows up here',
    copyHint:'Click to paste · ↑↓ select · Enter paste · Del delete', theme:'Theme', lang:'Lang',
    statsTitle:'Stats', settingsTitle:'Settings',
    statTotal:'Total', statText:'Text', statImage:'Images', statLink:'Links', statFile:'Files',
    statPinned:'Pinned', statToday:'Today', statPastes:'Pastes', statDaily:'Last 7 days',
    statTop:'Top 5 pasted', times:'×',
    setLimit:'History limit', setLimitHint:'Oldest unpinned items are cleaned up beyond the limit', setAuto:'Launch at startup', setAutoHint:'Runs quietly in the background after sign-in', shortcut:'Summon shortcut', about:'About', save:'Save', saved:'Settings saved',
    focusLost:'Copied to clipboard, but could not focus the previous window — press Ctrl+V manually', pasteFail:'Paste failed, content stays on clipboard', dayToday:'Today', dayYesterday:'Yesterday', dayEarlier:'Earlier',
  },
}
const lang = ref(localStorage.getItem('cv-lang') || 'zh')
const t = (k) => i18n[lang.value][k] ?? k
function toggleLang() {
  lang.value = lang.value === 'zh' ? 'en' : 'zh'
  localStorage.setItem('cv-lang', lang.value)
}

const theme = ref(localStorage.getItem('cv-theme') || 'dark')
function applyTheme() { document.documentElement.dataset.theme = theme.value }
function toggleTheme() {
  theme.value = theme.value === 'dark' ? 'light' : 'dark'
  localStorage.setItem('cv-theme', theme.value)
  applyTheme()
}
applyTheme()

const health = ref('...')
let beatMs = ref(0)
const filter = ref('all')
const query = ref('')
const clips = ref([])
const showStats = ref(false)
const stats = ref(null)
const showSettings = ref(false)
const settings = ref({ history_limit: 1000, panel_height: 540, autostart: false })
const toast = ref('')
let toastTimer = null
let searchTimer = null
const selected = ref(-1)

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
  const out = []
  let cur = null
  for (const c of clips.value) {
    const g = groupOf(c)
    if (!cur || cur.label !== g) { cur = { label: g, items: [] }; out.push(cur) }
    cur.items.push(c)
  }
  return out
})

let reqId = 0
async function refresh() {
  const id = ++reqId
  const r = await invoke('list_clips', { filter: filter.value, query: query.value.trim() })
  if (id !== reqId) return // a newer request already superseded this one
  clips.value = r
  selected.value = -1
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
  stats.value = await invoke('stats')
}
function closeOverlays() { showStats.value = false; showSettings.value = false }
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
    } catch (e) { console.error(e) }
  }, 500)
}, { deep: true })
async function openSettings() {
  closeOverlays()
  showSettings.value = true
  settingsLoaded = false
  settings.value = await invoke('get_settings')
  await nextTick()
  settingsLoaded = true
}
const flat = computed(() => grouped.value.flatMap((g) => g.items))
watch([clips, beatMs], () => {
  health.value = `${clips.value.length} items · ipc ${beatMs.value}ms`
}, { immediate: true })

function thumb(c) { return c.image_path ? convertFileSrc(c.image_path) : '' }

async function onPin(c, e) { e.stopPropagation(); await invoke('toggle_pin', { id: c.id }); refresh() }
async function onDelete(c, e) {
  e.stopPropagation()
  await invoke('delete_clip', { id: c.id })
  refresh()
}
async function onClick(c) {
  try { await invoke('paste_clip', { id: c.id }) }
  catch (err) {
    const msg = String(err ?? '')
    if (msg.includes('FOCUS_LOST')) showToast(t('focusLost'))
    else showToast(t('pasteFail'))
  }
}

const zoom = ref(null)
const zoomData = ref(null)
let hideTimer = null
function cancelHide() { clearTimeout(hideTimer) }
// closing the popup is owned by the Rust cursor watchdog; the frontend
// never hides it on hover events (event-timing races caused false closes)
function scheduleHide() {}
async function openZoom(c, e) {
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
  await emit('zoom-data', payload)
  const zw = await WebviewWindow.getByLabel('zoom')
  if (zw) {
    await zw.setPosition(new LogicalPosition(x, top))
    await zw.show()
  }
}
function rowEnter(c, e) { openZoom(c, e) }
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

function onKey(e) {
  const inField = e.target instanceof HTMLInputElement && e.target.type === 'text'
  // typing in search: only Delete is dangerous (deletes the selected clip);
  // arrows/Enter stay live for list navigation
  if (e.key === 'Escape') {
    if (showStats.value || showSettings.value) { closeOverlays(); return }
    if (query.value) { clearSearch(); return }
    invoke('hide_panel_cmd')
    return
  }
  if (showStats.value || showSettings.value) return
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
onMounted(async () => {
  if (isZoomWin) {
    // this window is the hover preview: render payload, report hover state
    unlisteners.push(await listen('zoom-data', (e) => { zoomData.value = e.payload }))
    window.addEventListener('mouseenter', () => emit('zoom-hover'))
    window.addEventListener('mouseleave', () => emit('zoom-leave'))
    return
  }
  { // persist panel height after edge/grip resize (debounced)
    let rt = null
    await getCurrentWindow().listen('tauri://resize', ({ payload }) => {
      clearTimeout(rt)
      rt = setTimeout(() => {
        const h = payload.height / window.devicePixelRatio
        if (h >= 360 && h <= 900) invoke('save_panel_height', { h }).catch(console.error)
      }, 400)
    })
  }
  unlisteners.push(await listen('clips-changed', refresh))
  unlisteners.push(await listen('panel-shown', refresh))
  window.addEventListener('keydown', onKey)
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
  }
})
onBeforeUnmount(() => {
  unlisteners.forEach((u) => u())
  window.removeEventListener('keydown', onKey)
})
</script>

<template>
  <div v-if="isZoomWin" class="zoomwin">
    <img v-if="zoomData && zoomData.type === 'image'" :src="zoomData.src" />
    <pre v-else-if="zoomData">{{ zoomData.text }}</pre>
  </div>
  <div v-else class="panel">
    <div class="topbar">
      <span class="dot"></span>
      <span class="brand">ClipVault</span>
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
      <button class="icon-btn lang-btn" :title="t('lang')" @click="toggleLang">{{ lang === 'zh' ? 'EN' : '中' }}</button>
    </div>

    <div class="search-row" v-if="!showStats && !showSettings">
      <input class="search" v-model="query" @input="onSearch" :placeholder="t('search')" />
      <button v-if="query" class="search-clear" @click="clearSearch" title="Clear">
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
      </button>
    </div>

    <div class="tabs" v-if="!showStats && !showSettings">
      <button v-for="f in ['all','text','image','link','file','pinned']" :key="f"
              class="tab" :class="{ active: filter === f }" @click="setFilter(f)">
        {{ t(f) }}
      </button>
    </div>

    <div class="list" v-if="!showStats && !showSettings">
      <div v-if="clips.length === 0" class="empty">
        <div class="empty-title">{{ t('empty') }}</div>
        <div class="empty-hint">{{ t('emptyHint') }}</div>
      </div>
      <template v-for="g in grouped" :key="g.label">
        <div class="group-label">{{ g.label }}</div>
        <div v-for="c in g.items" :key="c.id" class="row" :class="{ sel: flat[selected] && flat[selected].id === c.id, open: false }" @click="onClick(c)" @mouseenter="rowEnter(c, $event)" @mouseleave="rowLeave">
          <img v-if="c.kind === 'image'" class="thumb" :src="thumb(c)" loading="lazy" decoding="async" />
          <svg v-else-if="c.kind === 'file'" class="kind-icon" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/></svg>
          <span v-if="c.kind === 'file'" class="preview">{{ (c.preview || '').replace(/^\s+/, '') }}</span>
          <svg v-else-if="c.kind === 'html'" class="kind-icon" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m16 18 6-6-6-6"/><path d="m8 6-6 6 6 6"/></svg>
          <span v-if="c.kind === 'html'" class="preview">{{ (c.preview || '').replace(/^\s+/, '') }}</span>
          <span v-else class="preview">{{ (c.preview || '').replace(/^\s+/, '') }}</span>
          <span class="spacer"></span>
          <button class="icon-btn pin" :class="{ on: c.pinned }" @click="onPin(c, $event)" title="Pin">
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></svg>
          </button>
          <button class="icon-btn del" @click="onDelete(c, $event)" title="Delete">
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/><path d="M10 11v6"/><path d="M14 11v6"/></svg>
          </button>
        </div>
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
              <input type="checkbox" v-model="settings.autostart" />
              <span class="track"><span class="knob"></span></span>
            </label>
          </div>
          <div class="set-hint">{{ t('setAutoHint') }}</div>
        </div>
        <div class="group-label" style="margin-top: 14px;">{{ t('shortcut') }}</div>
        <div class="set-card">
          <div class="set-head">
            <span class="set-name">ClipVault</span>
            <kbd class="kbd">Alt + V</kbd>
          </div>
        </div>
        <div class="about">{{ t('about') }} ClipVault v0.1.0 · MIT</div>
      </div>
    </div>

    <div v-if="!isZoomWin" class="health">{{ health }}</div>
    <div v-if="!isZoomWin" class="grip" @mousedown.prevent="startResize($event)" title="">
      <span></span>
    </div>

    <transition name="fade">
      <div v-if="toast" class="toast">{{ toast }}</div>
    </transition>
  </div>
</template>

<style scoped>
.panel { display: flex; flex-direction: column; height: 100vh; background: var(--bg); }
.topbar { display: flex; align-items: center; gap: 8px; padding: 8px 10px 4px; }
.dot { width: 8px; height: 8px; border-radius: 50%; background: var(--accent); }
.brand { font-weight: 600; font-size: 13px; }
.hint { flex: 1; color: var(--fg-dim); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.icon-btn { border: 0; background: transparent; color: var(--fg-dim); width: 26px; height: 26px; border-radius: 6px; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; }
.icon-btn:hover { background: var(--bg-hover); color: var(--fg); }
.lang-btn { width: auto; padding: 0 7px; font-size: 11px; }
.search-row { padding: 4px 10px; position: relative; }
.search-clear { position: absolute; right: 18px; top: 50%; transform: translateY(-50%); border: 0; background: transparent; color: var(--fg-dim); width: 22px; height: 22px; border-radius: 6px; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; }
.search-clear:hover { background: var(--bg-hover); color: var(--fg); }
.search { width: 100%; padding: 7px 30px 7px 10px; background: var(--bg-2); color: var(--fg); border: 1px solid var(--border); border-radius: 6px; outline: none; font-size: 13px; }
.search:focus { border-color: var(--accent-dim); }
.tabs { display: flex; gap: 4px; padding: 4px 10px; flex-wrap: wrap; }
.tab { border: 0; background: transparent; color: var(--fg-dim); padding: 4px 12px; border-radius: 999px; cursor: pointer; font-size: 12px; }
.tab:hover { background: var(--bg-hover); }
.tab.active { background: var(--accent); color: #fff; }
.list { flex: 1; overflow-y: auto; padding: 2px 6px 8px; }
@keyframes rowin { from { opacity: 0; transform: translateY(2px); } to { opacity: 1; transform: none; } }
.row { animation: rowin 80ms ease-out; }
.empty { color: var(--fg-dim); text-align: center; margin-top: 56px; }
.empty-title { font-size: 14px; }
.empty-hint { font-size: 11px; margin-top: 6px; opacity: .8; padding: 0 30px; }
.group-label { font-size: 10px; color: var(--fg-dim); padding: 8px 8px 2px; }
.row { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 7px 10px 7px 8px;
  border-left: 2px solid transparent; border-radius: 6px; cursor: pointer; }
.row + .row { border-top: 1px solid var(--border); }
.row:hover { background: var(--bg-hover); }
.row.sel { background: var(--bg-hover); border-left-color: var(--accent); }
.preview { flex: 1; min-width: 0; font-size: 12px; line-height: 1.45; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.kind-icon { color: var(--fg-dim); flex-shrink: 0; }
.thumb { height: 36px; max-width: 64px; object-fit: cover; border-radius: 5px; border: 1px solid var(--border); flex: 1; min-width: 0; }
.spacer { flex: 0; }
.icon-btn.pin.on { color: var(--pin); }
.icon-btn.del:hover { color: var(--danger); }
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
.top-row .preview { flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; -webkit-line-clamp: 1; }
.top-row .cnt { color: var(--fg-dim); font-size: 11px; }
.settings { padding: 6px 10px 12px; display: flex; flex-direction: column; gap: 8px; }
.set-card { background: var(--bg-2); border: 1px solid var(--border); border-radius: 8px; padding: 10px 12px; }
.set-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
.set-name { font-size: 12px; font-weight: 600; }
.set-value { font-size: 12px; font-variant-numeric: tabular-nums; color: var(--accent); font-weight: 600; }
.set-hint { font-size: 10.5px; color: var(--fg-dim); margin-top: 6px; line-height: 1.4; }
.set-slider { width: 100%; margin-top: 8px; accent-color: var(--accent); height: 4px; cursor: pointer; }
.switch { position: relative; display: inline-block; cursor: pointer; }
.switch input { position: absolute; opacity: 0; inset: 0; margin: 0; cursor: pointer; }
.track { display: block; width: 36px; height: 20px; border-radius: 999px; background: var(--border); transition: background .15s ease; position: relative; }
.knob { position: absolute; top: 2px; left: 2px; width: 16px; height: 16px; border-radius: 50%; background: #fff; transition: transform .15s ease; box-shadow: 0 1px 3px rgba(0,0,0,.3); }
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
.health { font-size: 9px; color: var(--fg-dim); text-align: right; padding: 0 10px 2px; opacity: .7; flex-shrink: 0; }
.grip { height: 14px; display: flex; align-items: center; justify-content: center; cursor: ns-resize; flex-shrink: 0; }
.grip span { width: 36px; height: 4px; border-radius: 2px; background: var(--border); }
.grip:hover span { background: var(--fg-dim); }
.toast { position: fixed; bottom: 14px; left: 50%; transform: translateX(-50%); background: var(--bg-2); color: var(--fg); border: 1px solid var(--border); padding: 8px 16px; border-radius: 999px; font-size: 12px; box-shadow: 0 6px 20px rgba(0,0,0,.35); max-width: 90%; z-index: 60; }
.fade-enter-active, .fade-leave-active { transition: opacity .2s; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
::-webkit-scrollbar { width: 8px; }
::-webkit-scrollbar-thumb { background: var(--border); border-radius: 4px; }
</style>
