use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
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
pub static PANEL_VISIBLE: AtomicBool = AtomicBool::new(false);
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
pub use dib::dib_to_png;
pub use formats::{
    log_clipboard_formats, read_clipboard_files, read_clipboard_html, read_clipboard_png_raw,
    write_clipboard_files, write_clipboard_html,
};
pub use injector::{focus_hwnd, injector_loop, request_paste_keystroke};
pub use window::{
    capture_focus, get_cursor_pos, get_foreground, log_send_time, point_in_panel, restore_focus,
    restore_foreground, spawn_zoom_watchdog, update_panel_rect,
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
    std::thread::spawn(move || unsafe {
        cvlog!("[cv] listener thread started");
        let hmod = GetModuleHandleW(None).unwrap();
        let class_name: Vec<u16> = "ClipVaultListener\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(listener_wndproc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hInstance: HINSTANCE(hmod.0),
            hbrBackground: HBRUSH::default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
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
        )
        .unwrap();
        if AddClipboardFormatListener(hwnd).is_ok() {
            // seed whatever is already on the clipboard: copies made before
            // the listener registered would otherwise be silently missed
            std::thread::sleep(std::time::Duration::from_millis(400));
            on_change();
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_CLIPBOARDUPDATE {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut on_change));
                if result.is_err() {
                    cvlog!("[cv] listener handler panicked; thread continues");
                }
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
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
        cvlog!(
            "[cv] hook click at ({},{}) visible={} in_panel={}",
            p.pt.x,
            p.pt.y,
            PANEL_VISIBLE.load(Ordering::SeqCst),
            point_in_panel(p.pt.x, p.pt.y)
        );
        if PANEL_VISIBLE.load(Ordering::SeqCst) && !point_in_panel(p.pt.x, p.pt.y) {
            // flag only — the main thread polls and hides (hiding from the hook
            // thread risks deadlock)
            OUTSIDE_CLICK.store(true, Ordering::SeqCst);
        }
    }
    CallNextHookEx(None, n_code, w_param, l_param)
}

pub fn install_mouse_hook() {
    std::thread::spawn(|| unsafe {
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0);
        let _ = hook;
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
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
