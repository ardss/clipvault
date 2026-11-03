import { describe, it, expect, beforeEach } from 'vitest'
import {
  pushUndo, popUndo, undoDepth, resetUndo, buildUndoCommands, UNDO_CAPACITY,
} from '../groupActions.js'

describe('one-step undo stack', () => {
  beforeEach(() => resetUndo())

  it('holds at most one action (最近一步撤销)', () => {
    expect(UNDO_CAPACITY).toBe(1)
    pushUndo({ type: 'merge', sourceId: 1, targetId: 2 })
    pushUndo({ type: 'merge', sourceId: 3, targetId: 4 })
    expect(undoDepth()).toBe(1)
    expect(popUndo()).toEqual({ type: 'merge', sourceId: 3, targetId: 4 })
    expect(popUndo()).toBeNull()
  })

  it('pop on empty stack is null and depth stays 0', () => {
    expect(popUndo()).toBeNull()
    expect(undoDepth()).toBe(0)
  })
})

describe('buildUndoCommands — reverse Tauri commands', () => {
  it('merge reverses to split_clip of the source', () => {
    expect(buildUndoCommands({ type: 'merge', sourceId: 7, targetId: 9 })).toEqual([
      { cmd: 'split_clip', args: { id: 7 } },
    ])
  })

  it('split reverses to merge back into the captured sibling', () => {
    expect(buildUndoCommands({ type: 'split', id: 5, siblingId: 6 })).toEqual([
      { cmd: 'merge_clips', args: { source_id: 5, target_id: 6 } },
    ])
  })

  it('split of a lone member (no sibling) is not reversible — empty plan, not a guess', () => {
    expect(buildUndoCommands({ type: 'split', id: 5, siblingId: null })).toEqual([])
  })

  it('ungroup rebuilds the group by merging every member into the first', () => {
    const plan = buildUndoCommands({ type: 'ungroup', memberIds: [1, 2, 3] })
    expect(plan).toEqual([
      { cmd: 'merge_clips', args: { source_id: 2, target_id: 1 } },
      { cmd: 'merge_clips', args: { source_id: 3, target_id: 1 } },
    ])
  })

  it('ungroup of a single member yields an empty plan', () => {
    expect(buildUndoCommands({ type: 'ungroup', memberIds: [4] })).toEqual([])
  })

  it('unknown or missing action plans nothing', () => {
    expect(buildUndoCommands(null)).toEqual([])
    expect(buildUndoCommands({ type: 'explode' })).toEqual([])
  })
})
