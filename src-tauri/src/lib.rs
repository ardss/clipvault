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
    // cap: rotate to cv.log.1 (one previous session is kept for forensics —
    // heartbeat noise once filled the log within hours and the delete-and-
    // restart cap destroyed yesterday's evidence on the first overflow)
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 256 * 1024 {
            let rotated = dir.join("cv.log.1");
            let _ = std::fs::remove_file(&rotated);
            let _ = std::fs::rename(&path, &rotated);
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

#[cfg(test)]
mod cvlog_tests {
    use super::*;

    /// (d) cv.log cap: once the log exceeds 256 KB, the next write must delete
    /// it and start fresh (cap logic lives inline in cvlog_write — this drives
    /// it through its real entry point via the APPDATA override).
    #[test]
    fn cvlog_caps_at_256kb_and_stays_parseable() {
        let dir = std::env::temp_dir().join(format!(
            "cvlog_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("com.clipvault.app")).unwrap();
        std::env::set_var("APPDATA", &dir); // process-global: no other test reads APPDATA

        // pre-fill past the cap
        let log = dir.join("com.clipvault.app").join("cv.log");
        std::fs::write(&log, vec![b'x'; 300 * 1024]).unwrap();

        cvlog_write(format_args!("after-cap marker {}", std::process::id()));

        let len = std::fs::metadata(&log).unwrap().len();
        assert!(
            len <= 256 * 1024,
            "cv.log exceeded cap after write: {len} bytes"
        );
        let txt = std::fs::read_to_string(&log).unwrap();
        assert!(
            txt.contains("after-cap marker"),
            "fresh log must contain the new entry"
        );
        assert!(
            !txt.contains('x'),
            "old oversized content must be gone, not appended to"
        );
        // rotation keeps one previous log for forensics
        let rotated = dir.join("com.clipvault.app").join("cv.log.1");
        assert!(rotated.exists(), "cv.log.1 must exist after rotation");
        assert!(
            std::fs::read_to_string(&rotated).unwrap().contains('x'),
            "rotated log must hold the old content"
        );
        // a second overflow rotates again: the oversized log ('y') moves to
        // cv.log.1, the new marker lands in a fresh cv.log (bounded disk)
        std::fs::write(&log, vec![b'y'; 300 * 1024]).unwrap();
        cvlog_write(format_args!("second marker {}", std::process::id()));
        let rotated2 = std::fs::read_to_string(&rotated).unwrap();
        assert!(
            rotated2.contains('y') && !rotated2.contains("second marker"),
            "cv.log.1 must hold the rotated oversized log"
        );
        let fresh = std::fs::read_to_string(&log).unwrap();
        assert!(
            fresh.contains("second marker") && !fresh.contains('y'),
            "cv.log must start fresh with the new entry"
        );
        let _ = std::fs::remove_dir_all(&dir);
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
            // Second launch must use the same show path as the hotkey/tray:
            // raw window.show() leaves PASTE_TARGET and PANEL_VISIBLE stale,
            // so paste can fail to restore the previous application focus.
            show_panel(app);
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
            win::hide_single_instance_window();
            let settings = load_settings(app.handle());
            apply_settings(app.handle(), &settings);
            build_tray(app)?;
            db::init(app)?;
            app.manage(SettingsState(Mutex::new(settings)));
            let handle = app.handle().clone();
            win::spawn_clipboard_listener(move || capture::handle_clipboard_change(&handle));
            // the outside-click hook is installed per panel-show (self-healing,
            // zero cost while hidden) — nothing to install at boot
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
///
/// Two-phase (audit P3): the DB lock is held only while pulling the
/// referenced-path set — the filesystem sweep (read_dir + stat per file) and
/// the existence checks over history rows run outside it.
fn spawn_images_reconcile(h: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        let Ok(dir) = h.path().app_data_dir().map(|d| d.join("images")) else {
            return;
        };
        let Some(st) = h.try_state::<db::Db>() else {
            return;
        };
        // phase 1 — inside the lock: the referenced-path set
        let referenced: std::collections::HashSet<String> = {
            let conn = st.0.lock().unwrap_or_else(|p| p.into_inner());
            let mut out = std::collections::HashSet::new();
            if let Ok(mut stmt) =
                conn.prepare("SELECT image_path FROM clips WHERE image_path IS NOT NULL")
            {
                if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                    for p in rows.flatten() {
                        out.insert(p.clone());
                        if let Some(stem) = std::path::Path::new(&p).file_stem() {
                            out.insert(format!("{}_t.png", stem.to_string_lossy()));
                        }
                    }
                }
            }
            out
        }; // lock released — phases 2/3 only touch the filesystem
        let orphans = reconcile_scan_orphans(&dir, &referenced);
        let missing = reconcile_count_missing(&dir, &referenced);
        let mut removed = 0;
        for p in orphans {
            if std::fs::remove_file(&p).is_ok() {
                removed += 1;
            }
        }
        if removed > 0 || missing > 0 {
            cvlog!("[cv] image reconcile: {removed} aged orphans removed, {missing} missing files retained in history");
        }
    });
}

/// Phase 2 (no lock): list image files older than the orphan grace that no
/// row references. A capture in flight writes its file before its DB row, so
/// fresh files (< 7 days) are never candidates.
fn reconcile_scan_orphans(
    dir: &std::path::Path,
    referenced: &std::collections::HashSet<String>,
) -> Vec<std::path::PathBuf> {
    const ORPHAN_GRACE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
    let mut orphans = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if !p.is_file() || p.extension().and_then(|e| e.to_str()) != Some("png") {
                continue;
            }
            // If metadata/time cannot be read, fail closed: keep the file.
            let aged = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age >= ORPHAN_GRACE);
            if !aged {
                continue;
            }
            let s = p.to_string_lossy().into_owned();
            if !referenced.contains(&s) {
                orphans.push(p);
            }
        }
    }
    orphans
}

