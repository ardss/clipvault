/// Always-on diagnostic log: appended to cv.log in the app-data dir (capped
/// at 256 KB). Both the main process and the resident injector write here —
/// stderr is invisible for a GUI app, and this is the only way to see what a
/// user's real session did after the fact.
macro_rules! cvlog {
    ($($arg:tt)*) => {
        crate::cvlog_write(format_args!($($arg)*))
    };
}

/// Compile-time single source for the app identifier (build.rs reads
/// tauri.conf.json). cvlog, the injector handshake dir/event and the
/// single-instance window names all derive from this constant, matching
/// app_data_dir() — a build with a changed identifier (the com.clipvault.test
/// isolation builds) is then fully isolated: data dir, log and paste channel.
pub const CV_IDENTIFIER: &str = env!("CV_IDENTIFIER");

/// cv.log lives under <APPDATA>/<identifier>/ — the same identifier the rest
/// of the app resolves via app.path().app_data_dir(). A missing APPDATA must
/// NOT fall back to the process CWD (autostart CWD is System32 — unwritable
/// for standard users, the log was silently lost): the line is dropped with
/// a stderr note instead.
fn resolve_log_dir(appdata: Option<String>) -> Option<std::path::PathBuf> {
    appdata.map(|d| std::path::Path::new(&d).join(CV_IDENTIFIER))
}

/// Set in the injector subprocess: only the main process rotates cv.log. A
/// second rotating process clobbered cv.log.1 mid-rotation (the flaky
/// cvlog rotation test; cross-process rename has no mutual exclusion).
pub static LOG_ROTATION_DISABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Serializes rotate+append within the process: two threads interleaving
/// check→remove→rename destroyed the freshly rotated cv.log.1 (observed as
/// cvlog_caps_at_256kb_and_stays_parseable failing under parallel load).
static CVLOG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn cvlog_write(args: std::fmt::Arguments) {
    match resolve_log_dir(std::env::var("APPDATA").ok()) {
        Some(dir) => cvlog_write_to(&dir, args),
        None => {
            eprintln!("[cv] cvlog: APPDATA unset — diagnostic line dropped: {args}");
        }
    }
}

