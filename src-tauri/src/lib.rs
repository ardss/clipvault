/// Debug logging gated by CV_LOG=1 — release builds stay silent.
macro_rules! cvlog {
    ($($arg:tt)*) => {
        if std::env::var("CV_LOG").is_ok() {
            eprintln!($($arg)*);
        }
    };
}

mod db;
mod win;




use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    pub history_limit: i64,
    pub panel_height: f64,
    pub autostart: bool,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default)]
    pub sensitive_keywords: Vec<String>,
}

fn default_hotkey() -> String { "Alt+V".into() }

impl Default for Settings {
    fn default() -> Self {
        Self { history_limit: 1000, panel_height: 540.0, autostart: false, hotkey: default_hotkey(), sensitive_keywords: Vec::new() }
    }
}

pub struct SettingsState(pub Mutex<Settings>);

/// Last frontend heartbeat (ms epoch); a webview watchdog reloads the UI
/// when the panel is visible but the JS loop has gone silent.
pub static HEARTBEAT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

fn load_settings(app: &AppHandle) -> Settings {
    let dir = app.path().app_data_dir().unwrap_or_default();
    std::fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(app: &AppHandle, s: &Settings) {
    let dir = app.path().app_data_dir().unwrap_or_default();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(dir.join("settings.json"), json);
    }
}

fn apply_settings(app: &AppHandle, s: &Settings) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.set_size(tauri::LogicalSize::new(380.0, s.panel_height));
    }
    win::set_autostart(s.autostart);
}

fn save_image(app: &AppHandle, png: &[u8]) -> Option<(String, u32, u32)> {
    let (bytes, w, h) = win::dib_to_png(png)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&bytes, &mut hasher);
    let hash = std::hash::Hasher::finish(&hasher);
    let dir = app.path().app_data_dir().ok()?;
    let path = dir.join("images").join(format!("{hash:016x}.png"));
    if !path.exists() {
        std::fs::write(&path, &bytes).ok()?;
    }
    // 256px thumbnail: the panel lists load this instead of the full image
    let thumb = dir.join("images").join(format!("{hash:016x}_t.png"));
    if !thumb.exists() {
        if let Ok(img) = image::load_from_memory(&bytes) {
            let t = img.thumbnail(256, 256);
            let _ = t.save(&thumb);
        }
    }
    Some((path.to_string_lossy().into_owned(), w, h))
}

