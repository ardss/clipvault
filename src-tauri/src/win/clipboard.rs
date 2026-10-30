//! Clipboard open/read/write primitives and self-write bookkeeping.
use super::*;

// ---------- shared primitives ----------

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

/// Reads raw bytes of `fmt` (availability check + open/lock/close).
pub(crate) fn read_clipboard_bytes(fmt: u32) -> Option<Vec<u8>> {
    if unsafe { IsClipboardFormatAvailable(fmt).is_err() } {
        return None;
    }
    if !open_clipboard_retry() {
        return None;
    }
    unsafe {
        let h = match GetClipboardData(fmt) {
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
        let data = std::slice::from_raw_parts(ptr, size).to_vec();
        let _ = GlobalUnlock(hg);
        let _ = CloseClipboard();
        Some(data)
    }
}

/// Allocates a GMEM_MOVEABLE block holding `bytes` — NOT yet owned by the
/// clipboard (the caller frees it on failure). Allocation is the likely
/// failure point for large images (up to ~400MB RGBA), so multi-format
/// writers allocate BEFORE EmptyClipboard: a failure then cannot leave the
/// user's previous clipboard destroyed.
pub(crate) fn alloc_global(bytes: &[u8]) -> Option<HGLOBAL> {
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).ok()?;
        let ptr = GlobalLock(h) as *mut u8;
        if ptr.is_null() {
            let _ = GlobalFree(h);
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let _ = GlobalUnlock(h);
        Some(h)
    }
}

/// Hands a pre-allocated block to the ALREADY-OPEN clipboard. Ownership
/// transfers to the system on success; the block is freed on failure.
pub(crate) fn set_clipboard_data_prealloc(fmt: u32, h: HGLOBAL) -> bool {
    unsafe {
        let ok = SetClipboardData(fmt, HANDLE(h.0)).is_ok();
        if !ok {
            let _ = GlobalFree(h);
        }
        ok
    }
}

/// Sets one format on an ALREADY-OPEN clipboard. The allocated global is
/// released on failure and handed to the system on success.
pub(crate) fn set_clipboard_data_raw(fmt: u32, bytes: &[u8]) -> bool {
    match alloc_global(bytes) {
        Some(h) => set_clipboard_data_prealloc(fmt, h),
        None => false,
    }
}

/// Opens the clipboard, replaces its content with a single format, closes and
/// stamps the self-write sequence number.
pub(crate) fn write_clipboard_single(fmt: u32, bytes: &[u8]) -> bool {
    if !open_clipboard_retry() {
        return false;
    }
    unsafe {
        let _ = EmptyClipboard();
    }
    let ok = set_clipboard_data_raw(fmt, bytes);
    // stamp BEFORE closing: SetClipboardData already bumped the sequence
    // number, and stamping inside the open session closes the microsecond
    // gap where the capture worker could read the old number and treat our
    // write as external
    if ok {
        mark_self_write();
    }
    unsafe {
        let _ = CloseClipboard();
    }
    ok
}

// ---------- readers ----------

pub fn read_clipboard_text() -> Option<String> {
    let bytes = read_clipboard_bytes(CF_UNICODETEXT)?;
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    let s = String::from_utf16_lossy(&units);
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn read_clipboard_dib_vec() -> Option<Vec<u8>> {
    read_clipboard_bytes(CF_DIB)
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
    }
    // value 0 = the app asked to be excluded from history tools
    match read_clipboard_bytes(reg("CanIncludeInClipboardHistory")) {
        Some(b) if b.len() >= 4 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) == 0,
        _ => false,
    }
}

// ---------- writers ----------

/// Puts PNG bytes on the clipboard the way mainstream apps expect:
/// a standard bottom-up 32bpp CF_DIB **plus** the registered "PNG" format
/// (Chromium-based readers prefer "PNG"; GDI-era readers take CF_DIB).
pub fn write_clipboard_png(png: &[u8]) -> bool {
    // same bomb limits as the capture path — stored bytes can still expand
    // to gigabytes of RGBA without a dimension cap
    let mut reader = image::ImageReader::new(std::io::Cursor::new(png));
    reader.set_format(image::ImageFormat::Png);
    let mut lim = image::Limits::default();
    lim.max_image_width = Some(10000);
    lim.max_image_height = Some(10000);
    reader.limits(lim);
    let img = match reader.decode() {
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
    let png_fmt = unsafe { RegisterClipboardFormatW(PCWSTR(png_name.as_ptr())) };
    // allocate BOTH payloads BEFORE opening/emptying the clipboard: from the
    // first failure point on, the user's previous clipboard is untouched
    let Some(h_dib) = alloc_global(&dib) else {
        return false;
    };
    let h_png = if png_fmt != 0 {
        alloc_global(png)
    } else {
        None
    };
    if !open_clipboard_retry() {
        unsafe {
            let _ = GlobalFree(h_dib);
            if let Some(h) = h_png {
                let _ = GlobalFree(h);
            }
        }
        return false;
    }
    unsafe {
        let _ = EmptyClipboard();
    }
    // two formats in one open session: set without close/stamp so both land
    // before we hand the clipboard back
    let ok_dib = set_clipboard_data_prealloc(CF_DIB, h_dib);
    let ok_png = h_png.is_some_and(|h| set_clipboard_data_prealloc(png_fmt, h));
    if ok_dib || ok_png {
        // anything that landed is OURS — stamp even on a partial failure so
        // the capture worker never echoes our half-written clipboard back as
        // a fresh external clip
        mark_self_write();
    }
    unsafe {
        let _ = CloseClipboard();
    }
    if !ok_dib && ok_png {
        cvlog!("[cv] clipboard: partial write — PNG format landed without CF_DIB");
    }
    ok_dib
}

pub fn write_clipboard_text(s: &str) -> bool {
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
    write_clipboard_single(CF_UNICODETEXT, bytes)
}

// ---------- self-write bookkeeping ----------

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

#[cfg(test)]
mod clipboard_atomicity_tests {
    /// A failed multi-format write must not destroy the previous clipboard:
    /// garbage PNG bytes fail DECODE, which happens before the clipboard is
    /// even opened — the sentinel must survive. (The DIB/PNG allocations are
    /// likewise made before EmptyClipboard, so an allocation failure under
    /// memory pressure can no longer leave a cleared clipboard behind.)
    #[test]
    fn failed_png_write_never_destroys_the_previous_clipboard() {
        let _clipboard = super::clipboard_test_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        assert!(
            super::write_clipboard_text("SENTINEL-KEEP"),
            "seed clipboard"
        );
        assert!(!super::write_clipboard_png(b"not a png at all"));
        assert_eq!(
            super::read_clipboard_text().as_deref(),
            Some("SENTINEL-KEEP"),
            "previous clipboard content must survive a failed write"
        );
    }

    /// The pre-allocation primitive: alloc → contents intact → free.
    #[test]
    fn alloc_global_roundtrip_preserves_bytes() {
        let h = super::alloc_global(b"HELLO").expect("alloc must succeed");
        unsafe {
            let ptr = super::GlobalLock(h) as *const u8;
            assert!(!ptr.is_null());
            assert_eq!(std::slice::from_raw_parts(ptr, 5), b"HELLO");
            let _ = super::GlobalUnlock(h);
            let _ = super::GlobalFree(h);
        }
    }
}
