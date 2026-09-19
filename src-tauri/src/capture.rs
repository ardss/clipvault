//! Clipboard capture pipeline: read the clipboard on change, classify,
//! persist (with dedup, keyword filter, oversized-text side files) and prune.
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::db;
use crate::settings::SettingsState;
use crate::win;

pub(crate) fn save_image(app: &AppHandle, png: &[u8]) -> Option<(String, u32, u32)> {
    let (bytes, w, h) = win::dib_to_png(png)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&bytes, &mut hasher);
    let hash = std::hash::Hasher::finish(&hasher);
    let dir = app.path().app_data_dir().ok()?;
    let path = dir.join("images").join(format!("{hash:016x}.png"));
    if !path.exists() {
        std::fs::write(&path, &bytes).ok()?;
    }
    // 256px thumbnail: the panel lists load this instead of the full image.
    // Decode limits guard against decompression bombs — clipboard bytes can
    // come from any app, and an unconstrained decode can allocate gigabytes
    let thumb = dir.join("images").join(format!("{hash:016x}_t.png"));
    if !thumb.exists() {
        let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes));
        reader.set_format(image::ImageFormat::Png);
        let mut lim = image::Limits::default();
        lim.max_image_width = Some(10000);
        lim.max_image_height = Some(10000);
        reader.limits(lim);
        if let Ok(img) = reader.decode() {
            let t = img.thumbnail(256, 256);
            let _ = t.save(&thumb);
        }
    }
    Some((path.to_string_lossy().into_owned(), w, h))
}

pub(crate) fn handle_clipboard_change(app: &AppHandle) {
    if win::is_self_write() {
        return;
    }
    // capture paused from settings — copies go to the real clipboard
    // untouched, nothing is recorded
    if app
        .state::<SettingsState>()
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .paused
    {
        return;
    }
    // password managers flag sensitive copies — never record those. Writers
    // put formats on the clipboard in steps, so re-check a few times before
    // trusting a "not flagged" result
    if win::clipboard_marked_sensitive() {
        eprintln!("[cv] sensitive clipboard content skipped");
        return;
    }
    // writers put formats on the clipboard in steps — confirm once after a
    // short settle instead of polling three times (was 360ms on every copy)
    std::thread::sleep(Duration::from_millis(120));
    if win::clipboard_marked_sensitive() {
        eprintln!("[cv] sensitive clipboard content skipped");
        return;
    }
    // snapshot settings once so limit and keywords come from the same state
    let settings_state = app.state::<SettingsState>();
    let (limit, keywords) = {
        let s = settings_state.0.lock().unwrap_or_else(|p| p.into_inner());
        (s.history_limit, s.sensitive_keywords.clone())
    };
    let st = app.state::<db::Db>();
    let lock = || st.0.lock().unwrap_or_else(|p| p.into_inner());
    let mut conn = lock();
    let mut captured = false;
    // priority: files > rich text/plain > image
    if let Some(files) = win::read_clipboard_files() {
        cvlog!("[cv] files read: {}", files.len());
        if db::upsert_file(&conn, &files).is_ok() {
            captured = true;
        }
    } else {
        cvlog!("[cv] files: none, trying text/html/dib");
        let mut text = win::read_clipboard_text();
        let html_raw = win::read_clipboard_html();
        // the HTML Format blob is stored inline in the DB — cap it like text;
        // an oversized rich copy degrades to its plain text (which still gets
        // the oversized side-file path and the keyword filter)
        let html = if html_raw.as_ref().is_some_and(|h| h.len() <= 256 * 1024) {
            html_raw.clone()
        } else {
            None
        };
        if text.is_none() {
            if let Some(h) = &html_raw {
                if html.is_none() {
                    text = Some(html_to_plain(h));
                }
            }
        }
        if let (Some(t), Some(h)) = (&text, &html) {
            if db::upsert_html(&conn, t, h).is_ok() {
                captured = true;
            }
        } else if let Some(h) = html {
            // some writers set "HTML Format" without CF_UNICODETEXT
            let plain = html_to_plain(&h);
            if db::upsert_html(&conn, &plain, &h).is_ok() {
                captured = true;
            }
        } else if let Some(t) = text {
            let clean = sanitize_text(&t);
            let lower = clean.to_lowercase();
            // keyword filter applies to both the inline and the oversized path —
            // a >256KB copy containing a sensitive word must not be written to disk
            if keywords
                .iter()
                .any(|k| !k.trim().is_empty() && lower.contains(&k.trim().to_lowercase()))
            {
                eprintln!("[cv] text matched sensitive keyword — skipped");
            } else if t.len() <= 256 * 1024 {
                if db::upsert_text(&conn, &clean).is_ok() {
                    captured = true;
                }
            } else {
                // oversized: full text goes to a side file so nothing is lost —
                // the DB row keeps only a short preview
                let Ok(dir) = app.path().app_data_dir().map(|d| d.join("texts")) else {
                    return;
                };
                let _ = std::fs::create_dir_all(&dir);
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                std::hash::Hash::hash(&clean, &mut hasher);
                let path = dir.join(format!("{:016x}.txt", std::hash::Hasher::finish(&hasher)));
                if std::fs::write(&path, &clean).is_err() {
                    return;
                }
                let short: String = clean.chars().take(2000).collect();
                match db::upsert_text_file(&conn, &short, path.to_string_lossy().as_ref()) {
                    Ok(_) => {
                        captured = true;
                        cvlog!("[cv] big text stored: {}", path.display());
                    }
                    Err(e) => cvlog!("[cv] big text upsert ERR: {e}"),
                }
            }
        } else if let Some(png) = win::read_clipboard_png_raw() {
            // Snipping Tool / browsers / Office write exact "PNG" bytes —
            // no DIB decoding involved
            cvlog!("[cv] png format read: {} bytes", png.len());
            // decode OUTSIDE the db lock — a large screenshot can take
            // seconds to decode and must not stall list/paste queries
            let saved = if png.len() <= 20 * 1024 * 1024 {
                drop(conn);
                save_image(app, &png)
            } else {
                None
            };
            conn = lock();
            if let Some((path, w, h)) = saved {
                if db::upsert_image(&conn, &path, w, h).is_ok() {
                    captured = true;
                }
            }
        } else if let Some(dib) = win::read_clipboard_dib_vec() {
            win::log_clipboard_formats();
            let saved = if dib.len() <= 20 * 1024 * 1024 {
                drop(conn);
                save_image(app, &dib)
            } else {
                None
            };
            conn = lock();
            if let Some((path, w, h)) = saved {
                if db::upsert_image(&conn, &path, w, h).is_ok() {
                    captured = true;
                }
            }
        }
    }
    if captured {
        if let Ok(victims) = db::enforce_limit(&conn, limit) {
            // remove image files of evicted rows (thumbs + oversized-text side files too)
            for (img, txt) in victims {
                db::remove_clip_files(img.as_deref(), txt.as_deref());
            }
        }
        drop(conn);
        let _ = app.emit("clips-changed", ());
    }
}

