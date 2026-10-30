//! Window helpers: foreground/cursor, panel rect, focus capture/restore, zoom watchdog.
use super::*;

// ---------- window helpers ----------

pub fn get_foreground() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

pub fn get_cursor_pos() -> (i32, i32) {
    unsafe {
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        (p.x, p.y)
    }
}

/// Diagnostic: logs the real foreground window and its focused child right
/// before the injector fires (paste-loss triage). cvlog, not stderr — the
/// GUI subsystem has no console, so stderr output was invisible in the very
/// sessions this triage exists for.
pub fn log_send_time() {
    unsafe {
        let fg = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let _ = GetGUIThreadInfo(thread, &mut gi);
        cvlog!(
            "[cv] send-time: fg={:x} focus={:x}",
            fg.0 as usize,
            gi.hwndFocus.0 as usize
        );
    }
}

pub fn update_panel_rect(hwnd: isize) {
    PANEL_HWND.store(hwnd, Ordering::SeqCst);
}

/// ZOOM_HWND is written by register_zoom and used to be never cleared when
/// the zoom window was destroyed (the 10-min frontend idle reaper). Win32
/// recycles HWND values: the stale handle eventually pointed at an
/// unrelated LIVE window, so clicks inside it stopped hiding the panel and
/// show_panel skipped paste-target capture. All readers go through this
/// validator, which also self-heals the global to 0.
pub fn live_zoom_hwnd(stored: isize) -> isize {
    if stored == 0 {
        return 0;
    }
    if unsafe { IsWindow(HWND(stored as _)) }.as_bool() {
        stored
    } else {
        ZOOM_HWND.store(0, Ordering::SeqCst);
        0
    }
}

/// OS-truth window rect (Tauri's own position bookkeeping can diverge from
/// what the OS actually applied after DPI rescaling).
pub fn get_window_rect(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mut r = RECT::default();
        if GetWindowRect(HWND(hwnd as _), &mut r).is_ok() {
            return Some((r.left, r.top, r.right, r.bottom));
        }
    }
    None
}

pub fn point_in_panel(x: i32, y: i32) -> bool {
    // read the panel's CURRENT rect at click time — a rect cached at show
    // time goes stale (async repositioning, DPI rescale) and then every
    // click counts as "outside", instantly hiding the panel
    let hwnd = PANEL_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        unsafe {
            let mut r = RECT::default();
            if GetWindowRect(HWND(hwnd as _), &mut r).is_ok()
                && x >= r.left
                && x <= r.right
                && y >= r.top
                && y <= r.bottom
            {
                return true;
            }
        }
    }
    let (l, t, r, b) = *PANEL_RECT.lock().unwrap_or_else(|p| p.into_inner());
    if x >= l && x <= r && y >= t && y <= b {
        return true;
    }
    // the zoom preview window belongs to the panel UI — validated live: a
    // stale/recycled HWND used to swallow outside clicks for an unrelated window
    let zh = live_zoom_hwnd(ZOOM_HWND.load(Ordering::SeqCst));
    if zh != 0 && unsafe { IsWindowVisible(HWND(zh as _)) }.as_bool() {
        unsafe {
            let mut r2 = RECT::default();
            if GetWindowRect(HWND(zh as _), &mut r2).is_ok() {
                return x >= r2.left && x <= r2.right && y >= r2.top && y <= r2.bottom;
            }
        }
    }
    false
}

