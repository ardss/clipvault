// localStorage-backed UI preferences extracted from App.vue (behavior unchanged).
// i18n message tables live in ./i18n.js; this module owns the persisted
// prefs (cv-lang / cv-theme) and their reactive refs.
import { computed, ref, watch } from 'vue'
import { documentLangAttr, translate } from './i18n.js'

const LANG_KEY = 'cv-lang'
const THEME_KEY = 'cv-theme'

export function createI18nStore() {
  const lang = ref(localStorage.getItem(LANG_KEY) || 'zh')
  const t = (k) => translate(lang.value, k)
  watch(lang, (l) => { document.documentElement.lang = documentLangAttr(l) })
  document.documentElement.lang = documentLangAttr(lang.value)
  function toggleLang() {
    lang.value = lang.value === 'zh' ? 'en' : 'zh'
    localStorage.setItem(LANG_KEY, lang.value)
  }
  return { lang, t, toggleLang }
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