/// Removes control characters that break rendering/search (keeps newline, CR, tab).
fn sanitize_text(s: &str) -> String {
    s.chars()
        .filter(|&c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
        .collect()
}

fn html_to_plain(html: &[u8]) -> String {
    let s = String::from_utf8_lossy(html);
    let body = match (s.find("<!--StartFragment-->"), s.find("<!--EndFragment-->")) {
        (Some(a), Some(b)) if a < b => &s[a + 20..b],
        _ => &s[..],
    };
    let mut out = String::new();
    let mut in_tag = false;
    for ch in body.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod logic_tests {
    use super::*;

    // ---- sanitize_text ----

    #[test]
    fn sanitize_keeps_newline_cr_tab_strips_other_controls() {
        let input = "line1\nline2\r\ntab\there\u{0}\u{1}\u{7}\u{8}end";
        let out = sanitize_text(input);
        assert_eq!(out, "line1\nline2\r\ntab\thereend");
    }

    #[test]
    fn sanitize_keeps_printable_unicode_and_emoji() {
        let input = "héllo wörld 日本語 \u{1F600}";
        assert_eq!(sanitize_text(input), input);
    }

    #[test]
    fn sanitize_empty_and_control_only() {
        assert_eq!(sanitize_text(""), "");
        assert_eq!(sanitize_text("\u{0}\u{1}\u{2}"), "");
        assert_eq!(sanitize_text("\u{0}x\u{1}"), "x");
    }

    // ---- html_to_plain ----

    #[test]
    fn html_strips_tags_keeps_text() {
        let html = b"<html><body><p>Hello <b>world</b></p></body></html>";
        assert_eq!(html_to_plain(html), "Hello world");
    }

    #[test]
    fn html_uses_start_end_fragment_markers() {
        let html = b"version:0.9\r\nstarthtml:0000100\r\nendhtml:0000230\r\n<!--StartFragment--><i>frag</i><!--EndFragment-->trailing";
        assert_eq!(html_to_plain(html), "frag");
    }

    #[test]
    fn html_decodes_entities() {
        let html = b"<p>a &amp; b &lt;tag&gt; &quot;q&quot;&nbsp;sp</p>";
        assert_eq!(html_to_plain(html), "a & b <tag> \"q\" sp");
    }

    #[test]
    fn html_without_fragment_or_malformed() {
        assert_eq!(html_to_plain(b"<p>plain</p>"), "plain");
        // EndFragment before StartFragment -> whole string is body
        let s = String::from_utf8_lossy(b"<!--EndFragment-->x<!--StartFragment-->y");
        let body = match (s.find("<!--StartFragment-->"), s.find("<!--EndFragment-->")) {
            (Some(a), Some(b)) if a < b => &s[a + 20..b],
            _ => &s[..],
        };
        assert!(body.contains('y'));
    }

    #[test]
    fn html_nested_and_unclosed_tags() {
        assert_eq!(html_to_plain(b"<div><div>deep</div></div>"), "deep");
        assert_eq!(html_to_plain(b"<p>unclosed"), "unclosed");
        assert_eq!(html_to_plain(b"<"), ""); // lone '<' starts a tag that never ends
                                             // literal '>' outside a tag is kept (was dropped before the fix)
        assert_eq!(html_to_plain(b">"), ">");
        assert_eq!(html_to_plain(b"a > b"), "a > b");
    }

    #[test]
    fn html_trims_whitespace() {
        assert_eq!(html_to_plain(b"  <p>  spaced  </p>  "), "spaced");
    }

    #[test]
    fn html_invalid_utf8_is_lossy() {
        assert_eq!(html_to_plain(&[0x68, 0x69, 0xFF]), "hi\u{FFFD}");
    }
}