/// Records which window inside `target` should receive the paste, so focus
/// can be restored after the panel (which takes focus) is hidden.
///
/// `hwndFocus` is the identity (Win32: the window receiving keyboard input);
/// `hwndCaret` only draws the caret — related but NOT interchangeable
/// (contract P1-3). The caret owner is auxiliary: used only when hwndFocus
/// is unknown (0) or reports the top-level form itself (the WinForms quirk
/// where focus is the form while the caret sits on a real control). Whatever
/// gets recorded is re-validated against the target at restore time
/// (identity + containment), so a stale or foreign capture fails closed
/// instead of loosening the paste check.
pub fn capture_focus(target: isize) {
    if target == 0 {
        return;
    }
    unsafe {
        let thread = GetWindowThreadProcessId(HWND(target as _), None);
        let mut gi = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread, &mut gi).is_ok() {
            let focus = gi.hwndFocus.0 as isize;
            let caret = gi.hwndCaret.0 as isize;
            let candidate = capture_candidate(focus, caret, target);
            let same_thread = candidate == target || window_thread_id(candidate) == thread;
            let stored = captured_child_gate(
                candidate,
                target,
                same_thread,
                is_descendant_of(candidate, target),
            );
            FOCUS_HWND.store(stored, Ordering::SeqCst);
            cvlog!("[cv] capture_focus: target={target:x} focus={focus:x} caret={caret:x} child={stored:x}");
        } else {
            cvlog!("[cv] capture_focus: GetGUIThreadInfo failed");
            // unknown state — no child identity, top-level matching only
            FOCUS_HWND.store(0, Ordering::SeqCst);
        }
    }
}

/// Pure capture selection (unit-tested): hwndFocus is the identity; the
/// caret owner is auxiliary only.
fn capture_candidate(focus: isize, caret: isize, target: isize) -> isize {
    if focus != 0 && focus != target {
        focus
    } else if caret != 0 {
        caret
    } else {
        focus
    }
}

/// Pure capture gate (unit-tested): store only a provable identity — the
/// target itself, or a live same-thread descendant. Anything else records as
/// "no child" (top-level-only paste matching).
fn captured_child_gate(
    candidate: isize,
    target: isize,
    same_thread: bool,
    descendant: bool,
) -> isize {
    if candidate == target || (candidate != 0 && same_thread && descendant) {
        candidate
    } else {
        0
    }
}

// ---------- verified focus restore (contract P1-1/P1-2) ----------

/// Outcome of a verified focus restore. Every variant must be handled by the
/// paste path; none may silently degrade into a looser paste check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRestore {
    /// SetFocus succeeded AND the target thread's actual focus is the child.
    Restored,
    /// No child identity was captured — top-level matching is the whole
    /// contract (nothing to restore).
    NoCapturedChild,
    /// The captured control no longer exists.
    ChildDestroyed,
    /// The captured handle exists but no longer belongs to the target
    /// (different thread, not a descendant, not the target itself).
    IdentityMismatch,
    /// The target is valid but focus could not be restored to the captured
    /// identity — never downgraded into a looser handshake.
    RestoreFailed,
}

/// Fact-based classification of the captured child against the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChildCheck {
    RestoreChild,
    RestoreSelf,
    NoCapturedChild,
    ChildDestroyed,
    IdentityMismatch,
}

/// Pure classifier (unit-tested + exercised against real windows in tests).
fn classify_child(
    child: isize,
    target: isize,
    alive: bool,
    same_thread: bool,
    descendant: bool,
) -> ChildCheck {
    if child == 0 {
        return ChildCheck::NoCapturedChild;
    }
    if child == target {
        // IsChild(parent, parent) is false by definition — the top-level
        // holding keyboard focus itself is a VALID identity, not an invalid
        // one (contract P1-2).
        return if alive {
            ChildCheck::RestoreSelf
        } else {
            ChildCheck::ChildDestroyed
        };
    }
    if !alive {
        return ChildCheck::ChildDestroyed;
    }
    if !same_thread || !descendant {
        return ChildCheck::IdentityMismatch;
    }
    ChildCheck::RestoreChild
}

fn child_alive(hwnd: isize) -> bool {
    hwnd != 0 && unsafe { IsWindow(HWND(hwnd as _)) }.as_bool()
}

fn window_thread_id(hwnd: isize) -> u32 {
    unsafe { GetWindowThreadProcessId(HWND(hwnd as _), None) }
}

fn is_descendant_of(child: isize, target: isize) -> bool {
    child != 0
        && child != target
        && unsafe { IsChild(HWND(target as _), HWND(child as _)) }.as_bool()
}

