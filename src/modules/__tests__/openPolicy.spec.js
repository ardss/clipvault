import { describe, it, expect } from 'vitest'
import { shouldFetchFullContent, CONTENT_TRUNCATE_LIMIT } from '../openPolicy.js'

// Incident #6 regression: the fetch-on-open condition must trigger on
// text_path (oversized >256KB copies keep only a preview in `content`)
// as well as on content length >= 4096.
describe('openPolicy.shouldFetchFullContent', () => {
  it('text_path branch: oversized copy with short preview must fetch', () => {
    // >256KB text lives in a side file; content is only a 2000-char preview
    const oversized = {
      kind: 'text',
      content: 'preview only'.repeat(100), // ~1200 chars < 4096
      text_path: 'clips/abc.txt',
    }
    expect(shouldFetchFullContent(oversized)).toBe(true)
  })

  it('text_path truthiness: empty-string text_path must NOT fetch', () => {
    const c = { kind: 'text', content: 'short', text_path: '' }
    expect(shouldFetchFullContent(c)).toBe(false)
  })

  it('length branch: content >= 4096 must fetch even without text_path', () => {
    expect(CONTENT_TRUNCATE_LIMIT).toBe(4096)
    const boundary = { kind: 'text', content: 'x'.repeat(4096) }
    expect(shouldFetchFullContent(boundary)).toBe(true)
  })

  it('length branch: content 4095 chars must NOT fetch', () => {
    expect(shouldFetchFullContent({ kind: 'text', content: 'x'.repeat(4095) })).toBe(false)
  })

  it('image kind never fetches regardless of length', () => {
    expect(shouldFetchFullContent({ kind: 'image', content: 'x'.repeat(5000), text_path: 'p' })).toBe(false)
  })

  it('short normal text without text_path must not fetch', () => {
    expect(shouldFetchFullContent({ kind: 'text', content: 'hello' })).toBe(false)
    expect(shouldFetchFullContent({ kind: 'text', content: '' })).toBe(false)
    expect(shouldFetchFullContent({ kind: 'text' })).toBe(false)
  })

  it('null/undefined clip is safe', () => {
    expect(shouldFetchFullContent(null)).toBe(false)
  })
})
