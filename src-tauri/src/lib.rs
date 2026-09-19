/// Always-on diagnostic log: appended to cv.log in the app-data dir (capped
/// at 256 KB). Both the main process and the resident injector write here —
/// stderr is invisible for a GUI app, and this is the only way to see what a
/// user's real session did after the fact.
macro_rules! cvlog {
    ($($arg:tt)*) => {
        crate::cvlog_write(format_args!($($arg)*))
    };
}

pub fn cvlog_write(args: std::fmt::Arguments) {
    use std::io::Write;
    let dir = std::env::var("APPDATA")
        .map(|d| std::path::Path::new(&d).join("com.clipvault.app"))
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let path = dir.join("cv.log");
    // cap: delete and start fresh rather than growing forever
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 256 * 1024 {
            let _ = std::fs::remove_file(&path);
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let _ = writeln!(f, "{ts} {}", args);
    }
}

mod capture;
mod commands;
mod db;
mod settings;
mod win;

use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;
use tauri::Manager;

use commands::{hide_panel, show_panel};
use settings::{apply_settings, load_settings, SettingsState};

/// Last frontend heartbeat (ms epoch); a webview watchdog reloads the UI
/// when the panel is visible but the JS loop has gone silent.
pub(crate) static HEARTBEAT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

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
            // first paint order: hotkey + tray before anything heavy — the
            // user's first Alt+V should not wait on db migrations
            let settings = load_settings(app.handle());
            apply_settings(app.handle(), &settings);
            build_tray(app)?;
            db::init(app)?;
            app.manage(SettingsState(Mutex::new(settings)));
            let handle = app.handle().clone();
            win::spawn_clipboard_listener(move || capture::handle_clipboard_change(&handle));
            win::install_mouse_hook();
            let h3 = app.handle().clone();
            // this tick also consumes the outside-click flag (one worker
            // thread instead of two)
            win::spawn_zoom_watchdog(h3);
            spawn_heartbeat_watchdog(app.handle().clone());
            spawn_images_reconcile(app.handle().clone());
            // the paste injector is spawned on demand by request_paste_keystroke
            // (and self-heals if it died) — no eager process at boot
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::clip_content,
            commands::create_zoom,
            commands::register_zoom,
            commands::report_error,
            commands::heartbeat,
            commands::list_clips,
            commands::stats,
            commands::toggle_pin,
            commands::delete_clip,
            commands::clear_history,
            commands::paste_clip,
            commands::show_panel_cmd,
            commands::hide_panel_cmd,
            settings::get_settings,
            settings::set_settings,
            settings::save_panel_height,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Webview IPC self-healing: heartbeat watchdog reloads a dead UI.
fn spawn_heartbeat_watchdog(h: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        let Some(pw) = h.get_webview_window("panel") else {
            continue;
        };
        let Ok(visible) = pw.is_visible() else {
            continue;
        };
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
            if let Some(w) = h.get_webview_window("panel") {
                let _ = w.eval("location.reload()");
            }
            HEARTBEAT.store(now, Ordering::SeqCst);
        }
    });
}

/// Reconcile the images dir against the DB once at startup: delete orphan
/// files no row references anymore, and purge rows whose image file is gone
/// (they would otherwise show broken previews forever).
fn spawn_images_reconcile(h: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        let Ok(dir) = h.path().app_data_dir().map(|d| d.join("images")) else {
            return;
        };
        let Some(st) = h.try_state::<db::Db>() else {
            return;
        };
        let conn = st.0.lock().unwrap_or_else(|p| p.into_inner());
        let mut removed = 0;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                let s = p.to_string_lossy().into_owned();
                // thumbnails are named {hash}_t.png and never stored in
                // image_path — match them via the row's full-image path,
                // otherwise every startup would delete every thumbnail
                let referenced: bool = conn
                    .query_row(
                        "SELECT count(*) FROM clips WHERE image_path=?1 OR replace(image_path, '.png', '_t.png')=?1",
                        rusqlite::params![s, s],
                        |r| r.get::<_, i64>(0),
                    )
                    .map(|c| c > 0)
                    .unwrap_or(true);
                if !referenced && std::fs::remove_file(&p).is_ok() {
                    removed += 1;
                }
            }
        }
        let mut dead = 0;
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, image_path FROM clips WHERE kind='image' AND image_path IS NOT NULL",
        ) {
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| e.to_string());
            if let Ok(rows) = rows {
                for (id, p) in rows.flatten() {
                    if !std::path::Path::new(&p).exists() {
                        let _ = conn.execute("DELETE FROM clips WHERE id=?1", [id]);
                        dead += 1;
                    }
                }
            }
        }
        if removed > 0 || dead > 0 {
            cvlog!("[cv] reconciled: removed {removed} orphan images, {dead} dead rows");
        }
    });
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;
    let show = MenuItem::with_id(app, "show", "显示面板", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 ClipVault", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main");
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.tooltip("ClipVault")
        .menu(&menu)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "show" => show_panel(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}