/// Restores keyboard focus to the captured child, verifying identity before
/// the call and the actual OS focus after it. The caller must handle every
/// outcome; `RestoreFailed` and `IdentityMismatch` must never be turned into
/// a looser paste check (contract P1-1).
pub fn restore_focus_verified(target: isize, child: isize) -> FocusRestore {
    if target == 0 {
        // the paste path validates the target before writing; a zero target
        // here means the identity itself is gone — fail closed
        return FocusRestore::RestoreFailed;
    }
    let alive = child_alive(child);
    let check = classify_child(
        child,
        target,
        alive,
        alive && window_thread_id(child) == window_thread_id(target),
        alive && is_descendant_of(child, target),
    );
    match check {
        ChildCheck::RestoreChild | ChildCheck::RestoreSelf => {
            if set_focus_and_verify(target, child) {
                FocusRestore::Restored
            } else {
                FocusRestore::RestoreFailed
            }
        }
        ChildCheck::NoCapturedChild => FocusRestore::NoCapturedChild,
        ChildCheck::ChildDestroyed => FocusRestore::ChildDestroyed,
        ChildCheck::IdentityMismatch => FocusRestore::IdentityMismatch,
    }
}

/// SetFocus + post-verification. SetFocus returns the focus hwnd from BEFORE
/// the call, so its Result alone proves nothing about the transfer — the OS
/// view of the target thread's focus is verified instead, with a short settle
/// window (some frameworks reassign focus asynchronously on activation).
fn set_focus_and_verify(target: isize, child: isize) -> bool {
    unsafe {
        let thread = GetWindowThreadProcessId(HWND(target as _), None);
        let this_thread = GetCurrentThreadId();
        let attached = AttachThreadInput(this_thread, thread, true).as_bool();
        let set_ok = SetFocus(HWND(child as _)).is_ok();
        if attached {
            let _ = AttachThreadInput(this_thread, thread, false);
        }
        if !set_ok {
            cvlog!("[cv] restore: SetFocus failed target={target:x} child={child:x}");
            return false;
        }
        for _ in 0..5 {
            if thread_focus_hwnd(target) == Some(child) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cvlog!("[cv] restore: post-set focus verify failed target={target:x} child={child:x}");
        false
    }
}

/// The hwnd currently owning keyboard focus on `target`'s thread, or None
/// when the OS query fails (an unknown state, never treated as a match).
fn thread_focus_hwnd(target: isize) -> Option<isize> {
    unsafe {
        let thread = GetWindowThreadProcessId(HWND(target as _), None);
        let mut gi = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread, &mut gi).is_ok() {
            Some(gi.hwndFocus.0 as isize)
        } else {
            None
        }
    }
}

/// Where does keyboard focus actually live right now? `Some(hwnd)` only when
/// the foreground IS the verified target and its thread reports a focus hwnd
/// inside it; every other state is `None` (attribution unknown).
pub fn current_focus_attribution(target: isize) -> Option<isize> {
    let fg = get_foreground();
    attributed_focus(fg, thread_focus_hwnd(target), target, &|h| {
        is_descendant_of(h, target)
    })
}

/// Pure attribution decision (unit-tested): foreground must BE the target,
/// the reported focus must exist and be the target or its descendant.
fn attributed_focus(
    fg: isize,
    thread_focus: Option<isize>,
    target: isize,
    is_descendant: &dyn Fn(isize) -> bool,
) -> Option<isize> {
    if target == 0 || fg != target {
        return None;
    }
    let h = thread_focus?;
    if h == 0 {
        return None;
    }
    if h == target || is_descendant(h) {
        Some(h)
    } else {
        None
    }
}

