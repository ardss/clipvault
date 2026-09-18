/// Debug logging gated by CV_LOG=1 — release builds stay silent.
macro_rules! cvlog {
    ($($arg:tt)*) => {
        if std::env::var("CV_LOG").is_ok() {
            eprintln!($($arg)*);
        }
    };
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
use settings::{apply_settings, load_settings, save_settings, SettingsState};

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
            db::init(app)?;
            let settings = load_settings(app.handle());
            apply_settings(app.handle(), &settings);
            app.manage(SettingsState(Mutex::new(settings)));
            let handle = app.handle().clone();
            win::spawn_clipboard_listener(move || capture::handle_clipboard_change(&handle));
            win::install_mouse_hook();
            let h3 = app.handle().clone();
            win::spawn_zoom_watchdog(h3);
            spawn_heartbeat_watchdog(app.handle().clone());
            spawn_images_reconcile(app.handle().clone());
            spawn_height_poller(app.handle().clone());
            if let Some(z) = app.get_webview_window("zoom") {
                if let Ok(h) = z.hwnd() {
                    // NOT no-activate: the preview must take focus on click so
                    // native text selection + Ctrl+C works inside it
                    win::ZOOM_HWND.store(h.0 as isize, Ordering::SeqCst);
                }
            }
            // resident helper process that performs the paste keystroke
            if let Ok(exe) = std::env::current_exe() {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                let _ = std::process::Command::new(exe)
                    .arg("--injector")
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn();
            }
            let h2 = app.handle().clone();
            std::thread::spawn(move || loop {
                if win::OUTSIDE_CLICK.swap(false, Ordering::SeqCst) {
                    hide_panel(&h2);
                }
                std::thread::sleep(Duration::from_millis(60));
            });
            build_tray(app)?;
            // hotkey itself is registered by apply_settings above (uses the
            // configured value; falls back to Alt+V)
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
                let referenced: bool = conn
                    .query_row(
                        "SELECT count(*) FROM clips WHERE image_path=?1 OR image_path LIKE '%' || ?2",
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

/// Persist user-dragged panel height: poll the real window size (event
/// plumbing proved unreliable; polling cannot be missed).
fn spawn_height_poller(h: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(1200));
        let Some(pw) = h.get_webview_window("panel") else {
            continue;
        };
        let Ok(size) = pw.inner_size() else { continue };
        let Ok(scale) = pw.scale_factor() else {
            continue;
        };
        let logical = (size.height as f64 / scale).round();
        if !(360.0..=900.0).contains(&logical) {
            continue;
        }
        let st = h.state::<SettingsState>();
        let mut s = st.0.lock().unwrap_or_else(|p| p.into_inner());
        if (s.panel_height - logical).abs() > 1.0 {
            s.panel_height = logical;
            save_settings(&h, &s);
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
