//! Tauri commands plus the panel show/hide choreography they share with the
//! hotkey handler and the tray menu.
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::db;
use crate::win;

pub(crate) fn show_panel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        cvlog!("[cv] panel url: {:?}", w.url());
        let fg = win::get_foreground();
        win::PASTE_TARGET.store(fg, Ordering::SeqCst);
        win::capture_focus(fg);
        let (cx, cy) = win::get_cursor_pos();
        // use the panel's REAL current size (user-resizable), not a constant
        let (pw, ph) = w
            .outer_size()
            .map(|s| (s.width as i32, s.height as i32))
            .unwrap_or((380, 540));
        let mut x = cx + 12;
        let mut y = cy + 12;
        // flip near screen edges
        if let Ok(monitors) = app.available_monitors() {
            for m in monitors {
                let pos = m.position();
                let size = m.size();
                let (mx, my) = (pos.x, pos.y);
                let (mw, mh) = (size.width as i32, size.height as i32);
                if cx >= mx && cx < mx + mw && cy >= my && cy < my + mh {
                    if x + pw > mx + mw {
                        x = cx - pw - 12;
                    }
                    if y + ph > my + mh {
                        y = cy - ph - 12;
                    }
                    // final clamp: never let the panel leave this monitor
                    // (width 380 fits everywhere; height clamps to the top)
                    x = x.clamp(mx, (mx + mw - pw).max(mx));
                    y = y.clamp(my, (my + mh - ph).max(my));
                    break;
                }
            }
        }
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
        let _ = w.show();
        let _ = w.set_focus();
        win::update_panel_rect(w.hwnd().map(|h| h.0 as isize).unwrap_or(0));
        win::PANEL_VISIBLE.store(true, Ordering::SeqCst);
        let _ = w.emit("panel-shown", ());
    }
}

pub(crate) fn hide_panel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
    }
    if let Some(z) = app.get_webview_window("zoom") {
        let _ = z.hide();
    }
    win::PANEL_VISIBLE.store(false, Ordering::SeqCst);
    win::PASTE_TARGET.store(0, Ordering::SeqCst);
}