/// Writes one log line into `dir/cv.log`, rotating to cv.log.1 past the cap.
/// Parameter-injected dir: tests drive this directly instead of mutating the
/// process-global APPDATA (a data race against every parallel env reader).
pub fn cvlog_write_to(dir: &std::path::Path, args: std::fmt::Arguments) {
    use std::io::Write;
    let _g = CVLOG_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let path = dir.join("cv.log");
    if !LOG_ROTATION_DISABLED.load(std::sync::atomic::Ordering::SeqCst) {
        // cap: rotate to cv.log.1 (one previous session is kept for forensics
        // — heartbeat noise once filled the log within hours and the
        // delete-and-restart cap destroyed yesterday's evidence on the first
        // overflow)
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > 256 * 1024 {
                let rotated = dir.join("cv.log.1");
                let _ = std::fs::remove_file(&rotated);
                let _ = std::fs::rename(&path, &rotated);
            }
        }
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // the whole line is built first and handed to a single write_all —
    // writeln!(f, "{ts} {args}") went through write_fmt (multiple write
    // calls) and two processes appending interleaved torn lines
    let line = format!("{ts} {args}\n");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod cvlog_tests {
    use super::*;

    fn test_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cvlog_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// cv.log must live under <APPDATA>/<identifier> — the same identifier the
    /// rest of the app resolves via app_data_dir() — and a missing APPDATA
    /// must drop the line, never fall back to the process CWD (autostart CWD
    /// is System32: unwritable for standard users, the log was silently lost).
    #[test]
    fn cvlog_dir_is_identifier_derived_and_never_falls_back_to_cwd() {
        let dir =
            resolve_log_dir(Some("X:\\appdata-root".into())).expect("Some(APPDATA) must resolve");
        assert_eq!(
            dir,
            std::path::Path::new("X:\\appdata-root").join(CV_IDENTIFIER),
            "the dir must derive from the compile-time identifier, not a hardcoded copy"
        );
        assert_eq!(
            resolve_log_dir(None),
            None,
            "APPDATA unset must drop the line instead of writing into the CWD"
        );
    }

    /// (d) cv.log cap: once the log exceeds 256 KB, the next write must rotate
    /// it to cv.log.1 and start fresh. Driven through cvlog_write_to with an
    /// injected dir — the old APPDATA set_var raced every parallel test that
    /// reads the environment.
    #[test]
    fn cvlog_caps_at_256kb_and_stays_parseable() {
        let dir = test_dir("cap");
        let log = dir.join("cv.log");

        // pre-fill past the cap
        std::fs::write(&log, vec![b'x'; 300 * 1024]).unwrap();

        cvlog_write_to(
            &dir,
            format_args!("after-cap marker {}", std::process::id()),
        );

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
        let rotated = dir.join("cv.log.1");
        assert!(rotated.exists(), "cv.log.1 must exist after rotation");
        assert!(
            std::fs::read_to_string(&rotated).unwrap().contains('x'),
            "rotated log must hold the old content"
        );
        // a second overflow rotates again: the oversized log ('y') moves to
        // cv.log.1, the new marker lands in a fresh cv.log (bounded disk)
        std::fs::write(&log, vec![b'y'; 300 * 1024]).unwrap();
        cvlog_write_to(&dir, format_args!("second marker {}", std::process::id()));
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

    /// Concurrent writers must not destroy the rotated evidence. The exact
    /// flake the parallel test suite exposed: thread A rotates ('x' →
    /// cv.log.1), thread B's check→rename then clobbers cv.log.1 with the
    /// fresh marker log — yesterday's forensics gone.
    #[test]
    fn concurrent_writers_keep_rotated_evidence_intact() {
        let dir = test_dir("race");
        let log = dir.join("cv.log");
        std::fs::write(&log, vec![b'x'; 300 * 1024]).unwrap();
        std::thread::scope(|s| {
            for i in 0..16 {
                let dir = &dir;
                s.spawn(move || {
                    cvlog_write_to(dir, format_args!("race writer {i}"));
                });
            }
        });
        let rotated = std::fs::read_to_string(dir.join("cv.log.1")).unwrap();
        assert!(
            rotated.contains('x') && !rotated.contains("race writer"),
            "cv.log.1 must hold ONLY the rotated old content — a concurrent \
             rotation clobbered the evidence"
        );
        let fresh = std::fs::read_to_string(&log).unwrap();
        assert!(
            fresh.contains("race writer"),
            "post-rotation markers must land in the fresh log"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod heartbeat_watchdog_tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// The watchdog must run on a monotonic clock (wall-clock epoch ms jumped
    /// forward after sleep/resume — reloading a HEALTHY panel — and backward
    /// after NTP correction, silently killing the watchdog), and must demand
    /// two consecutive stale 30s checks before reloading (the webview beats
    /// every 5s; a single window races the renderer coming back after resume).
    #[test]
    fn watchdog_uses_monotonic_clock_and_demands_two_consecutive_misses() {
        let t0 = Instant::now();
        // never beat → never reload (the webview may simply not have started)
        assert_eq!(watchdog_step(None, t0, 0), (false, 0));
        // fresh beat → healthy; the miss counter resets
        assert_eq!(watchdog_step(Some(t0), t0, 3), (false, 0));
        // first stale check: wait one more window (sleep/resume grace)
        assert_eq!(
            watchdog_step(Some(t0 - Duration::from_secs(31)), t0, 0),
            (false, 1)
        );
        // two consecutive stale checks → reload, counter restarts
        assert_eq!(
            watchdog_step(Some(t0 - Duration::from_secs(31)), t0, 1),
            (true, 0)
        );
        // monotonicity is structural: Instant has no "jumped backward"
        // representation — duration_since is clamped at zero, so a wall-clock
        // style `now - last < 0` dead branch cannot even be expressed
        assert_eq!(
            watchdog_step(Some(t0), t0 + Duration::from_secs(120), 0),
            (false, 1)
        );
    }
}

mod capture;
mod commands;
mod db;
mod settings;
mod win;

// L2 guard tests only — the module itself is empty outside #[cfg(test)].
#[cfg(test)]
mod guards;

use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;
use tauri::Manager;

use commands::{hide_panel, show_panel};
use settings::{apply_settings, load_settings, SettingsState};

/// Last frontend heartbeat as a monotonic Instant. A wall-clock epoch value
/// jumped forward after sleep/resume (watchdog reloaded a HEALTHY panel) and
/// backward after NTP correction (`now - last` went negative and silently
/// disabled the watchdog). Instant makes the backward case unrepresentable.
pub(crate) static LAST_BEAT: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// One watchdog decision (pure, unit-tested): is the beat stale on the
/// monotonic clock, and have TWO consecutive 30s checks agreed on it? A
/// single stale window raced the webview's own 5s beat after sleep/resume —
/// the first tick after resume could fire before the renderer beat again,
/// reloading a healthy UI (flash, lost search text).
fn watchdog_step(
    last: Option<std::time::Instant>,
    now: std::time::Instant,
    missed: u32,
) -> (bool, u32) {
    let stale = match last {
        None => false, // never beat: the webview may not have started yet
        Some(t) => now.duration_since(t) > Duration::from_secs(30),
    };
    if !stale {
        return (false, 0);
    }
    let missed = missed + 1;
    if missed >= 2 {
        (true, 0)
    } else {
        (false, missed)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if std::env::args().any(|a| a == "--injector") {
        // the injector is a second cv.log writer — it must never rotate
        // (cross-process rename has no mutual exclusion; see CVLOG_LOCK)
        LOG_ROTATION_DISABLED.store(true, std::sync::atomic::Ordering::SeqCst);
        win::injector_loop();
    }
    if std::env::args().any(|a| a == "--alt-tap") {
        win::helper_alt_tap();
    }
    tauri::Builder::default()
        // the zoom window is destroyed by the frontend idle reaper; without
        // this, ZOOM_HWND kept the dead HWND and Win32 handle recycling made
        // it point at an unrelated live window (broke outside-click hiding
        // and paste-target capture)
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) && window.label() == "zoom" {
                win::ZOOM_HWND.store(0, Ordering::SeqCst);
            }
        })
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
            // derive the plugin window names from the RUNTIME identifier (the
            // plugin does the same from app.config()) — never a hardcoded copy
            win::hide_single_instance_window(app.config().identifier.as_str());
            let (settings, prune_frozen) = load_settings(app.handle());
            apply_settings(app.handle(), &settings);
            build_tray(app)?;
            db::init(app)?;
            app.manage(SettingsState(
                Mutex::new(settings),
                std::sync::atomic::AtomicBool::new(prune_frozen),
            ));
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

/// Webview IPC self-healing: heartbeat watchdog reloads a dead UI. Runs on
/// the monotonic clock (LAST_BEAT) and demands two consecutive stale checks.
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
        static MISSED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let missed = MISSED.load(Ordering::SeqCst);
        let last = *LAST_BEAT.lock().unwrap_or_else(|p| p.into_inner());
        let (reload, next_missed) = watchdog_step(last, std::time::Instant::now(), missed);
        MISSED.store(next_missed, Ordering::SeqCst);
        if reload {
            cvlog!("[cv] heartbeat lost, reloading webview");
            if let Some(w) = h.get_webview_window("panel") {
                let _ = w.eval("location.reload()");
            }
            // restart the grace window: one reload per incident, not per tick
            *LAST_BEAT.lock().unwrap_or_else(|p| p.into_inner()) = Some(std::time::Instant::now());
        }
    });
}