/// Taps ALT down/up to release the foreground lock, verifying the keystroke
/// actually landed via GetAsyncKeyState BETWEEN down and up (after the up
/// the key already reads as released — the check must sit between the two
/// calls). keybd_event issued from inside the app process is the same
/// primitive the project proved gets silently swallowed (the injector
/// process exists for exactly that) — trusting it unverified repeated the
/// self-contradictory assumption.
fn tap_alt_verified() -> bool {
    unsafe {
        keybd_event(VK_MENU.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        // 0x8000 = the key is physically down right now
        let verified = GetAsyncKeyState(VK_MENU.0 as i32) & 0x8000u16 as i16 != 0;
        keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        verified
    }
}

/// Pure decision (unit-tested): an unverified in-process tap must fall back
/// to the helper process — the only injection path with proven delivery.
fn needs_helper_alt_fallback(verified: bool) -> bool {
    !verified
}

/// Fallback: the identical ALT tap from a separate process (same mechanism
/// the paste injector relies on). Detached, no window, null stdio.
fn tap_alt_via_helper() {
    if let Ok(exe) = std::env::current_exe() {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // stdio MUST be null: a GUI process launched from Explorer has no
        // valid std handles (same failure the injector spawn documented)
        let r = std::process::Command::new(exe)
            .arg("--alt-tap")
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if let Err(e) = r {
            cvlog!("[cv] restore: helper ALT tap spawn failed: {e}");
        }
    }
}

pub fn restore_foreground(target: isize) -> bool {
    if target == 0 {
        cvlog!("[cv] restore: target is 0");
        return false;
    }
    unsafe {
        let h = HWND(target as _);
        if !IsWindow(h).as_bool() {
            cvlog!("[cv] restore: target {target:x} is not a window");
            return false;
        }
        if IsIconic(h).as_bool() {
            let _ = ShowWindowAsync(h, SW_RESTORE);
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
        // ALT tap releases the foreground lock — verified against the OS
        // keyboard state; if the in-process tap was swallowed (documented,
        // reproducible), fall back to the helper process before judging
        // SetForegroundWindow
        if needs_helper_alt_fallback(tap_alt_verified()) {
            cvlog!("[cv] restore: in-process ALT tap not verified — falling back to helper tap");
            tap_alt_via_helper();
        }
        let this_thread = GetCurrentThreadId();
        let target_thread = GetWindowThreadProcessId(h, None);
        let _ = AttachThreadInput(this_thread, target_thread, true);
        let _ = SetForegroundWindow(h);
        let _ = AttachThreadInput(this_thread, target_thread, false);
        for _ in 0..10 {
            let fg = GetForegroundWindow();
            if fg == h {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        cvlog!(
            "[cv] restore: target={:x} fg_now={:x} err={:?}",
            target,
            GetForegroundWindow().0 as usize,
            GetLastError()
        );
        false
    }
}

// ---------- UIPI target elevation check ----------

/// Integrity level RID of a process (0x1000 low / 0x2000 medium / 0x3000
/// high). Cross-IL queryable via PROCESS_QUERY_LIMITED_INFORMATION.
pub fn process_integrity_level(pid: u32) -> Option<u32> {
    use windows::Win32::Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel,
        TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let proc_h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut token = Default::default();
        if OpenProcessToken(proc_h, TOKEN_QUERY, &mut token).is_err() {
            return None;
        }
        // size query: fails by design, filling the required length
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut len);
        let mut buf = vec![0u8; len as usize];
        if GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buf.as_mut_ptr().cast()),
            len,
            &mut len,
        )
        .is_err()
        {
            return None;
        }
        let label = buf.as_ptr() as *const TOKEN_MANDATORY_LABEL;
        let sid = (*label).Label.Sid;
        if sid.is_invalid() {
            return None;
        }
        let count = *GetSidSubAuthorityCount(sid) as usize;
        if count == 0 {
            return None;
        }
        Some(*GetSidSubAuthority(sid, (count - 1) as u32))
    }
}

/// Pure decision (unit-tested): a target MORE privileged than us silently
/// drops our keybd_event (UIPI) — the paste would "succeed" (clipboard
/// overwritten, row marked used, panel closed) while nothing is pasted.
pub(crate) fn elevation_blocks_paste(target_il: u32, own_il: u32) -> bool {
    target_il > own_il
}

/// True when the paste target's process runs at a HIGHER integrity level
/// than ours. Query failure is fail-open (logged): a token-query hiccup
/// must not kill every paste.
pub fn target_elevated(target: isize) -> bool {
    if target == 0 {
        return false;
    }
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(HWND(target as _), Some(&mut pid));
    }
    if pid == 0 {
        return false;
    }
    match (
        process_integrity_level(pid),
        process_integrity_level(std::process::id()),
    ) {
        (Some(t), Some(o)) => elevation_blocks_paste(t, o),
        _ => {
            cvlog!("[cv] paste: target integrity query failed — proceeding (fail-open)");
            false
        }
    }
}

