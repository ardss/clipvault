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
pub use injector::{focus_hwnd_take, injector_loop, request_paste_keystroke};
pub use window::{
    capture_focus, current_focus_attribution, get_cursor_pos, get_foreground, get_window_rect,
    log_send_time, point_in_panel, restore_focus_verified, restore_foreground, spawn_zoom_watchdog,
    update_panel_rect, FocusRestore,
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
            eprintln!("[cv] listener: GetModuleHandleW failed — capture disabled");
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
            eprintln!(
                "[cv] listener window create failed (attempt {})",
                attempt + 1
            );
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
        let Ok(hwnd) = hwnd else {
            eprintln!("[cv] listener: window creation failed after retries — capture disabled");
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
/// visibility.
pub fn hide_single_instance_window() {
    unsafe {
        let class: Vec<u16> = "com.clipvault.app-sic\0".encode_utf16().collect();
        let name: Vec<u16> = "com.clipvault.app-siw\0".encode_utf16().collect();
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
static MOUSE_HOOK_CMD: StdMutex<Option<std::sync::mpsc::Sender<bool>>> = StdMutex::new(None);
static MOUSE_HOOK_THREAD: AtomicBool = AtomicBool::new(false);

/// The low-level mouse hook is only needed while the panel is visible —
/// installing on show and uninstalling on hide (a) removes a permanent
/// global hook from the system and (b) self-heals: a hook silently removed
/// by the system (low-level timeout) is reinstalled on the next show. The
/// previous install-once-forever never recovered (audit P3).
///
/// The hook's callback is invoked on the thread that installed it, and that
/// thread MUST pump messages — so install/uninstall happen on a dedicated
/// hook thread driven by a command channel, with a fast PeekMessage pump.
pub fn set_mouse_hook(on: bool) {
    if MOUSE_HOOK_DESIRED.swap(on, Ordering::SeqCst) == on {
        return;
    }
    let tx = {
        let mut g = MOUSE_HOOK_CMD.lock().unwrap_or_else(|p| p.into_inner());
        if !MOUSE_HOOK_THREAD.swap(true, Ordering::SeqCst) {
            let (tx, rx) = std::sync::mpsc::channel::<bool>();
            *g = Some(tx);
            std::thread::spawn(move || {
                let mut hook: Option<HHOOK> = None;
                loop {
                    // drain pending commands (non-blocking)
                    if let Ok(on) = rx.try_recv() {
                        if on && hook.is_none() {
                            match unsafe {
                                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0)
                            } {
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
                    }
                    // pump: the low-level hook requires an active message pump
                    // on the installing thread, or the system drops the hook
                    unsafe {
                        let mut msg = MSG::default();
                        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                            let _ = TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(40));
                }
            });
        }
        g.clone()
    };
    if let Some(tx) = tx {
        let _ = tx.send(on);
    } else {
        // command channel missing — undo the desired flag so a later call retries
        MOUSE_HOOK_DESIRED.store(!on, Ordering::SeqCst);
    }
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
