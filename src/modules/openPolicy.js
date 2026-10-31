// fetch-on-open decision extracted from App.vue (openZoom), behavior unchanged.
//
// The list query truncates content at 4096 chars — the full body must be
// fetched for long texts before showing the zoom window. Oversized (>256KB)
// copies keep the full text in a side file: their content column is only a
// 2000-char preview, so text_path must trigger the fetch too.
// (Incident #6: the original condition missed text_path and truncated.)

export const CONTENT_TRUNCATE_LIMIT = 4096

/**
 * Decide whether a clip's full content must be fetched via `clip_content`
 * before it can be displayed in the zoom window.
 * @param {object} c - clip row from list_clips
 * @returns {boolean}
 */
export function shouldFetchFullContent(c) {
  if (!c) return false
  if (c.kind === 'image') return false
  return (c.content || '').length >= CONTENT_TRUNCATE_LIMIT || !!c.text_path
}