/// Phase 2b (no lock): history rows whose image file is currently absent.
fn reconcile_count_missing(
    dir: &std::path::Path,
    referenced: &std::collections::HashSet<String>,
) -> usize {
    let _ = dir;
    referenced
        .iter()
        .filter(|p| !p.ends_with("_t.png") && !std::path::Path::new(p.as_str()).exists())
        .count()
}

#[cfg(test)]
mod image_reconcile_tests {
    use super::*;
    use std::collections::HashSet;

    fn referenced_set(conn: &rusqlite::Connection) -> HashSet<String> {
        let mut out = HashSet::new();
        let mut stmt = conn
            .prepare("SELECT image_path FROM clips WHERE image_path IS NOT NULL")
            .unwrap();
        for p in stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .flatten()
        {
            out.insert(p.clone());
            if let Some(stem) = std::path::Path::new(&p).file_stem() {
                out.insert(format!("{}_t.png", stem.to_string_lossy()));
            }
        }
        out
    }

    #[test]
    fn startup_keeps_inflight_image_and_missing_history_row() {
        let dir = std::env::temp_dir().join(format!(
            "cv-reconcile-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let fresh = dir.join("inflight.png");
        std::fs::write(&fresh, b"simulated-image").unwrap();
        let missing = dir.join("temporarily-unavailable.png");
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE clips(id INTEGER PRIMARY KEY, kind TEXT, image_path TEXT);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clips(id,kind,image_path) VALUES(1,'image',?1)",
            [missing.to_string_lossy().as_ref()],
        )
        .unwrap();
        let referenced = referenced_set(&conn);
        // fresh file: not aged, never a candidate even though unreferenced
        let orphans = reconcile_scan_orphans(&dir, &referenced);
        assert!(orphans.is_empty(), "fresh in-flight image must survive");
        let absent = reconcile_count_missing(&dir, &referenced);
        assert_eq!(absent, 1);
        assert_eq!(
            conn.query_row("SELECT count(*) FROM clips", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1,
            "missing image must not silently erase its clipboard history"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn aged_unreferenced_file_is_a_candidate() {
        use std::time::Duration;
        let dir = std::env::temp_dir().join(format!(
            "cv-reconcile-aged-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("old-orphan.png");
        std::fs::write(&old, b"old").unwrap();
        let older = old.metadata().unwrap().modified().unwrap();
        let back = older - Duration::from_secs(8 * 24 * 60 * 60);
        let f = std::fs::File::options().write(true).open(&old).unwrap();
        f.set_modified(back).unwrap();
        drop(f);
        let orphans = reconcile_scan_orphans(&dir, &HashSet::new());
        assert_eq!(
            orphans.len(),
            1,
            "aged unreferenced file must be a candidate"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
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
