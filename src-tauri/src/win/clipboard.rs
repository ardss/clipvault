//! Clipboard open/read/write primitives and self-write bookkeeping.
use super::*;

// ---------- clipboard read ----------

pub(crate) fn open_clipboard_retry() -> bool {
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
        let h = match GetClipboardData(CF_UNICODETEXT) {
            Ok(h) => h,
            Err(e) => {
                cvlog!("[cv] read: GetData err {e}");
                let _ = CloseClipboard();
                return None;
            }
        };
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
        let h = match GetClipboardData(CF_DIB) {
            Ok(h) => h,
            Err(e) => {
                cvlog!("[cv] read: GetData err {e}");
                let _ = CloseClipboard();
                return None;
            }
        };
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
    // stamp the clipboard sequence number our write produced. Any clipboard
    // update — ours or external — bumps the counter and ours re-stamp it on
    // every write, so equality can only mean "the pending update is ours";
    // no time window needed (a delayed listener thread used to fall outside
    // the old 600ms gate and record our own paste as a new clip)
    SELF_WRITE_SEQ.store(
        unsafe { GetClipboardSequenceNumber() } as isize,
        Ordering::SeqCst,
    );
}

pub fn is_self_write() -> bool {
    SELF_WRITE_SEQ.load(Ordering::SeqCst) == unsafe { GetClipboardSequenceNumber() } as isize
}

pub fn clipboard_marked_sensitive() -> bool {
    let reg = |name: &str| -> u32 {
        let n = name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>();
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
