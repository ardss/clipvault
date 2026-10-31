//! L3 clipboard-listener probe (TESTING_ARCHITECTURE_PLAN §3.3): behavior-level
//! proof that a registered listener (AddClipboardFormatListener →
//! WM_CLIPBOARDUPDATE) observes external clipboard writes within a timeout and
//! that latest-wins holds for rapid successive writes.
//!
//! SAFETY (plan §1.3-3 / §3.3): this probe WRITES THE REAL SYSTEM CLIPBOARD.
//! It MUST only run behind the `CV_E2E_ISOLATED=1` gate via
//! `scripts/run-native-probes.ps1`, which preserves the user's clipboard
//! before the run (TESTING.md §6) and restores it after, with the production
//! instance stopped (otherwise the single-instance plugin and the live
//! listener make the idle-gated design meaningless). Without the gate it
//! refuses with a non-zero exit. It is `#[ignore]`d so no bare `cargo test`
//! ever reaches it (probes never run in GH Actions).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const UPDATE_TIMEOUT: Duration = Duration::from_secs(5);
/// Raw Win32 clipboard format id — same constant as win/mod.rs:14.
const CF_UNICODETEXT: u32 = 13;

/// Gate: probes refuse to run without explicit isolation. exit(1) is
/// intentional — the whole test binary must end non-zero so the executor's
/// gate check is impossible to miss.
fn require_isolation_gate() {
    if std::env::var("CV_E2E_ISOLATED").as_deref() != Ok("1") {
        eprintln!(
            "REFUSED: clipboard_listener_probe writes the real system clipboard. \
             It requires the isolation gate CV_E2E_ISOLATED=1 and must be run via \
             scripts/run-native-probes.ps1 (stop production instance → preserve \
             clipboard → probe → restore → restart & verify). See \
             TESTING_ARCHITECTURE_PLAN §1.3-3 and TESTING.md §6."
        );
        std::process::exit(1);
    }
}

static UPDATES: AtomicUsize = AtomicUsize::new(0);

/// A hidden message window receiving WM_CLIPBOARDUPDATE. The window and its
/// message pump live on the SAME thread (the test's main thread) — window
/// messages are delivered to the creating thread's queue.
unsafe fn spawn_listener_window() -> windows::Win32::Foundation::HWND {
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe extern "system" fn wndproc(
        hwnd: windows::Win32::Foundation::HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_CLIPBOARDUPDATE {
            UPDATES.fetch_add(1, Ordering::SeqCst);
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    let class_name = windows::core::w!("cv_clipboard_listener_probe");
    let hinstance = unsafe { GetModuleHandleW(None) }.expect("module handle");
    let hinstance = windows::Win32::Foundation::HINSTANCE(hinstance.0);
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        lpszClassName: class_name,
        hInstance: hinstance,
        ..Default::default()
    };
    let atom = unsafe { RegisterClassW(&wc) };
    assert_ne!(atom, 0, "RegisterClassW");
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            class_name,
            WINDOW_STYLE(0), // hidden
            0,
            0,
            0,
            0,
            None,
            None,
            hinstance,
            None,
        )
    }
    .expect("listener window");
    unsafe {
        windows::Win32::System::DataExchange::AddClipboardFormatListener(hwnd)
            .expect("AddClipboardFormatListener");
    }
    hwnd
}

/// Pumps this thread's message queue until at least one more update arrives
/// or the deadline passes. Returns the number of updates seen in total.
unsafe fn pump_until_updates(min_total: usize, deadline: Instant) -> usize {
    use windows::Win32::UI::WindowsAndMessaging::*;
    loop {
        if UPDATES.load(Ordering::SeqCst) >= min_total {
            return UPDATES.load(Ordering::SeqCst);
        }
        if Instant::now() > deadline {
            return UPDATES.load(Ordering::SeqCst);
        }
        let mut msg = Default::default();
        // non-blocking peek: we poll the atomic counter too (the update is
        // posted from the system to our wndproc, dispatched right here)
        let got = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) };
        if got.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else {
            std::thread::sleep(Duration::from_micros(500));
        }
    }
}

