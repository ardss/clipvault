use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;
use tauri::Manager;
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const CF_UNICODETEXT: u32 = 13;
pub const CF_DIB: u32 = 8;

pub static SELF_WRITE_SEQ: AtomicIsize = AtomicIsize::new(-1);
pub static PASTE_TARGET: AtomicIsize = AtomicIsize::new(0);
/// Owner PID of PASTE_TARGET, captured at the same moment. An HWND value can
/// be recycled by an unrelated window after its owner exits, so a live handle
/// alone is not proof the original destination still exists.
pub static PASTE_TARGET_PID: AtomicUsize = AtomicUsize::new(0);
pub static PANEL_VISIBLE: AtomicBool = AtomicBool::new(false);
pub static PANEL_HWND: AtomicIsize = AtomicIsize::new(0);
pub static PANEL_RECT: StdMutex<(i32, i32, i32, i32)> = StdMutex::new((0, 0, 0, 0));
pub static OUTSIDE_CLICK: AtomicBool = AtomicBool::new(false);
pub static FOCUS_HWND: AtomicIsize = AtomicIsize::new(0);
pub static ZOOM_HWND: AtomicIsize = AtomicIsize::new(0);

mod autostart;
mod clipboard;
mod dib;
mod formats;
mod injector;
mod window;

// explicit re-exports: only what lib.rs (and the tests) actually use.
// Children import from each other via `super::<module>::<item>`, never
// through these globs.
pub use autostart::set_autostart;
pub use clipboard::{
    clipboard_marked_sensitive, is_self_write, read_clipboard_dib_vec, read_clipboard_text,
    write_clipboard_png, write_clipboard_text,
};
pub use dib::{dib_to_png, png_bytes_to_png};
pub use formats::{
    log_clipboard_formats, read_clipboard_files, read_clipboard_html, read_clipboard_png_raw,
    write_clipboard_files, write_clipboard_html,
};
pub use injector::{focus_hwnd_take, helper_alt_tap, injector_loop, request_paste_keystroke};
pub use window::{
    capture_focus, current_focus_attribution, get_cursor_pos, get_foreground, get_window_rect,
    live_zoom_hwnd, log_send_time, point_in_panel, restore_focus_verified, restore_foreground,
    spawn_zoom_watchdog, target_elevated, update_panel_rect, FocusRestore,
};

// ---------- clipboard listener thread ----------

const WM_CLIPBOARDUPDATE: u32 = 0x031D;

unsafe extern "system" fn listener_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

