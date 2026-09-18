//! Extra clipboard formats: CF_HDROP files, "HTML Format", raw "PNG", format logging.
use super::clipboard::{open_clipboard_retry, read_clipboard_bytes, write_clipboard_single};
use super::*;

// ---------- files (CF_HDROP) ----------

pub const CF_HDROP: u32 = 15;

pub fn read_clipboard_files() -> Option<Vec<String>> {
    let data = read_clipboard_bytes(CF_HDROP)?;
    if data.len() < 20 {
        return None;
    }
    parse_dropfiles_blob(&data)
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
    write_clipboard_single(CF_HDROP, &blob)
}

// ---------- rich text (HTML Format) ----------

fn html_format_id() -> u32 {
    let name: Vec<u16> = "HTML Format\0".encode_utf16().collect();
    unsafe { RegisterClipboardFormatW(PCWSTR(name.as_ptr())) }
}

pub fn read_clipboard_html() -> Option<Vec<u8>> {
    let fmt = html_format_id();
    if fmt == 0 {
        return None;
    }
    read_clipboard_bytes(fmt)
}

pub fn write_clipboard_html(html: &[u8]) -> bool {
    let fmt = html_format_id();
    if fmt == 0 {
        return false;
    }
    // write_clipboard_single stamps the sequence number so the listener
    // attributes this second write (after the text write) to us as well
    write_clipboard_single(fmt, html)
}

/// Reads raw bytes of the registered "PNG" format if present (Snipping Tool,
/// browsers and Office write it; byte-exact, no DIB decoding involved).
pub fn read_clipboard_png_raw() -> Option<Vec<u8>> {
    let name: Vec<u16> = "PNG".encode_utf16().collect();
    let fmt = unsafe { RegisterClipboardFormatW(PCWSTR(name.as_ptr())) };
    if fmt == 0 {
        return None;
    }
    read_clipboard_bytes(fmt).filter(|d| d.starts_with(&[0x89, 0x50, 0x4E, 0x47]))
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
        let _ = CloseClipboard();
    }
    cvlog!("[cv] formats: {:?}", names);
}

#[cfg(test)]
mod file_tests {
    use super::*;

    #[test]
    fn parses_canonical_dropfiles() {
        // byte pattern captured from a real Windows Forms SetFileDropList:
        // pFiles=20, zeros, fWide=0xFFFFFFFF at offset 16
        let path: Vec<u16> = "C:\\win\\a.txt"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut blob = vec![0x14u8, 0, 0, 0]; // pFiles = 20 (0x14)
        blob.extend_from_slice(&[0u8; 12]); // pt + fNC
        blob.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]); // fWide @16
        for u in path {
            blob.extend_from_slice(&u.to_le_bytes());
        }
        blob.extend_from_slice(&0u16.to_le_bytes()); // list terminator
        let out = parse_dropfiles_blob(&blob).expect("parse failed");
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
