use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex as StdMutex;
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use tauri::Manager;

pub const CF_UNICODETEXT: u32 = 13;
pub const CF_DIB: u32 = 8;

pub static SELF_WRITE_UNTIL: AtomicIsize = AtomicIsize::new(0);
pub static PASTE_TARGET: AtomicIsize = AtomicIsize::new(0);
pub static PANEL_VISIBLE: AtomicBool = AtomicBool::new(false);
pub static PANEL_RECT: StdMutex<(i32, i32, i32, i32)> = StdMutex::new((0, 0, 0, 0));
pub static OUTSIDE_CLICK: AtomicBool = AtomicBool::new(false);
pub static FOCUS_HWND: AtomicIsize = AtomicIsize::new(0);
pub static ZOOM_HWND: AtomicIsize = AtomicIsize::new(0);

// ---------- clipboard read ----------

fn open_clipboard_retry() -> bool {
    for ms in [0u32, 15, 35, 75, 150] {
        if ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(ms as u64));
        }
        unsafe {
            if OpenClipboard(None).is_ok() {
                return true;
            }
        }
    }
    false
}

pub fn read_clipboard_text() -> Option<String> {
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() } {
        cvlog!("[cv] read: no CF_UNICODETEXT");
        return None;
    }
    if !open_clipboard_retry() {
        cvlog!("[cv] read: OpenClipboard failed");
        return None;
    }
    unsafe {
        let h = match GetClipboardData(CF_UNICODETEXT) { Ok(h) => h, Err(e) => { cvlog!("[cv] read: GetData err {e}"); let _ = CloseClipboard(); return None; } };
        let hg = HGLOBAL(h.0);
        let ptr = GlobalLock(hg) as *const u16;
        if ptr.is_null() {
            cvlog!("[cv] read: GlobalLock null");
            let _ = CloseClipboard();
            return None;
        }
        let max_units = GlobalSize(hg) / 2;
        let mut len = 0usize;
        while len < max_units && *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        let s = String::from_utf16_lossy(slice);
        let _ = GlobalUnlock(hg);
        let _ = CloseClipboard();
        if s.trim().is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

pub fn read_clipboard_dib_vec() -> Option<Vec<u8>> {
    if unsafe { IsClipboardFormatAvailable(CF_DIB).is_err() } {
        return None;
    }
    if !open_clipboard_retry() {
        return None;
    }
    unsafe {
        let h = GetClipboardData(CF_DIB).ok()?;
        let hg = HGLOBAL(h.0);
        let ptr = GlobalLock(hg) as *const u8;
        if ptr.is_null() {
            let _ = CloseClipboard();
            return None;
        }
        let size = GlobalSize(hg);
        let dib = std::slice::from_raw_parts(ptr, size).to_vec();
        let _ = GlobalUnlock(hg);
        let _ = CloseClipboard();
        Some(dib)
    }
}

fn read_i32(dib: &[u8], off: usize) -> i32 {
    i32::from_le_bytes([dib[off], dib[off + 1], dib[off + 2], dib[off + 3]])
}

/// DIB (packed or with header) → RGBA → PNG bytes.
pub fn dib_to_png(dib: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    if dib.len() < 40 {
        return None;
    }
    let width = read_i32(dib, 4) as u32;
    let height = read_i32(dib, 8);
    let bpp = u16::from_le_bytes([dib[14], dib[15]]);
    if width == 0 || width > 20000 || height.unsigned_abs() > 20000 || (bpp != 24 && bpp != 32) {
        return None;
    }
    let top_down = height < 0;
    let h = height.unsigned_abs();
    let stride = ((width as usize * bpp as usize + 31) / 32) * 4;
    let header_size = read_i32(dib, 0) as usize;
    let compression = u32::from_le_bytes([dib[16], dib[17], dib[18], dib[19]]);
    // BI_BITFIELDS (3): channel masks follow a 40-byte header, or live inside
    // a BITMAPV4/V5 header (108/124 bytes) at offset 40
    let bitfields = compression == 3;
    let data_off = if bitfields && header_size <= 40 {
        40 + 12
    } else {
        header_size.max(40)
    };
    if data_off > dib.len() {
        return None;
    }
    // default BGRA masks; overridden by explicit bitfields
    let mut masks: [u32; 4] = [0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000]; // R,G,B,A
    if bitfields {
        // classic 40-byte header carries only 3 masks (RGB); the 4th (alpha)
        // exists only in BITMAPV4/V5 headers — reading it from pixel data
        // corrupts alpha
        let mask_count = if header_size >= 56 { 4 } else { 3 };
        for slot in 0..mask_count {
            let o = 40 + slot * 4;
            if o + 4 <= dib.len() {
                let m = u32::from_le_bytes([dib[o], dib[o + 1], dib[o + 2], dib[o + 3]]);
                if m != 0 {
                    masks[slot] = m;
                }
            }
        }
    }
    let decode = |v: u32, mask: u32| -> u8 {
        if mask == 0 {
            return 255;
        }
        let bits = mask.count_ones();
        if bits == 0 || bits >= 32 {
            return 255;
        }
        let shift = mask.trailing_zeros();
        let max = (1u64 << bits) - 1;
        let val = (((v & mask) >> shift) as u64 * 255 + max / 2) / max;
        val as u8
    };
    let px = &dib[data_off..];
    let mut buf = vec![0u8; width as usize * h as usize * 4];
    for y in 0..h as usize {
        let src_y = if top_down { y } else { h as usize - 1 - y };
        for x in 0..width as usize {
            let si = src_y * stride + x * (bpp as usize / 8);
            let di = (y * width as usize + x) * 4;
            if si + (bpp as usize / 8) > px.len() {
                continue;
            }
            let (r, g, b, a) = if bpp == 32 {
                let v = u32::from_le_bytes([px[si], px[si + 1], px[si + 2], px[si + 3]]);
                (
                    decode(v, masks[0]),
                    decode(v, masks[1]),
                    decode(v, masks[2]),
                    decode(v, masks[3]),
                )
            } else {
                (
                    px[si + 2],
                    px[si + 1],
                    px[si],
                    255u8,
                )
            };
            buf[di] = r;
            buf[di + 1] = g;
            buf[di + 2] = b;
            buf[di + 3] = if a == 0 { 255 } else { a };
        }
    }
    let img = image::RgbaImage::from_raw(width, h, buf)?;
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some((png.into_inner(), width, h))
}

/// Puts PNG bytes on the clipboard the way mainstream apps expect:
/// a standard bottom-up 32bpp CF_DIB **plus** the registered "PNG" format
/// (Chromium-based readers prefer "PNG"; GDI-era readers take CF_DIB).
pub fn write_clipboard_png(png: &[u8]) -> bool {
    let img = match image::load_from_memory(png) {
        Ok(i) => i.to_rgba8(),
        Err(_) => return false,
    };
    let (w, h) = img.dimensions();
    let mut dib = Vec::with_capacity(40 + img.len());
    dib.extend_from_slice(&40i32.to_le_bytes());
    dib.extend_from_slice(&(w as i32).to_le_bytes());
    dib.extend_from_slice(&(h as i32).to_le_bytes()); // bottom-up: positive height
    dib.extend_from_slice(&1i16.to_le_bytes());
    dib.extend_from_slice(&32i16.to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes()); // BI_RGB
    dib.extend_from_slice(&(img.len() as i32).to_le_bytes());
    dib.extend_from_slice(&[0u8; 16]);
    for y in (0..h).rev() {
        for x in 0..w {
            let px = img.get_pixel(x, y).0;
            dib.extend_from_slice(&[px[2], px[1], px[0], px[3]]); // BGRA
        }
    }
    let png_name: Vec<u16> = "PNG\0".encode_utf16().collect();
    unsafe {
        if !open_clipboard_retry() {
            return false;
        }
        let _ = EmptyClipboard();
        let mut ok = false;
        // CF_DIB
        if let Ok(hd) = GlobalAlloc(GMEM_MOVEABLE, dib.len()) {
            let ptr = GlobalLock(hd) as *mut u8;
            if ptr.is_null() {
                let _ = GlobalFree(hd);
            } else {
                std::ptr::copy_nonoverlapping(dib.as_ptr(), ptr, dib.len());
                let _ = GlobalUnlock(hd);
                if SetClipboardData(CF_DIB, HANDLE(hd.0)).is_err() {
                    let _ = GlobalFree(hd);
                } else {
                    ok = true;
                }
            }
        }
        // registered "PNG" format with the original bytes
        let png_fmt = RegisterClipboardFormatW(PCWSTR(png_name.as_ptr()));
        if let Ok(hp) = GlobalAlloc(GMEM_MOVEABLE, png.len()) {
            let ptr = GlobalLock(hp) as *mut u8;
            if ptr.is_null() {
                let _ = GlobalFree(hp);
            } else {
                std::ptr::copy_nonoverlapping(png.as_ptr(), ptr, png.len());
                let _ = GlobalUnlock(hp);
                if SetClipboardData(png_fmt, HANDLE(hp.0)).is_err() {
                    let _ = GlobalFree(hp);
                }
            }
        }
        let _ = CloseClipboard();
        if ok {
            mark_self_write();
        }
        ok
    }
}

pub fn write_clipboard_text(s: &str) -> bool {
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    if !open_clipboard_retry() {
        return false;
    }
    unsafe {
        let _ = EmptyClipboard();
        let h = match GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
            Ok(h) => h,
            Err(_) => {
                let _ = CloseClipboard();
                return false;
            }
        };
        let ptr = GlobalLock(h) as *mut u16;
        if ptr.is_null() {
            let _ = GlobalFree(h);
            let _ = CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        let _ = GlobalUnlock(h);
        let ok = SetClipboardData(CF_UNICODETEXT, HANDLE(h.0)).is_ok();
        if !ok {
            let _ = GlobalFree(h);
        }
        let _ = CloseClipboard();
        if ok {
            mark_self_write();
        }
        ok
    }
}


pub fn mark_self_write() {
    let t = now_ms();
    SELF_WRITE_UNTIL.store(t + 600, Ordering::SeqCst);
}

pub fn is_self_write() -> bool {
    now_ms() < SELF_WRITE_UNTIL.load(Ordering::SeqCst)
}

fn now_ms() -> isize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as isize)
        .unwrap_or(0)
}

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

