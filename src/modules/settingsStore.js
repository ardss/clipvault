// localStorage-backed UI preferences extracted from App.vue (behavior unchanged).
// i18n message tables live in ./i18n.js; this module owns the persisted
// prefs (cv-lang / cv-theme) and their reactive refs.
import { computed, ref, watch } from 'vue'
import { documentLangAttr, translate, LANGS } from './i18n.js'

const LANG_KEY = 'cv-lang'
const THEME_KEY = 'cv-theme'

const LANG_LABEL = { zh: '中', en: 'EN', ja: 'あ', ko: '한', es: 'ES', fr: 'FR', de: 'DE' }

export function createI18nStore() {
  const saved = localStorage.getItem(LANG_KEY)
  const lang = ref(LANGS.includes(saved) ? saved : 'zh')
  const t = (k) => translate(lang.value, k)
  watch(lang, (l) => { document.documentElement.lang = documentLangAttr(l) })
  document.documentElement.lang = documentLangAttr(lang.value)
  function toggleLang() {
    const i = LANGS.indexOf(lang.value)
    lang.value = LANGS[(i + 1) % LANGS.length]
    localStorage.setItem(LANG_KEY, lang.value)
  }
  return { lang, t, toggleLang, langLabel: () => LANG_LABEL[lang.value] ?? lang.value }
}

export function createThemeStore() {
  const theme = ref(localStorage.getItem(THEME_KEY) || 'dark')
  function applyTheme() { document.documentElement.dataset.theme = theme.value }
  function toggleTheme() {
    theme.value = theme.value === 'dark' ? 'light' : 'dark'
    localStorage.setItem(THEME_KEY, theme.value)
    applyTheme()
  }
  applyTheme()
  return { theme, toggleTheme }
}

// Sensitive keywords live in settings as a string array; the UI edits one
// comma-separated string (computed get/set extracted from App.vue).
export function sensitiveTextComputed(settings) {
  return computed({
    get: () => (settings.value.sensitive_keywords || []).join(', '),
    set: (v) => {
      settings.value.sensitive_keywords = v.split(',').map((s) => s.trim()).filter(Boolean)
    },
  })
}

// Excluded source apps share the same comma-separated-input pattern.
export function excludedAppsComputed(settings) {
  return computed({
    get: () => (settings.value.excluded_apps || []).join(', '),
    set: (v) => {
      settings.value.excluded_apps = v.split(',').map((s) => s.trim()).filter(Boolean)
    },
  })
}
