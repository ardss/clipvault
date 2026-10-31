import { describe, it, expect } from 'vitest'
import { reduce, DISARM_EVENTS, INITIAL_STATE, ARM_TIMEOUT_MS } from '../confirmArmed.js'

// Incident #10 (CHANGELOG.md:73, user-data-endangering): an armed
// "clear all history" confirmation survived the settings panel being
// closed, so an unrelated later click wiped the whole history.
// The transition table below EXHAUSTIVELY asserts: every disarm-class
// event (panel close / page switch / blur / expiry / escape) forces
// armed=false, and no other event re-arms from the armed state.

const T0 = 1000
const disarmed = () => ({ ...INITIAL_STATE })
const armedFresh = () => ({ armed: true, armedAt: T0, fired: false })
const armedExpired = () => ({ armed: true, armedAt: T0 - ARM_TIMEOUT_MS, fired: false })

const ALL_EVENTS = ['click', ...DISARM_EVENTS]

// exhaustive transition table: [from-state, event, now, expected-state]
const TABLE = []

// from DISARMED: only 'click' arms; everything else stays disarmed
for (const ev of ALL_EVENTS) {
  if (ev === 'click') {
    TABLE.push(['disarmed', 'click', T0, { armed: true, armedAt: T0, fired: false }])
  } else {
    TABLE.push(['disarmed', ev, T0, { armed: false, armedAt: 0, fired: false }])
  }
}

// from ARMED (fresh, within window)
TABLE.push(['armed-fresh', 'click', T0 + 1, { armed: false, armedAt: 0, fired: true }]) // confirm
for (const ev of DISARM_EVENTS) {
  if (ev === 'expire') {
    TABLE.push(['armed-fresh', 'expire', T0 + ARM_TIMEOUT_MS, { armed: false, armedAt: 0, fired: false }])
  } else {
    TABLE.push(['armed-fresh', ev, T0 + 10, { armed: false, armedAt: 0, fired: false }])
  }
}

// from ARMED (expired window — second click must NOT confirm)
TABLE.push(['armed-expired', 'click', T0, { armed: false, armedAt: 0, fired: false }])
for (const ev of DISARM_EVENTS) {
  TABLE.push(['armed-expired', ev, T0, { armed: false, armedAt: 0, fired: false }])
}

describe('confirmArmed.reduce — exhaustive transition table', () => {
  it('every table row matches', () => {
    for (const [from, ev, now, expected] of TABLE) {
      const state =
        from === 'disarmed' ? disarmed() : from === 'armed-fresh' ? armedFresh() : armedExpired()
      expect(reduce(state, ev, now), `${from} --${ev}--> ` + JSON.stringify(expected)).toEqual(expected)
    }
  })

  it('table covers every event from every armed state', () => {
    const covered = new Set(TABLE.map(([from, ev]) => from + '::' + ev))
    for (const ev of ALL_EVENTS) {
      expect(covered.has('armed-fresh::' + ev), `missing armed-fresh × ${ev}`).toBe(true)
      expect(covered.has('armed-expired::' + ev), `missing armed-expired × ${ev}`).toBe(true)
    }
  })
})

describe('confirmArmed — incident #10 disarm invariants', () => {
  it('ANY panel-close / switch-page / blur / escape / expire disarms', () => {
    for (const ev of DISARM_EVENTS) {
      const next = reduce(armedFresh(), ev, T0 + 5)
      expect(next.armed, `event ${ev} must disarm`).toBe(false)
      // and once disarmed, a later click only re-arms — it must not fire
      const after = reduce(next, 'click', T0 + 100)
      expect(after.fired).toBe(false)
    }
  })

  it('armed state can only be left armed by "click" from disarmed', () => {
    for (const ev of ALL_EVENTS) {
      const next = reduce(armedFresh(), ev, T0 + 1)
      if (ev !== 'click') expect(next.armed).toBe(false)
    }
  })

  it('first click arms; an unrelated disarmed-click never fires clear', () => {
    expect(reduce(disarmed(), 'click', 0).fired).toBe(false)
  })

  it('confirm window is exactly ARM_TIMEOUT_MS', () => {
    expect(ARM_TIMEOUT_MS).toBe(3000)
    expect(reduce(armedFresh(), 'click', T0 + ARM_TIMEOUT_MS - 1).fired).toBe(true)
    expect(reduce(armedFresh(), 'click', T0 + ARM_TIMEOUT_MS).fired).toBe(false)
  })

  it('unknown event is a no-op (never silently arms or disarms)', () => {
    const s = armedFresh()
    expect(reduce(s, 'something-else', T0)).toBe(s)
  })
})