// kept as a fallback: the panel currently uses WS_EX_NOACTIVATE set at creation
#[allow(dead_code)]
pub fn set_noactivate(hwnd: isize) {
    unsafe {
        let h = HWND(hwnd as _);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize);
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

// in-process paste is unreliable in some environments (see injector note below);
// kept for debugging / fallback use
#[allow(dead_code)]
pub fn send_ctrl_v() {
    unsafe {
        keybd_event(VK_CONTROL.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(VK_V.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        std::thread::sleep(std::time::Duration::from_millis(30));
        keybd_event(VK_V.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
    eprintln!(
        "[cv] send_ctrl_v: done, ctrl_held={:x}",
        unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) } as usize & 0xFFFF
    );
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
            let best = if !gi.hwndCaret.0.is_null() { gi.hwndCaret } else { gi.hwndFocus };
            eprintln!(
                "[cv] capture_focus: target={target:x} focus={:x} caret={:x}",
                gi.hwndFocus.0 as usize,
                gi.hwndCaret.0 as usize
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
struct MSLLHOOKSTRUCT {
    pt: POINT,
    _extra: [u8; 24],
}

unsafe extern "system" fn mouse_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code >= 0 && w_param.0 as u32 == WM_LBUTTONDOWN {
        let p = *(l_param.0 as *const MSLLHOOKSTRUCT);
        cvlog!("[cv] hook click at ({},{}) visible={} in_panel={}", p.pt.x, p.pt.y, PANEL_VISIBLE.load(Ordering::SeqCst), point_in_panel(p.pt.x, p.pt.y));
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
        let hook = SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(mouse_proc),
            None,
            0,
        );
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
        let img = image::RgbaImage::from_raw(w, h, { let mut v = Vec::new(); for _ in 0..(w*h) { v.extend_from_slice(&px); } v }).unwrap();
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


/// True when the current clipboard was flagged sensitive by the copying app
/// (Bitwarden / 1Password / KeePass / browsers set these on password copies).
/// - "ExcludeClipboardContentFromMonitorProcessing": presence alone = exclude
/// - "CanIncludeInClipboardHistory": DWORD 0 = exclude
pub fn clipboard_marked_sensitive() -> bool {
    let reg = |name: &str| -> u32 {
        let n = name.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        unsafe { RegisterClipboardFormatW(PCWSTR(n.as_ptr())) }
    };
    unsafe {
        let excl = reg("ExcludeClipboardContentFromMonitorProcessing");
        if excl != 0 && IsClipboardFormatAvailable(excl).is_ok() {
            return true;
        }
        let can = reg("CanIncludeInClipboardHistory");
        if can == 0 || IsClipboardFormatAvailable(can).is_err() {
            return false;
        }
        if !open_clipboard_retry() {
            return false;
        }
        let h = match GetClipboardData(can) {
            Ok(h) => h,
            Err(_) => {
                let _ = CloseClipboard();
                return false;
            }
        };
        let hg = HGLOBAL(h.0);
        let ptr = GlobalLock(hg) as *const u32;
        if ptr.is_null() {
            let _ = CloseClipboard();
            return false;
        }
        let v = *ptr;
        let _ = GlobalUnlock(hg);
        let _ = CloseClipboard();
        v == 0 // 0 = the app asked to be excluded from history
    }
}

// ---------- files (CF_HDROP) ----------

pub const CF_HDROP: u32 = 15;

pub fn read_clipboard_files() -> Option<Vec<String>> {
    if unsafe { IsClipboardFormatAvailable(CF_HDROP).is_err() } {
        cvlog!("[cv] files: no CF_HDROP");
        return None;
    }
    if !open_clipboard_retry() {
        cvlog!("[cv] files: open failed");
        return None;
    }
    unsafe {
        let h = GetClipboardData(CF_HDROP).ok()?;
        let hg = HGLOBAL(h.0);
        let ptr = GlobalLock(hg) as *const u8;
        if ptr.is_null() {
            let _ = CloseClipboard();
            return None;
        }
        let size = GlobalSize(hg);
        let data = std::slice::from_raw_parts(ptr, size).to_vec();
        let _ = GlobalUnlock(hg);
        let _ = CloseClipboard();
        if data.len() < 20 {
            return None;
        }
        parse_dropfiles_blob(&data)
    }
}

fn parse_dropfiles_blob(data: &[u8]) -> Option<Vec<String>> {
    if data.len() < 20 {
        return None;
    }
    let off = i32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    // DROPFILES: pFiles(0..4) pt(4..12) fNC(12..16) fWide(16..20)
    let f_wide = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) != 0;
    // defensive: some writers misplace the flag; trust the payload shape
    let wide_mode = f_wide
        || (data.len() > off + 1 && data[off + 1] == 0 && data[off] != 0 && data[off] < 0x80);
    if !wide_mode || off >= data.len() {
        return None;
    }
    let wide: Vec<u16> = data[off..]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let mut paths = Vec::new();
    let mut cur = Vec::new();
    for ch in wide {
        if ch == 0 {
            if cur.is_empty() {
                break; // double-null terminator
            }
            paths.push(String::from_utf16_lossy(&cur));
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    if paths.is_empty() {
        None
    } else {
        Some(paths)
    }
}

pub fn write_clipboard_files(paths: &[String]) -> bool {
    if paths.is_empty() {
        return false;
    }
    let mut blob: Vec<u8> = Vec::new();
    blob.extend_from_slice(&20i32.to_le_bytes()); // pFiles offset
    blob.extend_from_slice(&[0u8; 8]); // pt
    blob.extend_from_slice(&0u32.to_le_bytes()); // fNC
    blob.extend_from_slice(&1u32.to_le_bytes()); // fWide (offset 16!)
    for p in paths {
        for u in p.encode_utf16() {
            blob.extend_from_slice(&u.to_le_bytes());
        }
        blob.extend_from_slice(&0u16.to_le_bytes());
    }
    blob.extend_from_slice(&0u16.to_le_bytes()); // final double-null
    if !open_clipboard_retry() {
        return false;
    }
    unsafe {
        let _ = EmptyClipboard();
        let h = match GlobalAlloc(GMEM_MOVEABLE, blob.len()) {
            Ok(h) => h,
            Err(_) => {
                let _ = CloseClipboard();
                return false;
            }
        };
        let ptr = GlobalLock(h) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(h);
            let _ = CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(blob.as_ptr(), ptr, blob.len());
        let _ = GlobalUnlock(h);
        let ok = SetClipboardData(CF_HDROP, HANDLE(h.0)).is_ok();
        if !ok {
            let _ = GlobalFree(h);
        }
        let _ = CloseClipboard();
        if ok {
            mark_self_write();
        }
        ok
    }
}

// ---------- rich text (HTML Format) ----------

fn html_format_id() -> u32 {
    let name: Vec<u16> = "HTML Format\0".encode_utf16().collect();
    unsafe { RegisterClipboardFormatW(PCWSTR(name.as_ptr())) }
}

pub fn read_clipboard_html() -> Option<Vec<u8>> {
    let fmt = html_format_id();
    if fmt == 0 || unsafe { IsClipboardFormatAvailable(fmt).is_err() } {
        return None;
    }
    if !open_clipboard_retry() {
        return None;
    }
    unsafe {
        let h = GetClipboardData(fmt).ok()?;
        let hg = HGLOBAL(h.0);
        let ptr = GlobalLock(hg) as *const u8;
        if ptr.is_null() {
            let _ = CloseClipboard();
            return None;
        }
        let size = GlobalSize(hg);
        let data = std::slice::from_raw_parts(ptr, size).to_vec();
        let _ = GlobalUnlock(hg);
        let _ = CloseClipboard();
        Some(data)
    }
}

pub fn write_clipboard_html(html: &[u8]) -> bool {
    let fmt = html_format_id();
    if fmt == 0 || !open_clipboard_retry() {
        return false;
    }
    unsafe {
        let h = match GlobalAlloc(GMEM_MOVEABLE, html.len()) {
            Ok(h) => h,
            Err(_) => {
                let _ = CloseClipboard();
                return false;
            }
        };
        let ptr = GlobalLock(h) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(h);
            let _ = CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(html.as_ptr(), ptr, html.len());
        let _ = GlobalUnlock(h);
        let ok = SetClipboardData(fmt, HANDLE(h.0)).is_ok();
        if !ok {
            let _ = GlobalFree(h);
        }
        let _ = CloseClipboard();
        ok
    }
}


/// Reads raw bytes of the registered "PNG" format if present (Snipping Tool,
/// browsers and Office write it; byte-exact, no DIB decoding involved).
pub fn read_clipboard_png_raw() -> Option<Vec<u8>> {
    let name: Vec<u16> = "PNG".encode_utf16().collect();
    let fmt = unsafe { RegisterClipboardFormatW(PCWSTR(name.as_ptr())) };
    if fmt == 0 || unsafe { IsClipboardFormatAvailable(fmt).is_err() } {
        return None;
    }
    if !open_clipboard_retry() {
        return None;
    }
    unsafe {
        let h = GetClipboardData(fmt).ok()?;
        let hg = HGLOBAL(h.0);
        let ptr = GlobalLock(hg) as *const u8;
        if ptr.is_null() {
            let _ = CloseClipboard();
            return None;
        }
        let size = GlobalSize(hg);
        let data = std::slice::from_raw_parts(ptr, size).to_vec();
        let _ = GlobalUnlock(hg);
        let _ = CloseClipboard();
        (data.starts_with(&[0x89, 0x50, 0x4E, 0x47])).then_some(data)
    }
}

/// Logs every clipboard format currently on the clipboard (diagnostics).
pub fn log_clipboard_formats() {
    if !open_clipboard_retry() {
        return;
    }
    let mut fmt = 0u32;
    let mut names = Vec::new();
    unsafe {
        loop {
            fmt = EnumClipboardFormats(fmt);
            if fmt == 0 {
                break;
            }
            let mut buf = [0u16; 64];
            let n = GetClipboardFormatNameW(fmt, &mut buf);
            let name = if n > 0 {
                String::from_utf16_lossy(&buf[..n as usize])
            } else {
                match fmt {
                    1 => "CF_TEXT".into(),
                    2 => "CF_BITMAP".into(),
                    8 => "CF_DIB".into(),
                    13 => "CF_UNICODETEXT".into(),
                    15 => "CF_HDROP".into(),
                    17 => "CF_DIBV5".into(),
                    _ => format!("#{}", fmt),
                }
            };
            names.push(name);
        }
    }
    unsafe { let _ = CloseClipboard(); }
    cvlog!("[cv] formats: {:?}", names);
}


// ---------- resident injector ----------
// keybd_event issued from inside the app process is silently swallowed
// (cause unknown, reproducible); the identical call from a helper process
// lands reliably, so the app keeps a tiny resident injector and signals it
// through a named event.

use windows::Win32::System::Threading::{
    CreateEventW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE,
    SYNCHRONIZATION_ACCESS_RIGHTS,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Runs in a helper process (`app.exe --injector`): waits on the named event
/// and performs the paste keystroke. Exits when the main app is gone.
pub fn injector_loop() -> ! {
    unsafe {
        let name = wide("ClipVaultInject");
        let created = CreateEventW(None, false, false, PCWSTR(name.as_ptr()));
        if created.is_err() {
            std::process::exit(0);
        }
        let event = created.unwrap();
        let listener_class = wide("ClipVaultListener");
        loop {
            if WaitForSingleObject(event, 2000) == WAIT_OBJECT_0 {
                // wait until the target really has focus back (its activation
                // is asynchronous); injecting too early loses the keystroke
                let want = read_inject_target();
                for _ in 0..80 {
                    let (fg, focus) = foreground_focus();
                    if want.0 != 0 && fg == want.0 && (focus == want.1 || want.1 == 0) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                keybd_event(VK_CONTROL.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
                keybd_event(VK_V.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
                std::thread::sleep(std::time::Duration::from_millis(30));
                keybd_event(VK_V.0 as u8, 0, KEYEVENTF_KEYUP, 0);
                keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP, 0);
            }
            if FindWindowW(PCWSTR(listener_class.as_ptr()), None).is_err() {
                std::process::exit(0);
            }
        }
    }
}


/// (foreground hwnd, focused-child hwnd) of the current foreground thread.
fn foreground_focus() -> (isize, isize) {
    unsafe {
        let fg = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread, &mut gi).is_ok() {
            (fg.0 as isize, gi.hwndFocus.0 as isize)
        } else {
            (0, 0)
        }
    }
}

/// (target hwnd, target focus hwnd) written by the main app before signaling.
fn read_inject_target() -> (isize, isize) {
    let dir = std::env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\Temp".into());
    if let Ok(content) = std::fs::read_to_string(std::path::Path::new(&dir).join("cv-inject.txt")) {
        let nums: Vec<isize> = content
            .split_whitespace()
            .filter_map(|p| p.parse().ok())
            .collect();
        if nums.len() == 2 {
            return (nums[0], nums[1]);
        }
    }
    (0, 0)
}

/// The remembered focus child (0 once consumed by restore_focus).
pub fn focus_hwnd() -> isize {
    FOCUS_HWND.load(Ordering::SeqCst)
}

/// Signals the resident injector to send Ctrl+V.
pub fn request_paste_keystroke(target: isize, focus: isize) {
    // tell the injector what "ready" looks like
    let dir = std::env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\Temp".into());
    let _ = std::fs::write(
        std::path::Path::new(&dir).join("cv-inject.txt"),
        format!("{target} {focus}"),
    );
    static CACHE: StdMutex<Option<isize>> = StdMutex::new(None);
    let mut cache = CACHE.lock().unwrap();
    if let Some(h) = *cache {
        if h != 0 {
            unsafe {
                let _ = SetEvent(HANDLE(h as *mut core::ffi::c_void));
            }
            return;
        }
    }
    // (re)try opening; only cache on success — a failure (injector not yet
    // up, or gone) must stay retryable
    let name = wide("ClipVaultInject");
    if let Ok(hv) = unsafe {
        OpenEventW(
            SYNCHRONIZATION_ACCESS_RIGHTS(0x00100000) | EVENT_MODIFY_STATE,
            false,
            PCWSTR(name.as_ptr()),
        )
    } {
        *cache = Some(hv.0 as isize);
        unsafe {
            let _ = SetEvent(HANDLE(hv.0 as *mut core::ffi::c_void));
        }
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

// ---------- autostart (HKCU Run) ----------

pub fn set_autostart(enable: bool) -> bool {
    use windows::Win32::System::Registry::*;
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let name: Vec<u16> = "ClipVault".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut hk = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            0,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            &mut hk,
        )
        .is_err()
        {
            return false;
        }
        let ok = if enable {
            let exe = std::env::current_exe().unwrap_or_default();
            let cmd: Vec<u16> = format!("\"{}\" /autostart", exe.display())
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            RegSetValueExW(
                hk,
                PCWSTR(name.as_ptr()),
                0,
                REG_SZ,
                Some(&std::slice::from_raw_parts(
                    cmd.as_ptr() as *const u8,
                    cmd.len() * 2,
                ).to_vec()),
            )
            .is_ok()
        } else {
            RegDeleteValueW(hk, PCWSTR(name.as_ptr())).is_ok() || true
        };
        let _ = RegCloseKey(hk);
        ok
    }
}

#[cfg(test)]
mod file_tests {
    use super::*;


    #[test]
    fn parses_canonical_dropfiles() {
        // byte pattern captured from a real Windows Forms SetFileDropList:
        // pFiles=20, zeros, fWide=0xFFFFFFFF at offset 16
        let path: Vec<u16> = "C:\\win\\a.txt".encode_utf16().chain(std::iter::once(0)).collect();
        let mut blob = vec![0x14u8, 0, 0, 0]; // pFiles = 20 (0x14)
        blob.extend_from_slice(&[0u8; 12]); // pt + fNC
        blob.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]); // fWide @16
        for u in path {
            blob.extend_from_slice(&u.to_le_bytes());
        }
        blob.extend_from_slice(&0u16.to_le_bytes()); // list terminator
        let out = super::parse_dropfiles_blob(&blob).expect("parse failed");
        assert_eq!(out, vec!["C:\\win\\a.txt".to_string()]);
    }

    #[test]
    fn files_roundtrip_via_clipboard() {
        let paths = vec![r"C:\a\test1.txt".to_string(), r"C:\a\test2.txt".to_string()];
        assert!(write_clipboard_files(&paths), "write failed");
        let back = read_clipboard_files().expect("read back failed");
        assert_eq!(back, paths);
    }
}