/// Hides the zoom preview when the real cursor has left BOTH the panel and
/// the preview window for ~450ms. Position-based, so no event-order races.
pub fn spawn_zoom_watchdog(app: tauri::AppHandle<tauri::Wry>) {
    std::thread::spawn(move || {
        let mut misses = 0u32;
        let mut ticks: u64 = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(150));
            ticks += 1;
            // daily WAL checkpoint keeps clips.db-wal from growing unbounded
            // on always-on machines (SQLite only auto-checkpoints on writes)
            if ticks % (8 * 60 * 60 * 7) == 0 {
                if let Some(db) = app.try_state::<crate::db::Db>() {
                    let conn = db.0.lock().unwrap_or_else(|p| p.into_inner());
                    let _: Result<(), _> =
                        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
                }
            }
            // this tick doubles as the outside-click poller (one thread instead
            // of two): the hook only sets the flag, hiding from the hook thread
            // itself risks deadlock — here we are a plain worker thread.
            // Checked EVERY iteration: blocking sleeps must not delay the hide.
            if OUTSIDE_CLICK.swap(false, Ordering::SeqCst) {
                crate::commands::hide_panel(&app);
            }
            // validated live — a stale/recycled HWND must not keep the
            // watchdog spinning on an unrelated window
            let zh = live_zoom_hwnd(ZOOM_HWND.load(Ordering::SeqCst));
            if zh == 0 {
                misses = 0;
                continue;
            }
            let visible = unsafe { IsWindowVisible(HWND(zh as _)) }.as_bool();
            if !visible {
                misses = 0;
                continue;
            }
            let (x, y) = get_cursor_pos();
            if point_in_panel(x, y) {
                misses = 0; // cursor still over panel or preview — keep it open
                continue;
            }
            // one miss isn't enough (cursor may be travelling toward the
            // preview): 3 consecutive ticks (~450ms) without re-entering
            misses += 1;
            if misses >= 3 {
                misses = 0;
                if let Some(z) = app.get_webview_window("zoom") {
                    let _ = z.hide();
                }
            }
        }
    });
}

#[cfg(test)]
mod elevation_tests {
    use super::*;

    /// The paste path must refuse targets running at a HIGHER integrity
    /// level: UIPI silently drops our keybd_event there — the paste would
    /// "succeed" (clipboard overwritten, row marked used) while nothing is
    /// pasted. Own-level and lower targets stay allowed.
    #[test]
    fn own_process_integrity_level_is_queryable_and_decision_matrix_holds() {
        let own = process_integrity_level(std::process::id())
            .expect("own token integrity must be queryable");
        assert!(own >= 0x1000, "sanity: IL RID at least LOW, got {own:#x}");
        assert!(!elevation_blocks_paste(own, own), "same IL → allowed");
        assert!(
            !elevation_blocks_paste(0x1000, 0x2000),
            "lower target → allowed"
        );
        assert!(
            elevation_blocks_paste(0x3000, 0x2000),
            "high-IL target from medium-IL process → must block (UIPI)"
        );
    }
}

#[cfg(test)]
mod zoom_hwnd_tests {
    use super::*;

    /// Real (hidden) popup window for the positive case — identity facts
    /// come from the OS, not fixtures.
    fn test_popup_window() -> HWND {
        unsafe {
            static ONCE: std::sync::Once = std::sync::Once::new();
            let hmod = GetModuleHandleW(None).unwrap_or_default();
            ONCE.call_once(|| {
                let name: Vec<u16> = "ClipVaultZoomTest\0".encode_utf16().collect();
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(test_popup_wndproc),
                    lpszClassName: PCWSTR(name.as_ptr()),
                    hInstance: HINSTANCE(hmod.0),
                    ..Default::default()
                };
                RegisterClassW(&wc);
            });
            let cls: Vec<u16> = "ClipVaultZoomTest\0".encode_utf16().collect();
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(cls.as_ptr()),
                PCWSTR(cls.as_ptr()),
                WS_POPUP,
                0,
                0,
                40,
                30,
                HWND::default(),
                None,
                HINSTANCE(hmod.0),
                None,
            )
            .expect("test popup window")
        }
    }

    unsafe extern "system" fn test_popup_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    /// The zoom idle reaper destroys the zoom window but ZOOM_HWND was never
    /// cleared — Win32 recycles HWND values, so the stale handle eventually
    /// pointed at an unrelated live window, breaking outside-click hiding
    /// and paste-target capture. Readers must validate and self-heal.
    #[test]
    fn stale_zoom_hwnd_is_treated_as_absent() {
        assert_eq!(live_zoom_hwnd(0), 0);
        assert_eq!(
            live_zoom_hwnd(0x0DEFACED),
            0,
            "a fabricated/recycled handle must read as absent"
        );
        let w = test_popup_window();
        assert!(!w.0.is_null());
        assert_eq!(
            live_zoom_hwnd(w.0 as isize),
            w.0 as isize,
            "a live window passes through"
        );
        unsafe {
            let _ = DestroyWindow(w);
        }
        assert_eq!(
            live_zoom_hwnd(w.0 as isize),
            0,
            "a destroyed window reads as absent"
        );
    }
}