/// Writes `text` to the system clipboard as CF_UNICODETEXT (synchronous,
/// plain Win32 — this probe is intentionally self-contained, no tauri state).
unsafe fn write_clipboard_text(text: &str) {
    use windows::Win32::Foundation::{HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::*;

    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * 2;
    unsafe {
        // Other desktop processes (IMEs, remote-assist tools) hold the
        // clipboard open transiently; OpenClipboard fails with
        // ACCESS_DENIED while they do. Retry well past their hold times.
        let mut opened = false;
        for _ in 0..50 {
            if OpenClipboard(None).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(opened, "OpenClipboard: held by another process for >1s");
        EmptyClipboard().expect("EmptyClipboard");
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes).expect("GlobalAlloc");
        let h = HGLOBAL(h.0);
        let dst = GlobalLock(h);
        assert!(!dst.is_null(), "GlobalLock");
        std::ptr::copy_nonoverlapping(wide.as_ptr(), dst as *mut u16, wide.len());
        let _ = GlobalUnlock(h);
        SetClipboardData(CF_UNICODETEXT, HANDLE(h.0)).expect("SetClipboardData");
        let _ = CloseClipboard();
    }
}

/// Reads back the current CF_UNICODETEXT content (caller holds the gate).
unsafe fn read_clipboard_text() -> String {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::*;

    unsafe {
        let mut opened = false;
        for _ in 0..50 {
            if OpenClipboard(None).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            opened,
            "OpenClipboard for read-back: held by another process for >1s"
        );
        let h = GetClipboardData(CF_UNICODETEXT).expect("GetClipboardData");
        let h = HGLOBAL(h.0);
        let ptr = GlobalLock(h);
        assert!(!ptr.is_null(), "GlobalLock");
        // bound the read to the real allocation (GlobalSize returns bytes);
        // CF_UNICODETEXT is NUL-terminated, so stop at the NUL within bounds
        let max_u16 = (GlobalSize(h) / 2).max(1);
        let wide = std::slice::from_raw_parts(ptr as *const u16, max_u16);
        let mut s = String::new();
        for &u in wide {
            if u == 0 {
                break;
            }
            s.push(char::from_u32(u as u32).unwrap_or('\u{FFFD}'));
        }
        let _ = GlobalUnlock(h);
        let _ = CloseClipboard();
        s
    }
}

#[test]
#[ignore = "writes the real system clipboard; only via scripts/run-native-probes.ps1 \
            behind CV_E2E_ISOLATED=1"]
fn listener_observes_external_write_and_rapid_writes_settle_on_the_last() {
    require_isolation_gate();

    let hwnd = unsafe { spawn_listener_window() };
    let _guard = {
        struct RemoveListener(windows::Win32::Foundation::HWND);
        impl Drop for RemoveListener {
            fn drop(&mut self) {
                unsafe {
                    let _ =
                        windows::Win32::System::DataExchange::RemoveClipboardFormatListener(self.0);
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.0);
                }
            }
        }
        RemoveListener(hwnd)
    };
    // let the listener registration settle
    std::thread::sleep(Duration::from_millis(200));

    // 1) a single external write must be observed within the timeout
    UPDATES.store(0, Ordering::SeqCst);
    unsafe { write_clipboard_text("cv-probe:listener-1") };
    let n = unsafe { pump_until_updates(1, Instant::now() + UPDATE_TIMEOUT) };
    assert!(
        n >= 1,
        "WM_CLIPBOARDUPDATE must arrive within {UPDATE_TIMEOUT:?} after an \
         external clipboard write"
    );

    // 2) latest-wins on the write side: three rapid writes, read-back must
    //    return the LAST payload, never an interleaved torn mix
    unsafe { write_clipboard_text("cv-probe:rapid-1") };
    unsafe { write_clipboard_text("cv-probe:rapid-2") };
    unsafe { write_clipboard_text("cv-probe:rapid-3") };
    let got = unsafe { read_clipboard_text() };
    assert_eq!(
        got, "cv-probe:rapid-3",
        "the LAST write must be the settled clipboard content"
    );

    // 3) the rapid writes are also observed as updates (latest-wins over the
    //    event stream: at least one update for the trio; per-write dedup is
    //    the capture layer's job, not the OS listener's)
    let n = unsafe { pump_until_updates(2, Instant::now() + UPDATE_TIMEOUT) };
    assert!(
        n >= 2,
        "rapid successive writes must still be observed as updates"
    );
}
