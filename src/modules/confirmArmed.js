// Armed state machine for the destructive "clear all history" confirmation.
// Extracted from App.vue (incident #10, CHANGELOG.md:73): a pending armed
// confirmation must be disarmed by ANY panel close / page switch / blur —
// an unrelated later click must never be able to wipe the whole history.
//
// Pure reducer: App.vue holds the state in a ref and dispatches events; the
// timer that emits 'expire' after ARM_TIMEOUT_MS lives on the App.vue side.

export const ARM_TIMEOUT_MS = 3000

export const INITIAL_STATE = Object.freeze({ armed: false, armedAt: 0, fired: false })

// Events that MUST disarm. Kept as a named export so the spec can enumerate
// them exhaustively — a new disarm-event that is forgotten here fails the test.
export const DISARM_EVENTS = Object.freeze(['panel-close', 'switch-page', 'blur', 'expire', 'escape'])

/**
 * @param {{armed:boolean, armedAt:number, fired:boolean}} state
 * @param {'click'|'panel-close'|'switch-page'|'blur'|'expire'|'escape'} event
 * @param {number} now - monotonic-ish ms timestamp (Date.now in App.vue)
 */
export function reduce(state, event, now = 0) {
  switch (event) {
    case 'click':
      if (!state.armed) {
        return { armed: true, armedAt: now, fired: false }
      }
      // second click: confirm only inside the arming window
      if (now - state.armedAt < ARM_TIMEOUT_MS) {
        return { armed: false, armedAt: 0, fired: true }
      }
      return { armed: false, armedAt: 0, fired: false }
    case 'panel-close':
    case 'switch-page':
    case 'blur':
    case 'expire':
    case 'escape':
      return { armed: false, armedAt: 0, fired: state.fired }
    default:
      return state
  }
}

export function isArmed(state) {
  return state.armed
}