#[cfg(test)]
mod alt_tap_tests {
    use super::*;

    /// The in-process keybd_event ALT tap is the same primitive the project
    /// proved gets silently swallowed (the injector process exists because
    /// of it). An unverified tap must fall back to the helper process — the
    /// only injection path with proven delivery — before the restore is
    /// judged as failed.
    #[test]
    fn unverified_alt_tap_must_fall_back_to_the_helper_process() {
        assert!(needs_helper_alt_fallback(false));
        assert!(!needs_helper_alt_fallback(true));
    }

    /// The verification primitive runs against the real OS keyboard state
    /// (GetAsyncKeyState between down and up). Whether the tap itself lands
    /// is session-dependent — the contract under test is that the check is
    /// performed and acted on, not that input injection succeeds in CI.
    #[test]
    fn tap_alt_verified_runs_against_the_real_keyboard_state() {
        let _ = tap_alt_verified();
    }
}

#[cfg(test)]
mod focus_contract_tests {
    use super::*;

    // ---------- pure decision matrix (contract 8+2 scenarios) ----------

    fn check(
        child: isize,
        target: isize,
        alive: bool,
        same_thread: bool,
        descendant: bool,
    ) -> ChildCheck {
        classify_child(child, target, alive, same_thread, descendant)
    }

    #[test]
    fn classify_matrix_covers_every_contract_branch() {
        // no captured child → nothing to restore (top-level-only strategy)
        assert_eq!(
            check(0, 101, false, false, false),
            ChildCheck::NoCapturedChild
        );
        // child == target: IsChild(parent, parent) is false by definition —
        // self-focus is a VALID identity, never misjudged as invalid
        assert_eq!(check(101, 101, true, true, false), ChildCheck::RestoreSelf);
        assert_eq!(
            check(101, 101, false, true, false),
            ChildCheck::ChildDestroyed
        );
        // live child inside the verified target
        assert_eq!(check(5, 101, true, true, true), ChildCheck::RestoreChild);
        // captured control destroyed → explicit state, caller decides by
        // attribution (never auto-loosened here)
        assert_eq!(
            check(5, 101, false, true, false),
            ChildCheck::ChildDestroyed
        );
        // live but identity doesn't match: reused handle / foreign window /
        // wrong thread → mismatch, paste must cancel
        assert_eq!(
            check(5, 101, true, false, false),
            ChildCheck::IdentityMismatch
        );
        assert_eq!(
            check(5, 101, true, true, false),
            ChildCheck::IdentityMismatch
        );
    }

    #[test]
    fn capture_prefers_hwndfocus_caret_is_auxiliary_only() {
        // a real focused child wins over the caret
        assert_eq!(capture_candidate(7, 9, 101), 7);
        // WinForms quirk: form reported as hwndFocus while the caret sits on
        // the actual control — caret used as the finer identity here
        assert_eq!(capture_candidate(101, 9, 101), 9);
        // focus unknown entirely → caret as auxiliary
        assert_eq!(capture_candidate(0, 9, 101), 9);
        // top-level self-focus, no caret → the target itself is recorded
        assert_eq!(capture_candidate(101, 0, 101), 101);
        // nothing known at all
        assert_eq!(capture_candidate(0, 0, 101), 0);
    }