pub fn spawn_clipboard_listener<F: FnMut() + Send + 'static>(mut on_change: F) {
    // Capture used to run ON the listener thread (including its 120ms settle
    // sleep and image decode): a slow capture delayed the message pump, and a
    // burst of copies queued up serially. Now the listener thread only flags
    // "dirty" and a worker consumes it latest-wins — the clipboard is a
    // state, not an event queue, so skipping straight to the newest content
    // is the correct drain (audit P3).
    let pending = std::sync::Arc::new((StdMutex::new(false), std::sync::Condvar::new()));
    let worker_pending = pending.clone();
    std::thread::spawn(move || {
        let (lock, cv) = &*worker_pending;
        loop {
            {
                let mut flag = lock.lock().unwrap_or_else(|p| p.into_inner());
                while !*flag {
                    flag = cv.wait(flag).unwrap_or_else(|p| p.into_inner());
                }
                *flag = false;
            }
            // settle: writers put formats on the clipboard in steps
            std::thread::sleep(std::time::Duration::from_millis(120));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut on_change));
            if result.is_err() {
                cvlog!("[cv] capture handler panicked; worker continues");
            }
        }
    });
    std::thread::spawn(move || unsafe {
        cvlog!("[cv] listener thread started");
        let Ok(hmod) = GetModuleHandleW(None) else {
            // capture dies silently in cv.log unless THIS is the record —
            // stderr is invisible for a GUI app
            cvlog!("[cv] listener: GetModuleHandleW failed — capture disabled");
            return;
        };
        let class_name: Vec<u16> = "ClipVaultListener\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(listener_wndproc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hInstance: HINSTANCE(hmod.0),
            hbrBackground: HBRUSH::default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        // window creation can fail under desktop-heap pressure (long RDP
        // sessions) — a panic here would kill capture silently; retry instead
        let mut hwnd = Err(windows::core::Error::empty());
        for attempt in 0..5 {
            hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(class_name.as_ptr()),
                PCWSTR(class_name.as_ptr()),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                None,
                None,
                HINSTANCE(hmod.0),
                None,
            );
            if hwnd.is_ok() {
                break;
            }
            cvlog!(
                "[cv] listener window create failed (attempt {})",
                attempt + 1
            );
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
        let Ok(hwnd) = hwnd else {
            cvlog!("[cv] listener: window creation failed after retries — capture disabled");
            return;
        };
        if AddClipboardFormatListener(hwnd).is_ok() {
            // seed whatever is already on the clipboard: copies made before
            // the listener registered would otherwise be silently missed
            std::thread::sleep(std::time::Duration::from_millis(400));
            {
                let (lock, cv) = &*pending;
                let mut flag = lock.lock().unwrap_or_else(|p| p.into_inner());
                *flag = true;
                cv.notify_one();
            }
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_CLIPBOARDUPDATE {
                let (lock, cv) = &*pending;
                let mut flag = lock.lock().unwrap_or_else(|p| p.into_inner());
                *flag = true;
                cv.notify_one();
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

// ---------- single-instance event window (audit P2-1) ----------

/// tauri-plugin-single-instance creates its internal event window with the
/// title "<identifier>-siw" and force-shows it — it leaks into Alt-Tab and
/// Task View. Hiding it is lossless: second instances find it by class name
/// ("<identifier>-sic") to deliver WM_COPYDATA, which works regardless of
/// visibility. The pair is derived from the RUNTIME identifier (the plugin
/// uses app.config().identifier) — a hardcoded copy broke the moment the
/// identifier changed and misreported the miss as "plugin timing?".
fn single_instance_window_names(identifier: &str) -> (Vec<u16>, Vec<u16>) {
    (
        format!("{identifier}-sic\0").encode_utf16().collect(),
        format!("{identifier}-siw\0").encode_utf16().collect(),
    )
}

pub fn hide_single_instance_window(identifier: &str) {
    unsafe {
        let (class, name) = single_instance_window_names(identifier);
        match FindWindowW(PCWSTR(class.as_ptr()), PCWSTR(name.as_ptr())) {
            Ok(h) if !h.0.is_null() => {
                let _ = ShowWindow(h, SW_HIDE);
                cvlog!("[cv] single-instance event window hidden");
            }
            _ => {
                cvlog!("[cv] single-instance event window not found (plugin timing?)");
            }
        }
    }
}

// ---------- low-level mouse hook (click outside hides panel) ----------

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(clippy::upper_case_acronyms)] // Win32 API name — keep the correspondence obvious
struct MSLLHOOKSTRUCT {
    pt: POINT,
    _extra: [u8; 24],
}

unsafe extern "system" fn mouse_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code >= 0 && w_param.0 as u32 == WM_LBUTTONDOWN {
        let p = *(l_param.0 as *const MSLLHOOKSTRUCT);
        if PANEL_VISIBLE.load(Ordering::SeqCst) && !point_in_panel(p.pt.x, p.pt.y) {
            // flag only — the main thread polls and hides (hiding from the hook
            // thread risks deadlock)
            OUTSIDE_CLICK.store(true, Ordering::SeqCst);
        }
    }
    CallNextHookEx(None, n_code, w_param, l_param)
}

static MOUSE_HOOK_DESIRED: AtomicBool = AtomicBool::new(false);
static MOUSE_HOOK_THREAD: AtomicBool = AtomicBool::new(false);
/// Thread id of the dedicated hook thread (0 until it runs). Install/uninstall
/// commands are delivered as thread messages so the pump can block in
/// GetMessageW. This is not a style choice: the low-level hook callback runs
/// on the installing thread, and the system holds every mouse event until
/// that thread pumps it — a PeekMessage+sleep loop here once slept 40 ms per
/// batch and made the cursor visibly lag system-wide for as long as the
/// panel (and therefore the hook) was open.
static MOUSE_HOOK_TID: AtomicUsize = AtomicUsize::new(0);

const WM_HOOK_CMD: u32 = WM_APP + 1;

fn hook_thread_body() {
    MOUSE_HOOK_TID.store(unsafe { GetCurrentThreadId() } as usize, Ordering::SeqCst);
    let mut hook: Option<HHOOK> = None;
    loop {
        let mut msg = MSG::default();
        // blocking wait: zero CPU while idle, dispatches hook callbacks the
        // instant they arrive (never sleep between pumps — see MOUSE_HOOK_TID)
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if !r.as_bool() {
            continue;
        }
        if msg.message == WM_HOOK_CMD {
            let on = msg.wParam.0 != 0;
            if on && hook.is_none() {
                match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0) } {
                    Ok(h) => {
                        hook = Some(h);
                        cvlog!("[cv] mouse hook installed");
                    }
                    Err(e) => {
                        cvlog!("[cv] mouse hook install FAILED: {e}");
                    }
                }
            } else if !on {
                if let Some(h) = hook.take() {
                    unsafe {
                        let _ = UnhookWindowsHookEx(h);
                    }
                    cvlog!("[cv] mouse hook removed");
                }
            }
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// The low-level mouse hook is only needed while the panel is visible —
/// installing on show and uninstalling on hide (a) removes a permanent
/// global hook from the system and (b) self-heals: a hook silently removed
/// by the system (low-level timeout) is reinstalled on the next show. The
/// previous install-once-forever never recovered (audit P3).
pub fn set_mouse_hook(on: bool) {
    if MOUSE_HOOK_DESIRED.swap(on, Ordering::SeqCst) == on {
        return;
    }
    if !MOUSE_HOOK_THREAD.swap(true, Ordering::SeqCst) {
        std::thread::spawn(hook_thread_body);
    }
    // The thread starts once per process; this retry window only covers the
    // very first toggle racing the thread's own startup.
    for _ in 0..200 {
        let tid = MOUSE_HOOK_TID.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid as u32, WM_HOOK_CMD, WPARAM(on as usize), LPARAM(0));
            }
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cvlog!("[cv] mouse hook thread did not start — outside-click close disabled this session");
    MOUSE_HOOK_DESIRED.store(!on, Ordering::SeqCst);
}

#[cfg(test)]
mod mouse_hook_tests {
    /// Machine-enforced invariant of the 2026-10-31 incident: the low-level
    /// hook callback is dispatched from the hook thread's message pump, and
    /// the system withholds every mouse event until it returns. Any sleep
    /// inside the pump becomes direct system-wide input latency, invisible
    /// to CPU sampling and to every functional test. This guard fails the
    /// build if one is ever reintroduced into the pump or its body.
    #[test]
    fn hook_pump_must_not_sleep() {
        let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/win/mod.rs"));
        let start = src
            .find("fn hook_thread_body")
            .expect("hook_thread_body must exist");
        let end = src[start..]
            .find("pub fn set_mouse_hook")
            .expect("set_mouse_hook must follow hook_thread_body")
            + start;
        let body = &src[start..end];
        assert!(
            !body.contains("sleep("),
            "the hook message pump must not sleep between pumps: system-wide \
             mouse input is held while the hook callback is pending, so any \
             sleep here becomes direct cursor latency (2026-10-31 incident, \
             regression from e3f97da)"
        );
    }
    /// Smoke test for the whole toggle path: thread spawn, thread-message
    /// delivery, hook install and uninstall. The latency property itself
    /// (blocking GetMessageW pump, no sleep between pumps) is a code-review
    /// invariant — see MOUSE_HOOK_TID's doc comment for the 40 ms incident.
    #[test]
    fn mouse_hook_toggle_round_trip_does_not_hang() {
        super::set_mouse_hook(true);
        super::set_mouse_hook(false);
        super::set_mouse_hook(true);
        super::set_mouse_hook(false);
        assert!(!super::MOUSE_HOOK_DESIRED.load(std::sync::atomic::Ordering::SeqCst));
        assert_ne!(
            super::MOUSE_HOOK_TID.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "hook thread must have published its thread id"
        );
    }
}

#[cfg(test)]
mod single_instance_tests {
    /// The plugin derives its window class/name from app.config().identifier
    /// ("{id}-sic"/"{id}-siw"). Our hide call must derive the same pair from
    /// the runtime identifier — a hardcoded copy breaks the moment the
    /// identifier changes (window leaks into Alt-Tab, "plugin timing?"  false
    /// alarm on every launch).
    #[test]
    fn single_instance_window_names_follow_the_runtime_identifier() {
        let decode = |v: &[u16]| {
            String::from_utf16(&v[..v.len() - 1]).unwrap() // drop the NUL
        };
        let (c, n) = super::single_instance_window_names("com.clipvault.test");
        assert_eq!(decode(&c), "com.clipvault.test-sic");
        assert_eq!(decode(&n), "com.clipvault.test-siw");
        let (c2, _) = super::single_instance_window_names("com.clipvault.app");
        assert_ne!(
            decode(&c),
            decode(&c2),
            "different identifiers must yield different window names"
        );
    }
}

/// Serializes tests that mutate the REAL system clipboard. They run in
/// parallel inside one process and clobber each other's content mid-assert
/// (observed: failed_png_write_never_destroys_the_previous_clipboard read
/// another test's payload as its sentinel). Test-only; production paths are
/// serialized by the OS clipboard itself.
#[cfg(test)]
pub(crate) fn clipboard_test_lock() -> &'static StdMutex<()> {
    static LOCK: StdMutex<()> = StdMutex::new(());
    &LOCK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_png(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        let img = image::RgbaImage::from_raw(w, h, {
            let mut v = Vec::new();
            for _ in 0..(w * h) {
                v.extend_from_slice(&px);
            }
            v
        })
        .unwrap();
        let mut c = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut c, image::ImageFormat::Png)
            .unwrap();
        c.into_inner()
    }

    #[test]
    fn png_roundtrip_via_clipboard() {
        let _clipboard = clipboard_test_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let png = solid_png(60, 40, [255, 0, 0, 255]);
        assert!(write_clipboard_png(&png), "write failed");
        let dib = read_clipboard_dib_vec().expect("no dib back");
        let (out, w, h) = dib_to_png(&dib).expect("decode failed");
        assert_eq!((w, h), (60, 40));
        let img = image::load_from_memory(&out).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(5, 5)[0], 255);
        assert_eq!(img.get_pixel(5, 5)[2], 0);
    }

    #[test]
    fn bitfields_dib_decodes() {
        // 32bpp BI_BITFIELDS with 40-byte header + 12-byte mask, like screenshots
        let (w, h) = (4u32, 3u32);
        let mut dib = Vec::new();
        dib.extend_from_slice(&40i32.to_le_bytes());
        dib.extend_from_slice(&(w as i32).to_le_bytes());
        dib.extend_from_slice(&(h as i32).to_le_bytes()); // bottom-up
        dib.extend_from_slice(&1i16.to_le_bytes());
        dib.extend_from_slice(&32i16.to_le_bytes());
        dib.extend_from_slice(&3i32.to_le_bytes()); // BI_BITFIELDS
        dib.extend_from_slice(&((w * h * 4) as i32).to_le_bytes());
        dib.extend_from_slice(&[0u8; 16]);
        // masks: R=0x00FF0000 G=0x0000FF00 B=0x000000FF
        dib.extend_from_slice(&0x00FF0000u32.to_le_bytes());
        dib.extend_from_slice(&0x0000FF00u32.to_le_bytes());
        dib.extend_from_slice(&0x000000FFu32.to_le_bytes());
        for _ in 0..(w * h) {
            dib.extend_from_slice(&[0, 0, 255, 0]); // BGRA red
        }
        let (out, ow, oh) = dib_to_png(&dib).expect("decode failed");
        assert_eq!((ow, oh), (w, h));
        let img = image::load_from_memory(&out).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0)[0], 255, "red channel");
        assert_eq!(img.get_pixel(0, 0)[2], 0, "blue channel");
    }
}
