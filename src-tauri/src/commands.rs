//! Tauri commands plus the panel show/hide choreography they share with the
//! hotkey handler and the tray menu.
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::db;
use crate::win;

// A second launch while the panel already owns focus must not replace the
// previously captured paste destination with ClipVault's own window.
fn should_capture_paste_target(foreground: isize, panel: isize, zoom: isize) -> bool {
    foreground != 0 && foreground != panel && foreground != zoom
}

fn validated_paste_target(target: isize) -> Result<isize, String> {
    // The previously captured editor may have closed while the panel stayed open.
    // Reject a stale nonzero HWND before any system clipboard write.
    let alive = target != 0
        && unsafe {
            windows::Win32::UI::WindowsAndMessaging::IsWindow(windows::Win32::Foundation::HWND(
                target as _,
            ))
        }
        .as_bool();
    if !alive {
        Err("FOCUS_LOST".into())
    } else {
        Ok(target)
    }
}

#[cfg(test)]
mod paste_target_tests {
    use super::{should_capture_paste_target, validated_paste_target};

    #[test]
    fn missing_target_is_rejected_before_clipboard_mutation() {
        assert_eq!(validated_paste_target(0), Err("FOCUS_LOST".to_string()));
        // A nonzero HWND may already belong to a closed application.
        // Never overwrite the clipboard when the destination no longer exists.
        assert_eq!(validated_paste_target(1), Err("FOCUS_LOST".to_string()));
        let desktop = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow() };
        let live = desktop.0 as isize;
        assert_eq!(validated_paste_target(live), Ok(live));
    }

    #[test]
    fn second_launch_keeps_original_target_when_clipvault_is_foreground() {
        assert!(!should_capture_paste_target(10, 10, 20)); // panel focused
        assert!(!should_capture_paste_target(20, 10, 20)); // zoom focused
        assert!(!should_capture_paste_target(0, 10, 20)); // no foreground
        assert!(should_capture_paste_target(30, 10, 20)); // external editor
    }
}

pub(crate) fn show_panel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        cvlog!("[cv] panel url: {:?}", w.url());
        let fg = win::get_foreground();
        let panel_hwnd = w.hwnd().map(|h| h.0 as isize).unwrap_or(0);
        let zoom_hwnd = win::ZOOM_HWND.load(Ordering::SeqCst);
        if should_capture_paste_target(fg, panel_hwnd, zoom_hwnd) {
            win::PASTE_TARGET.store(fg, Ordering::SeqCst);
            win::capture_focus(fg);
        } else {
            cvlog!("[cv] panel re-show: preserve previous paste target");
        }
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
        // verify against the OS — Tauri's position bookkeeping can diverge
        // from what the OS applied after DPI rescaling (observed: physical
        // (637,0) requested, window landing at (510,0) on a 1.25x monitor,
        // while outer_position still reported the requested coords). Correct
        // once through the logical coordinate path if the OS disagrees.
        if let Some(hwnd) = w.hwnd().ok().map(|h| h.0 as isize) {
            if let Some((l, t, _, _)) = win::get_window_rect(hwnd) {
                if (l - x).abs() > 4 || (t - y).abs() > 4 {
                    let sf = w.scale_factor().unwrap_or(1.25);
                    let _ =
                        w.set_position(tauri::LogicalPosition::new(x as f64 / sf, y as f64 / sf));
                    if let Some((l2, t2, _, _)) = win::get_window_rect(hwnd) {
                        cvlog!("[cv] panel position corrected: os=({l},{t}) -> ({l2},{t2}) target=({x},{y})");
                    }
                }
            }
        }
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
pub(crate) async fn create_zoom(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::WebviewUrl;
    // async => runs on the IPC runtime thread. This MUST NOT run on the main
    // thread: build() blocks until the webview initializer event fires, and
    // on the main thread that event can never be pumped — a deadlock that
    // froze every later paste_clip invocation (the P0 "paste is dead" bug).
    cvlog!("[cv] create_zoom invoked");
    if app.get_webview_window("zoom").is_some() {
        cvlog!("[cv] create_zoom: window already exists");
        return Ok(());
    }
    // the query param pins preview mode even if the label check ever fails
    tauri::WebviewWindowBuilder::new(&app, "zoom", WebviewUrl::App("index.html?zoom=1".into()))
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
        .inspect(|_w| {
            cvlog!("[cv] create_zoom: window built");
        })
        .inspect_err(|e| {
            cvlog!("[cv] create_zoom FAILED: {e}");
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The zoom window is created lazily by the frontend; it registers its HWND
/// here on mount so the native watchdog can find it.
#[tauri::command]
pub(crate) fn register_zoom(app: tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    cvlog!(
        "[cv] register_zoom from label={}",
        app.get_webview_window("zoom")
            .map(|w| w.label().to_string())
            .unwrap_or_default()
    );
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

fn js_error_category(message: &str) -> &'static str {
    match message {
        "js-error" => "js-error",
        "unhandled-rejection" => "unhandled-rejection",
        _ => "unknown-js-error",
    }
}

#[cfg(test)]
mod js_error_privacy_tests {
    use super::js_error_category;

    #[test]
    fn never_persist_raw_error_text() {
        assert_eq!(js_error_category("js-error"), "js-error");
        assert_eq!(
            js_error_category("unhandled-rejection"),
            "unhandled-rejection"
        );
        assert_eq!(
            js_error_category("password=secret copied text"),
            "unknown-js-error"
        );
        assert_eq!(
            js_error_category("unhandled: private search"),
            "unknown-js-error"
        );
    }
}

#[tauri::command]
pub(crate) fn report_error(app: AppHandle, msg: String) {
    // Clipboard content may appear in arbitrary JS exception text. Only log known event classes.
    let category = js_error_category(&msg);
    eprintln!("[cv-js-error] {category}");
    let line = format!("{:?} {category}\n", std::time::SystemTime::now());
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
        db::get_clip(&conn, id)
    }?;
    // No external paste destination: fail before changing the system clipboard.
    // A tray/second-instance re-show may have preserved a zero target.
    let target = validated_paste_target(win::PASTE_TARGET.load(Ordering::SeqCst))?;
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
    // The target was validated before writing; hide_panel clears the global.
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
    db::mark_used(&state.0.lock().unwrap_or_else(|p| p.into_inner()), id);
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