fn handle_clipboard_change(app: &AppHandle) {
    if win::is_self_write() {
        return;
    }
    // password managers flag sensitive copies — never record those. Writers
    // put formats on the clipboard in steps, so re-check a few times before
    // trusting a "not flagged" result
    for _ in 0..3 {
        if win::clipboard_marked_sensitive() {
            eprintln!("[cv] sensitive clipboard content skipped");
            return;
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    let st = app.state::<db::Db>();
    let conn = st.0.lock().unwrap_or_else(|p| p.into_inner());
    let limit = app.state::<SettingsState>().0.lock().unwrap_or_else(|p| p.into_inner()).history_limit;
    let keywords: Vec<String> = app.state::<SettingsState>().0.lock().unwrap_or_else(|p| p.into_inner()).sensitive_keywords.clone();
    let mut captured = false;
    // priority: files > rich text/plain > image
    if let Some(files) = win::read_clipboard_files() {
        cvlog!("[cv] files read: {}", files.len());
        if db::upsert_file(&conn, &files).is_ok() {
            captured = true;
        }
    } else {
        cvlog!("[cv] files: none, trying text/html/dib");
        let text = win::read_clipboard_text();
        let html = win::read_clipboard_html();
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
            // huge texts would stall IPC, bloat WAL and make the panel lag —
            // skip anything over 256KB (real clipboard use is nowhere near)
            if t.len() <= 256 * 1024 {
                let clean = sanitize_text(&t);
                let lower = clean.to_lowercase();
                if keywords.iter().any(|k| !k.trim().is_empty() && lower.contains(&k.trim().to_lowercase())) {
                    eprintln!("[cv] text matched sensitive keyword — skipped");
                } else if db::upsert_text(&conn, &clean).is_ok() {
                    captured = true;
                }
            } else {
                // oversized: full text goes to a side file so nothing is lost —
                // the DB row keeps only a short preview
                let Ok(dir) = app.path().app_data_dir().map(|d| d.join("texts")) else {
                    return;
                };
                let _ = std::fs::create_dir_all(&dir);
                let clean = sanitize_text(&t);
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                std::hash::Hash::hash(&clean, &mut hasher);
                let path = dir.join(format!("{:016x}.txt", std::hash::Hasher::finish(&hasher)));
                if std::fs::write(&path, &clean).is_err() { return; }
                let short: String = clean.chars().take(2000).collect();
                match db::upsert_text_file(&conn, &short, &path.to_string_lossy().as_ref()) {
                    Ok(_) => { captured = true; cvlog!("[cv] big text stored: {}", path.display()); }
                    Err(e) => cvlog!("[cv] big text upsert ERR: {e}"),
                }
            }
        } else if let Some(png) = win::read_clipboard_png_raw() {
            // Snipping Tool / browsers / Office write exact "PNG" bytes —
            // no DIB decoding involved
            cvlog!("[cv] png format read: {} bytes", png.len());
            if png.len() <= 20 * 1024 * 1024 {
                if let Some((path, w, h)) = save_image(app, &png) {
                    if db::upsert_image(&conn, &path, w, h).is_ok() {
                        captured = true;
                    }
                }
            }
        } else if let Some(dib) = win::read_clipboard_dib_vec() {
            win::log_clipboard_formats();
            if dib.len() <= 20 * 1024 * 1024 {
                if let Some((path, w, h)) = save_image(app, &dib) {
                    if db::upsert_image(&conn, &path, w, h).is_ok() {
                        captured = true;
                    }
                }
            }
        }
    }
    if captured {
        if let Ok(victims) = db::enforce_limit(&conn, limit) {
            // remove image files of evicted rows (thumbs too)
            for p in victims {
                let t = p.replace(".png", "_t.png");
                let _ = std::fs::remove_file(&p);
                let _ = std::fs::remove_file(&t);
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
    let body = match (
        s.find("<!--StartFragment-->"),
        s.find("<!--EndFragment-->"),
    ) {
        (Some(a), Some(b)) if a < b => &s[a + 20..b],
        _ => &s[..],
    };
    let mut out = String::new();
    let mut in_tag = false;
    for ch in body.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

fn show_panel(app: &AppHandle) {
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
        let _ = w.set_position(PhysicalPosition::new(x, y));
        let _ = w.show();
        let _ = w.set_focus();
        win::update_panel_rect(w.hwnd().map(|h| h.0 as isize).unwrap_or(0));
        win::PANEL_VISIBLE.store(true, Ordering::SeqCst);
        let _ = w.emit("panel-shown", ());
    }
}

fn hide_panel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
    }
    if let Some(z) = app.get_webview_window("zoom") {
        let _ = z.hide();
    }
    win::PANEL_VISIBLE.store(false, Ordering::SeqCst);
    win::PASTE_TARGET.store(0, Ordering::SeqCst);
}

#[tauri::command]
fn report_error(msg: String) {
    eprintln!("[cv-js-error] {msg}");
    let line = format!("{:?} {msg}
", std::time::SystemTime::now());
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::var("APPDATA").unwrap_or_default() + "\\com.clipvault.app\\js-errors.log")
        .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
}

#[tauri::command]
fn heartbeat() {
    cvlog!("[cv] heartbeat received");
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    HEARTBEAT.store(t, Ordering::SeqCst);
}

#[tauri::command]
fn list_clips(state: tauri::State<db::Db>, filter: String, query: String) -> Result<Vec<db::Clip>, String> {
    let conn = state.0.lock().unwrap_or_else(|p| p.into_inner());
    db::list(&conn, &filter, &query)
}

#[tauri::command]
fn stats(state: tauri::State<db::Db>) -> Result<db::Stats, String> {
    db::stats(&state.0.lock().unwrap_or_else(|p| p.into_inner()))
}

#[tauri::command]
fn get_settings(state: tauri::State<SettingsState>) -> Settings {
    state.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

#[tauri::command]
fn set_settings(app: AppHandle, state: tauri::State<SettingsState>, settings: Settings) -> Result<(), String> {
    apply_settings(&app, &settings);
    save_settings(&app, &settings);
    *state.0.lock().unwrap_or_else(|p| p.into_inner()) = settings;
    Ok(())
}


#[tauri::command]
fn save_panel_height(app: AppHandle, state: tauri::State<SettingsState>, h: f64) -> Result<(), String> {
    let h = h.clamp(360.0, 900.0);
    let mut s = state.0.lock().unwrap_or_else(|p| p.into_inner());
    s.panel_height = h;
    save_settings(&app, &s);
    Ok(())
}

#[tauri::command]
fn toggle_pin(state: tauri::State<db::Db>, id: i64) -> Result<(), String> {
    db::toggle_pin(&state.0.lock().unwrap_or_else(|p| p.into_inner()), id)
}

#[tauri::command]
fn delete_clip(state: tauri::State<db::Db>, id: i64) -> Result<(), String> {
    db::delete(&state.0.lock().unwrap_or_else(|p| p.into_inner()), id)
}

#[tauri::command]
fn paste_clip(app: AppHandle, state: tauri::State<db::Db>, id: i64) -> Result<(), String> {
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
            let (plain, html) = db::get_html(&state.0.lock().unwrap_or_else(|p| p.into_inner()), id)?;
            win::write_clipboard_text(&plain)
                && (html.is_empty() || win::write_clipboard_html(&html))
        }
        "text" if image_path.is_some() => {
            // oversized text stored in a side file
            let full = std::fs::read_to_string(image_path.unwrap_or_default()).unwrap_or_default();
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
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{GetGUIThreadInfo, GUITHREADINFO, GetForegroundWindow};
        let fg = GetForegroundWindow();
        let thread = windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        let _ = GetGUIThreadInfo(thread, &mut gi);
        eprintln!(
            "[cv] send-time: fg={:x} focus={:x}",
            fg.0 as usize,
            gi.hwndFocus.0 as usize
        );
    }
    cvlog!("[cv] paste: sending ctrl+v");
    win::request_paste_keystroke(target, focus);
    cvlog!("[cv] paste: sent");
    Ok(())
}

#[tauri::command]
fn show_panel_cmd(app: AppHandle) {
    show_panel(&app);
}

#[tauri::command]
fn hide_panel_cmd(app: AppHandle) {
    hide_panel(&app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if std::env::args().any(|a| a == "--injector") {
        win::injector_loop();
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // second launch: surface the existing instance's panel instead
            if let Some(w) = app.get_webview_window("panel") {
                let _ = w.show();
            }
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        if win::PANEL_VISIBLE.load(Ordering::SeqCst) {
                            hide_panel(app);
                        } else {
                            show_panel(app);
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            db::init(app)?;
            let settings = load_settings(app.handle());
            apply_settings(app.handle(), &settings);
            app.manage(SettingsState(Mutex::new(settings)));
            let handle = app.handle().clone();
            win::spawn_clipboard_listener(move || handle_clipboard_change(&handle));
            win::install_mouse_hook();
            let h3 = app.handle().clone();
            win::spawn_zoom_watchdog(h3);
            // webview IPC self-healing: heartbeat watchdog reloads a dead UI
            {
                let h6 = app.handle().clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(Duration::from_secs(30));
                    let Some(pw) = h6.get_webview_window("panel") else { continue };
                    let Ok(visible) = pw.is_visible() else { continue };
                    if !visible {
                        continue;
                    }
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    let last = HEARTBEAT.load(Ordering::SeqCst);
                    if last != 0 && now - last > 30_000 {
                        cvlog!("[cv] heartbeat lost, reloading webview");
                        if let Some(w) = h6.get_webview_window("panel") {
                            let _ = w.eval("location.reload()");
                        }
                        HEARTBEAT.store(now, Ordering::SeqCst);
                    }
                });
            }
            // reconcile images dir against the DB once at startup: delete
            // orphans no row references anymore
            {
                let h4 = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(20));
                    let Ok(dir) = h4.path().app_data_dir().map(|d| d.join("images")) else { return };
                    let Some(st) = h4.try_state::<db::Db>() else { return };
                    let conn = st.0.lock().unwrap_or_else(|p| p.into_inner());
                    let mut removed = 0;
                    if let Ok(entries) = std::fs::read_dir(&dir) {
                        for e in entries.flatten() {
                            let p = e.path();
                            let s = p.to_string_lossy().into_owned();
                            let referenced: bool = conn
                                .query_row(
                                    "SELECT count(*) FROM clips WHERE image_path=?1 OR image_path LIKE '%' || ?2",
                                    rusqlite::params![s, s],
                                    |r| r.get::<_, i64>(0),
                                )
                                .map(|c| c > 0)
                                .unwrap_or(true);
                            if !referenced {
                                if std::fs::remove_file(&p).is_ok() { removed += 1; }
                            }
                        }
                    }
                    if removed > 0 {
                        cvlog!("[cv] reconciled: removed {removed} orphan images");
                    }
                });
            }
            // persist user-dragged panel height: poll the real window size
            // (event plumbing proved unreliable; polling cannot be missed)
            let h5 = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(1200));
                let Some(pw) = h5.get_webview_window("panel") else { continue };
                let Ok(size) = pw.inner_size() else { continue };
                let Ok(scale) = pw.scale_factor() else { continue };
                let logical = (size.height as f64 / scale).round();
                                if !(360.0..=900.0).contains(&logical) {
                    continue;
                }
                let st = h5.state::<SettingsState>();
                let mut s = st.0.lock().unwrap_or_else(|p| p.into_inner());
                if (s.panel_height - logical).abs() > 1.0 {
                    s.panel_height = logical;
                    save_settings(&h5, &s);
                }
            });
            if let Some(z) = app.get_webview_window("zoom") {
                if let Ok(h) = z.hwnd() {
                    // NOT no-activate: the preview must take focus on click so
                    // native text selection + Ctrl+C works inside it
                    win::ZOOM_HWND.store(h.0 as isize, Ordering::SeqCst);
                }
            }
            // resident helper process that performs the paste keystroke
            let exe = std::env::current_exe().unwrap();
            let _ = std::process::Command::new(exe).arg("--injector").spawn();
            let h2 = app.handle().clone();
            std::thread::spawn(move || loop {
                if win::OUTSIDE_CLICK.swap(false, Ordering::SeqCst) {
                    hide_panel(&h2);
                }
                std::thread::sleep(Duration::from_millis(60));
            });
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;
            let show = MenuItem::with_id(app, "show", "显示面板 (Alt+V)", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 ClipVault", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("ClipVault")
                .menu(&menu)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "show" => show_panel(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
            app.global_shortcut()
                .register("Alt+V".parse::<Shortcut>().unwrap())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            report_error, heartbeat, list_clips, stats, get_settings, set_settings, save_panel_height, toggle_pin, delete_clip,
            paste_clip, show_panel_cmd, hide_panel_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