/// Lazily creates the zoom preview window with a fixed, code-defined config —
/// the frontend never needs the broader create-webview-window permission.
#[tauri::command]
pub(crate) fn create_zoom(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::WebviewUrl;
    if app.get_webview_window("zoom").is_some() {
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "zoom", WebviewUrl::App("index.html".into()))
        .title("ClipVault")
        .inner_size(340.0, 460.0)
        .visible(false)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focused(false)
        .shadow(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The zoom window is created lazily by the frontend; it registers its HWND
/// here on mount so the native watchdog can find it.
#[tauri::command]
pub(crate) fn register_zoom(app: tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    if let Some(z) = app.get_webview_window("zoom") {
        if let Ok(h) = z.hwnd() {
            crate::win::ZOOM_HWND.store(h.0 as isize, Ordering::SeqCst);
        }
    }
}

#[tauri::command]
pub(crate) fn clip_content(state: tauri::State<crate::db::Db>, id: i64) -> Result<String, String> {
    db::get_content(&state.0.lock().unwrap_or_else(|p| p.into_inner()), id)
}

#[tauri::command]
pub(crate) fn report_error(app: AppHandle, msg: String) {
    eprintln!("[cv-js-error] {msg}");
    let line = format!("{:?} {msg}\n", std::time::SystemTime::now());
    // cap the log so a failing webview can't grow it unbounded
    if let Ok(meta) = std::fs::metadata(
        app.path()
            .app_data_dir()
            .unwrap_or_default()
            .join("js-errors.log"),
    ) {
        if meta.len() > 1_000_000 {
            return;
        }
    }
    if let Ok(mut path) = app.path().app_data_dir() {
        path.push("js-errors.log");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    }
}

#[tauri::command]
pub(crate) fn heartbeat() {
    cvlog!("[cv] heartbeat received");
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    crate::HEARTBEAT.store(t, Ordering::SeqCst);
}

#[tauri::command]
pub(crate) fn list_clips(
    state: tauri::State<db::Db>,
    filter: String,
    query: String,
) -> Result<Vec<db::Clip>, String> {
    let conn = state.0.lock().unwrap_or_else(|p| p.into_inner());
    db::list(&conn, &filter, &query)
}

#[tauri::command]
pub(crate) fn stats(state: tauri::State<db::Db>) -> Result<db::Stats, String> {
    db::stats(&state.0.lock().unwrap_or_else(|p| p.into_inner()))
}

#[tauri::command]
pub(crate) fn toggle_pin(state: tauri::State<db::Db>, id: i64) -> Result<(), String> {
    db::toggle_pin(&state.0.lock().unwrap_or_else(|p| p.into_inner()), id)
}

#[tauri::command]
pub(crate) fn delete_clip(state: tauri::State<db::Db>, id: i64) -> Result<(), String> {
    db::delete(&state.0.lock().unwrap_or_else(|p| p.into_inner()), id)
}

/// Wipes every clip: rows plus their on-disk images, thumbnails and
/// oversized-text side files. Returns the number of entries removed.
#[tauri::command]
pub(crate) fn clear_history(app: AppHandle, state: tauri::State<db::Db>) -> Result<usize, String> {
    let victims = {
        let conn = state.0.lock().unwrap_or_else(|p| p.into_inner());
        db::clear_all(&conn)?
    };
    let n = victims.len();
    for (img, txt) in victims {
        db::remove_clip_files(img.as_deref(), txt.as_deref());
    }
    let _ = app.emit("clips-changed", ());
    Ok(n)
}

#[tauri::command]
pub(crate) fn paste_clip(
    app: AppHandle,
    state: tauri::State<db::Db>,
    id: i64,
) -> Result<(), String> {
    let (kind, content, image_path) = {
        let conn = state.0.lock().unwrap_or_else(|p| p.into_inner());
        let r = db::get_clip(&conn, id);
        if r.is_ok() {
            db::mark_used(&conn, id);
        }
        r
    }?;
    let ok = match kind.as_str() {
        "image" => {
            let png = std::fs::read(image_path.unwrap_or_default()).unwrap_or_default();
            !png.is_empty() && win::write_clipboard_png(&png)
        }
        "file" => {
            let paths = db::get_files(&state.0.lock().unwrap_or_else(|p| p.into_inner()), id)?;
            win::write_clipboard_files(&paths)
        }
        "html" => {
            let (plain, html) =
                db::get_html(&state.0.lock().unwrap_or_else(|p| p.into_inner()), id)?;
            win::write_clipboard_text(&plain)
                && (html.is_empty() || win::write_clipboard_html(&html))
        }
        "text" if image_path.is_some() => {
            // oversized text stored in a side file; an empty read means the
            // row's file was deleted between listing and paste (e.g. by
            // clear_history) — fail loudly instead of pasting nothing
            let full = std::fs::read_to_string(image_path.unwrap_or_default()).unwrap_or_default();
            if full.is_empty() {
                return Err("clip content file is gone".into());
            }
            win::write_clipboard_text(&full)
        }
        _ => win::write_clipboard_text(&content.unwrap_or_default()),
    };
    cvlog!("[cv] paste: kind={kind} write_ok={ok}");
    if !ok {
        return Err("clipboard write failed".into());
    }
    // read the target BEFORE hide_panel — hide clears it
    let target = win::PASTE_TARGET.load(Ordering::SeqCst);
    hide_panel(&app);
    let mut restored = win::restore_foreground(target);
    if restored {
        win::restore_focus(target);
    }
    let focus = win::focus_hwnd();
    for _ in 0..4 {
        if restored {
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
        restored = win::restore_foreground(target);
    }
    cvlog!("[cv] paste: restored={restored}");
    if !restored {
        return Err("FOCUS_LOST".into());
    }
    // the target app needs a beat to finish re-activating (WinForms/WPF
    // restore their control focus asynchronously); 30ms was too early
    std::thread::sleep(Duration::from_millis(20));
    win::log_send_time();
    cvlog!("[cv] paste: sending ctrl+v");
    if !win::request_paste_keystroke(target, focus) {
        return Err("paste keystroke could not be delivered (injector unavailable)".into());
    }
    cvlog!("[cv] paste: sent");
    Ok(())
}

#[tauri::command]
pub(crate) fn show_panel_cmd(app: AppHandle) {
    show_panel(&app);
}

#[tauri::command]
pub(crate) fn hide_panel_cmd(app: AppHandle) {
    hide_panel(&app);
}
