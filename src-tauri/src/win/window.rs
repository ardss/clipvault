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
/// before the injector fires (paste-loss triage).
pub fn log_send_time() {
    unsafe {
        let fg = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let _ = GetGUIThreadInfo(thread, &mut gi);
        eprintln!(
            "[cv] send-time: fg={:x} focus={:x}",
            fg.0 as usize, gi.hwndFocus.0 as usize
        );
    }
}

pub fn update_panel_rect(hwnd: isize) {
    unsafe {
        let mut r = RECT::default();
        if GetWindowRect(HWND(hwnd as _), &mut r).is_ok() {
            *PANEL_RECT.lock().unwrap() = (r.left, r.top, r.right, r.bottom);
        }
    }
}

pub fn point_in_panel(x: i32, y: i32) -> bool {
    let (l, t, r, b) = *PANEL_RECT.lock().unwrap();
    if x >= l && x <= r && y >= t && y <= b {
        return true;
    }
    // the zoom preview window belongs to the panel UI
    let zh = ZOOM_HWND.load(Ordering::SeqCst);
    if zh != 0 {
        unsafe {
            let mut r2 = RECT::default();
            if GetWindowRect(HWND(zh as _), &mut r2).is_ok() {
                return x >= r2.left && x <= r2.right && y >= r2.top && y <= r2.bottom;
            }
        }
    }
    false
}

/// Remembers which child control inside `target` currently has keyboard focus,
/// so it can be restored after the panel (which takes focus) is hidden.
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
            // the caret owner is where typing lands; WinForms reports the
            // top-level form as hwndFocus but keeps hwndCaret on the control
            let best = if !gi.hwndCaret.0.is_null() {
                gi.hwndCaret
            } else {
                gi.hwndFocus
            };
            eprintln!(
                "[cv] capture_focus: target={target:x} focus={:x} caret={:x}",
                gi.hwndFocus.0 as usize, gi.hwndCaret.0 as usize
            );
            FOCUS_HWND.store(best.0 as isize, Ordering::SeqCst);
        } else {
            cvlog!("[cv] capture_focus: GetGUIThreadInfo failed");
        }
    }
}

/// Re-attaches keyboard focus to the control remembered by capture_focus.
pub fn restore_focus(target: isize) -> bool {
    let fh = FOCUS_HWND.swap(0, Ordering::SeqCst);
    if target == 0 || fh == 0 {
        return false;
    }
    unsafe {
        let thread = GetWindowThreadProcessId(HWND(target as _), None);
        let this_thread = GetCurrentThreadId();
        let _ = AttachThreadInput(this_thread, thread, true);
        let ok = SetFocus(HWND(fh as _)).is_ok();
        let _ = AttachThreadInput(this_thread, thread, false);
        cvlog!("[cv] restore_focus: fh={fh:x} ok={ok}");
        ok
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
        // ALT tap releases the foreground lock
        keybd_event(VK_MENU.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
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
        eprintln!(
            "[cv] restore: target={:x} fg_now={:x} err={:?}",
            target,
            GetForegroundWindow().0 as usize,
            GetLastError()
        );
        false
    }
}

/// Hides the zoom preview when the real cursor has left BOTH the panel and
/// the preview window for ~450ms. Position-based, so no event-order races.
pub fn spawn_zoom_watchdog(app: tauri::AppHandle<tauri::Wry>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(150));
        let zh = ZOOM_HWND.load(Ordering::SeqCst);
        if zh == 0 {
            continue;
        }
        let visible = unsafe { IsWindowVisible(HWND(zh as _)) }.as_bool();
        if !visible {
            continue;
        }
        let (x, y) = get_cursor_pos();
        if point_in_panel(x, y) {
            continue; // cursor still over panel or preview — keep it open
        }
        // one miss isn't enough (cursor may be travelling toward the preview)
        std::thread::sleep(std::time::Duration::from_millis(150));
        let (x2, y2) = get_cursor_pos();
        if point_in_panel(x2, y2) {
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
        let (x3, y3) = get_cursor_pos();
        if point_in_panel(x3, y3) {
            continue;
        }
        if let Some(z) = app.get_webview_window("zoom") {
            let _ = z.hide();
        }
    });
}
