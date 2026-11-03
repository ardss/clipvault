// 0.2 card grouping: one-step undo stack + reverse-command planning.
//
// Pure module so vitest can lock the semantics without a Tauri runtime.
// The stack holds at most ONE action ("最近一步撤销"): a merge chain A→B→C
// is not safely reversible by replaying inverses in the presence of
// concurrent clipboard events, so after one undo the stack is empty again.

const stack = []

export const UNDO_CAPACITY = 1

export function pushUndo(action) {
  stack.length = Math.min(stack.length, UNDO_CAPACITY - 1)
  stack.push(action)
}

export function popUndo() {
  return stack.pop() ?? null
}

export function undoDepth() {
  return stack.length
}

export function resetUndo() {
  stack.length = 0
}

/**
 * Plan the reverse Tauri command calls for one recorded action.
 * Returns an ordered list of {cmd, args}; an empty list means the action
 * cannot be reversed (e.g. split of a lone member destroyed the group —
 * there is no sibling left to re-merge into).
 *
 * - merge(source,target)  ⇐ split_clip(source)   (source leaves the group)
 * - split(id, sibling)    ⇐ merge_clips(id→sibling) — joins at the END of the
 *   group, so in-group ORDER may differ from the pre-split one; membership
 *   is exact.
 * - ungroup(memberIds)    ⇐ merge each member i>0 into member[0] — same
 *   caveat: membership exact, order rebuilt from merge sequence.
 */
export function buildUndoCommands(action) {
  if (!action) return []
  switch (action.type) {
    case 'merge':
      return [{ cmd: 'split_clip', args: { id: action.sourceId } }]
    case 'split':
      return action.siblingId == null
        ? []
        : [{ cmd: 'merge_clips', args: { source_id: action.id, target_id: action.siblingId } }]
    case 'ungroup': {
      const ids = action.memberIds ?? []
      return ids
        .slice(1)
        .map((id) => ({ cmd: 'merge_clips', args: { source_id: id, target_id: ids[0] } }))
    }
    default:
      return []
  }
}
