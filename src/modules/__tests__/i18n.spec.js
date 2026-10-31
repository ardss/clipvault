import { describe, it, expect } from 'vitest'
import { messages, translate, documentLangAttr } from '../i18n.js'

const langs = Object.keys(messages)

describe('i18n key completeness', () => {
  it('has exactly zh and en tables', () => {
    expect(langs.sort()).toEqual(['en', 'zh'])
  })

  it('zh and en expose the exact same key set', () => {
    const zhKeys = Object.keys(messages.zh).sort()
    const enKeys = Object.keys(messages.en).sort()
    expect(enKeys).toEqual(zhKeys)
  })

  it('every key is a non-empty string in every language', () => {
    for (const lang of langs) {
      for (const [k, v] of Object.entries(messages[lang])) {
        expect(typeof v, `${lang}.${k}`).toBe('string')
        expect(v.length, `${lang}.${k}`).toBeGreaterThan(0)
      }
    }
  })

  it('keys referenced by App.vue templates exist in both languages', () => {
    // Spot-check the template-touching keys, incl. filter tab ids (all/text/
    // image/link/file/pinned) and the incident #10 confirmation strings.
    const must = ['all', 'text', 'image', 'link', 'file', 'pinned', 'clearBtn', 'clearConfirm',
      'cleared', 'pausedTag', 'statsTitle', 'settingsTitle', 'expired', 'noMatch']
    for (const lang of langs) {
      for (const k of must) expect(messages[lang][k], `${lang}.${k}`).toBeDefined()
    }
  })

  it('translate falls back to the key itself for unknown keys (old behavior)', () => {
    expect(translate('zh', 'nonexistent_key')).toBe('nonexistent_key')
    expect(translate('en', 'dayToday')).toBe('Today')
    expect(translate('zh', 'dayToday')).toBe('今天')
    // unknown language falls back to zh table
    expect(translate('xx', 'dayToday')).toBe('今天')
  })

  it('documentLangAttr mapping matches old inline logic', () => {
    expect(documentLangAttr('zh')).toBe('zh-CN')
    expect(documentLangAttr('en')).toBe('en')
  })
})