    #[test]
    fn capture_gate_stores_only_provable_identities() {
        assert_eq!(captured_child_gate(101, 101, true, false), 101); // self
        assert_eq!(captured_child_gate(9, 101, true, true), 9); // live descendant
        assert_eq!(captured_child_gate(9, 101, true, false), 0); // foreign window
        assert_eq!(captured_child_gate(9, 101, false, true), 0); // other thread
        assert_eq!(captured_child_gate(0, 101, true, true), 0); // nothing
    }

    #[test]
    fn attribution_requires_provably_verified_target() {
        let desc = |h: isize| h == 9; // pretend 9 is a descendant of 101
                                      // foreground is not the target → attribution unknown
        assert_eq!(attributed_focus(202, Some(9), 101, &desc), None);
        // foreground matches but the OS reports no/unknown focus
        assert_eq!(attributed_focus(101, None, 101, &desc), None);
        assert_eq!(attributed_focus(101, Some(0), 101, &desc), None);
        // focus provably inside the verified target (self or descendant)
        assert_eq!(attributed_focus(101, Some(101), 101, &desc), Some(101));
        assert_eq!(attributed_focus(101, Some(9), 101, &desc), Some(9));
        // thread reports focus OUTSIDE the target → not attributable
        assert_eq!(attributed_focus(101, Some(77), 101, &desc), None);
    }

    // ---------- real Win32 identity checks (hidden windows, no input) ----

    /// DefWindowProcW is generic in windows-0.58 and can't coerce to a
    /// WNDPROC directly — wrap it like the clipboard listener does.
    unsafe extern "system" fn test_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    /// Creates a real (hidden) window of a test-only class. Pass a real
    /// parent for child windows, HWND::default() for a top-level — identity
    /// facts then come from the actual OS.
    fn test_window(parent: HWND, style: WINDOW_STYLE) -> HWND {
        unsafe {
            static ONCE: std::sync::Once = std::sync::Once::new();
            let hmod = GetModuleHandleW(None).unwrap_or_default();
            ONCE.call_once(|| {
                let name: Vec<u16> = "ClipVaultFocusTest\0".encode_utf16().collect();
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(test_wndproc),
                    lpszClassName: PCWSTR(name.as_ptr()),
                    hInstance: HINSTANCE(hmod.0),
                    ..Default::default()
                };
                RegisterClassW(&wc);
            });
            let cls: Vec<u16> = "ClipVaultFocusTest\0".encode_utf16().collect();
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(cls.as_ptr()),
                PCWSTR(cls.as_ptr()),
                style,
                0,
                0,
                60,
                40,
                parent,
                None,
                HINSTANCE(hmod.0),
                None,
            )
            .expect("test window creation")
        }
    }

    #[test]
    fn win32_identity_facts_use_real_windows() {
        let parent = test_window(HWND::default(), WS_POPUP);
        assert_ne!(parent.0 as isize, 0, "parent window must exist");
        let child = test_window(parent, WS_CHILD);
        assert_ne!(child.0 as isize, 0, "child window must exist");
        let t = parent.0 as isize;
        let c = child.0 as isize;

        // facts through the exact gatherers restore_focus_verified uses
        let alive = child_alive(c);
        let same_thread = window_thread_id(c) == window_thread_id(t);
        let descendant = is_descendant_of(c, t);
        assert!(alive && same_thread && descendant, "real child facts");
        assert_eq!(
            classify_child(c, t, alive, same_thread, descendant),
            ChildCheck::RestoreChild
        );

        // an unrelated top-level window is NOT inside the target
        let other = test_window(HWND::default(), WS_POPUP);
        let o = other.0 as isize;
        let facts = (
            child_alive(o),
            window_thread_id(o) == window_thread_id(t),
            is_descendant_of(o, t),
        );
        assert_eq!(
            classify_child(o, t, facts.0, facts.1, facts.2),
            ChildCheck::IdentityMismatch
        );

        // a fabricated handle is a destroyed child → fail closed
        let fake: isize = 0x0DEFACED;
        assert!(!child_alive(fake));
        assert_eq!(
            classify_child(fake, t, child_alive(fake), false, false),
            ChildCheck::ChildDestroyed
        );

        unsafe {
            let _ = DestroyWindow(child);
            let _ = DestroyWindow(other);
            let _ = DestroyWindow(parent);
        }
    }
}