/// Reconcile the images dir against the DB once at startup: delete orphan
/// files no row references anymore. History rows whose image file is gone
/// are only COUNTED (for the log), never purged — a missing file must not
/// silently erase clipboard history (the startup test locks this in; do not
/// "fix" this function into deleting rows).
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

/// Integration-test surface (tests/ipc_contract.rs): a mock-runtime app
/// carrying the REAL invoke handler — all 16 IPC commands exactly as
/// `run()` registers them — plus the REAL managed state (in-memory Db with
/// the production schema, default SettingsState). It deliberately does NOT
/// run run()'s setup: no tray, no global hotkey, no clipboard listener, no
/// mouse hook — a headless runner has no desktop for those, and mock_runtime
/// needs none of them for pure command dispatch.
///
/// Not feature-gated: `cfg(feature = "test")`/`cfg(test)` do not apply to
/// integration-test targets, and CI's bare `cargo test` must pick this up
/// unmodified. It compiles to one unreferenced function in the shipped
/// binary (main.rs only calls run()).
pub fn ipc_test_app() -> tauri::App<tauri::test::MockRuntime> {
    let app = tauri::test::mock_builder()
        // set_settings' hotkey path calls app.global_shortcut(); the plugin
        // must be present for the validation branch to run instead of panicking.
        // On MockRuntime shortcut registration lands in an in-memory map —
        // no real OS hotkey is claimed.
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app build");
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
    db::setup_schema(&conn).expect("schema setup on in-memory db");
    app.manage(db::Db(Mutex::new(conn)));
    app.manage(SettingsState(
        Mutex::new(settings::Settings::default()),
        // started "frozen" like the corrupt-file fallback: tests observe the
        // unfreeze via set_settings/save_panel_height
        std::sync::atomic::AtomicBool::new(true),
    ));
    app
}

/// Locked handle on the mock app's managed Db connection — lets integration
/// tests seed/drain rows through the production schema without reaching into
/// the private `db` module.
pub fn ipc_test_db(
    app: &tauri::App<tauri::test::MockRuntime>,
) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
    use tauri::Manager;
    let st: &db::Db = app.state::<db::Db>().inner();
    st.0.lock().unwrap_or_else(|p| p.into_inner())
}

/// Signature probe for tests/ipc_contract.rs: show_panel_cmd's only behavior
/// is real system side effects (WH_MOUSE_LL hook install), so the contract
/// test asserts its registered signature at compile time instead of invoking
/// it (plan §3.3 degrade rule ③).
#[allow(non_upper_case_globals)]
pub static __probe_show_panel_cmd_sig: fn(tauri::AppHandle) = commands::show_panel_cmd;
