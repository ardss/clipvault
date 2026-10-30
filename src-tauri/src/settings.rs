//! Settings: model, persistence, application (hotkey/autostart/panel size)
//! and the settings-related Tauri commands.
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::win;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct Settings {
    pub history_limit: i64,
    pub panel_height: f64,
    pub autostart: bool,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default)]
    pub sensitive_keywords: Vec<String>,
    #[serde(default)]
    pub paused: bool,
}

fn default_hotkey() -> String {
    "Alt+V".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            history_limit: 1000,
            panel_height: 540.0,
            autostart: false,
            hotkey: default_hotkey(),
            sensitive_keywords: Vec::new(),
            paused: false,
        }
    }
}

pub struct SettingsState(
    pub Mutex<Settings>,
    /// true while the in-memory settings are a parse-failure FALLBACK:
    /// history pruning is frozen until the user explicitly saves, because
    /// pruning with the DEFAULT limit would irreversibly delete history
    /// beyond 1000 entries (files included) the user never approved losing.
    pub std::sync::atomic::AtomicBool,
);

fn normalize(mut s: Settings) -> Settings {
    // hand-edited values must not wipe history or break the window
    s.history_limit = s.history_limit.clamp(1, 5000);
    s.panel_height = if s.panel_height.is_finite() {
        s.panel_height.clamp(360.0, 900.0)
    } else {
        540.0
    };
    // a "pause for this meeting" must never turn into a permanent silent
    // stop after a reboot — capture always resumes on launch
    s.paused = false;
    s
}

/// Reads settings.json, recovering from settings.json.bak (the last
/// known-good save) when the primary file is unreadable. Returns the
/// settings and whether we had to fall back to defaults (drives the
/// prune freeze). The corrupt bytes are preserved under
/// settings.json.corrupt; the .bak is never overwritten — the old code
/// copied the CORRUPT file onto settings.json.bak and called that "keep it
/// for recovery", so the backup never held a good version and nothing
/// ever read it.
fn read_settings_from(dir: &std::path::Path) -> (Settings, bool) {
    let primary = std::fs::read_to_string(dir.join("settings.json")).ok();
    if let Some(txt) = &primary {
        if let Ok(s) = serde_json::from_str::<Settings>(txt) {
            return (normalize(s), false);
        }
    }
    if let Ok(bak) = std::fs::read_to_string(dir.join("settings.json.bak")) {
        if let Ok(s) = serde_json::from_str::<Settings>(&bak) {
            crate::cvlog_write(format_args!(
                "[cv] settings.json unreadable — recovered from settings.json.bak"
            ));
            return (normalize(s), false);
        }
    }
    if primary.is_some() {
        let _ = std::fs::copy(dir.join("settings.json"), dir.join("settings.json.corrupt"));
    }
    crate::cvlog_write(format_args!(
        "[cv] settings.json unreadable and no usable .bak — defaults applied, \
         history pruning frozen until the next explicit save"
    ));
    (normalize(Settings::default()), true)
}

pub(crate) fn load_settings(app: &AppHandle) -> (Settings, bool) {
    let dir = app.path().app_data_dir().unwrap_or_default();
    read_settings_from(&dir)
}

pub(crate) fn save_settings(app: &AppHandle, s: &Settings) {
    let dir = app.path().app_data_dir().unwrap_or_default();
    save_settings_to(&dir, s);
}

fn save_settings_to(dir: &std::path::Path, s: &Settings) {
    let _ = std::fs::create_dir_all(dir);
    let Ok(json) = serde_json::to_string_pretty(s) else {
        return;
    };
    // atomic write: a crash mid-save must not leave a torn settings file
    let tmp = dir.join("settings.json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join("settings.json"));
    }
}

/// set_settings used to overwrite the whole file with the FRONTEND snapshot
/// taken when the settings UI opened — a save_panel_height persisted in the
/// meantime was silently rolled back (lost update; the lock only prevents
/// interleaved torn writes, not staleness). The panel's real height is the
/// source of truth and always wins.
fn merge_preserving_panel_height(incoming: Settings, current_panel_height: f64) -> Settings {
    let mut merged = incoming;
    merged.panel_height = current_panel_height;
    normalize(merged)
}

pub(crate) fn apply_settings(app: &AppHandle, s: &Settings) {
    if let Some(w) = app.get_webview_window("panel") {
        let h = if s.panel_height.is_finite() {
            s.panel_height.clamp(360.0, 900.0)
        } else {
            540.0
        };
        let _ = w.set_size(tauri::LogicalSize::new(380.0, h));
    }
    win::set_autostart(s.autostart);
    // setup runs on the main thread, so a direct register is correct here
    let _ = register_hotkey_now(app, &s.hotkey, "Alt+V");
}

/// Unregisters everything and registers the configured combo. Must run on the
/// main thread — RegisterHotKey delivers WM_HOTKEY to the registering
/// thread's message queue, and only the main thread pumps the plugin's loop.
/// `fallback` is re-registered when the requested combo fails, so the app is
/// never left without any summon shortcut.
fn register_hotkey_now(app: &AppHandle, hotkey: &str, fallback: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
    let g = app.global_shortcut();
    // validate BEFORE unregister_all — a rejected change must not destroy the
    // currently working hotkey
    let parsed = hotkey
        .trim()
        .parse::<Shortcut>()
        .map_err(|_| format!("unparseable hotkey: {hotkey:?}"))?;
    // a modifier-less combo (e.g. just "A") would swallow that key in every app
    if parsed.mods.is_empty() {
        return Err(format!("hotkey needs a modifier key: {hotkey:?}"));
    }
    let _ = g.unregister_all();
    if g.register(parsed).is_ok() {
        crate::cvlog_write(format_args!("[cv] hotkey registered: {hotkey}"));
        return Ok(());
    }
    let fb = fallback
        .trim()
        .parse::<Shortcut>()
        .map_err(|_| format!("unparseable fallback hotkey: {fallback:?}"))?;
    g.register(fb)
        .map_err(|e| format!("{hotkey} register failed ({e}); fallback also failed"))?;
    Err(format!(
        "{hotkey} is taken by another app — kept {fallback}"
    ))
}

/// set_settings runs on an IPC worker thread: post the registration to the
/// main thread and wait briefly for its result so a conflict surfaces as Err.
fn register_hotkey_via_main(app: &AppHandle, hotkey: &str, fallback: &str) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let applied = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let app2 = app.clone();
    let hk = hotkey.to_string();
    let fb = fallback.to_string();
    let flag = applied.clone();
    app.run_on_main_thread(move || {
        let r = register_hotkey_now(&app2, &hk, &fb);
        if r.is_ok() {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        let _ = tx.send(r);
    })
    .map_err(|e| e.to_string())?;
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(r) => r,
        Err(_) => {
            // the queued closure may still fire later — the compensation only
            // re-registers the previous combo if the original never applied,
            // otherwise it would clobber the user's newly-set hotkey
            let app3 = app.clone();
            let fb = fallback.to_string();
            let flag = applied;
            let _ = app.run_on_main_thread(move || {
                if !flag.load(std::sync::atomic::Ordering::SeqCst) {
                    let _ = register_hotkey_now(&app3, &fb, &fb);
                }
            });
            Err("hotkey apply timed out".into())
        }
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "cvset_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn good_json(limit: i64, height: f64) -> String {
        format!(
            r#"{{"history_limit":{limit},"panel_height":{height},"autostart":false,"hotkey":"Alt+V","sensitive_keywords":["token"],"paused":false}}"#
        )
    }

    /// A corrupt settings.json must recover from settings.json.bak (the last
    /// known-good save) instead of silently falling back to defaults — the
    /// default limit would prune history beyond 1000 entries and the empty
    /// keyword list would silently disable the sensitive-copy filter.
    #[test]
    fn corrupt_settings_recovers_from_the_bak_instead_of_defaulting() {
        let dir = temp_dir("bak-recovery");
        std::fs::write(dir.join("settings.json"), "{ this is not json").unwrap();
        std::fs::write(dir.join("settings.json.bak"), good_json(4242, 700.0)).unwrap();
        let (s, fell_back) = read_settings_from(&dir);
        assert!(!fell_back, "a usable .bak means NO fallback to defaults");
        assert_eq!(s.history_limit, 4242, "recovered settings must be used");
        assert_eq!(s.sensitive_keywords, vec!["token".to_string()]);
        // the recovery source must survive
        assert!(dir.join("settings.json.bak").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With no usable .bak, defaults apply AND history pruning must be
    /// frozen (fell_back=true drives that). The corrupt bytes are preserved
    /// for forensics under settings.json.corrupt — and the .bak is NEVER
    /// created from corrupt bytes (the old code copied the broken file onto
    /// settings.json.bak and called that "backup"; it never held a good
    /// version and nothing ever read it).
    #[test]
    fn fallback_defaults_freeze_prune_and_never_clobber_the_bak() {
        let dir = temp_dir("fallback");
        std::fs::write(dir.join("settings.json"), "CORRUPT-BYTES").unwrap();
        let (s, fell_back) = read_settings_from(&dir);
        assert!(
            fell_back,
            "unreadable primary + no .bak must report fallback"
        );
        assert_eq!(s.history_limit, 1000); // default — hence the freeze
        assert!(
            dir.join("settings.json.corrupt").exists(),
            "corrupt bytes preserved for forensics"
        );
        assert!(
            !dir.join("settings.json.bak").exists(),
            "a .bak must never be minted from corrupt bytes"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A healthy settings.json parses normally, no fallback.
    #[test]
    fn healthy_settings_parse_with_no_fallback() {
        let dir = temp_dir("healthy");
        std::fs::write(dir.join("settings.json"), good_json(777, 640.0)).unwrap();
        let (mut s, fell_back) = read_settings_from(&dir);
        assert!(!fell_back);
        assert_eq!(s.history_limit, 777);
        assert_eq!(s.panel_height, 640.0);
        // save/load roundtrip through the atomic writer
        s.panel_height = 800.0;
        save_settings_to(&dir, &s);
        let (s2, f2) = read_settings_from(&dir);
        assert!(!f2);
        assert_eq!(s2.panel_height, 800.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// set_settings must not write the stale frontend snapshot's
    /// panel_height over a fresher save_panel_height value (lost update:
    /// drag the panel taller, then flip any setting → the height rolled
    /// back). The panel's real height is the source of truth.
    #[test]
    fn panel_height_from_a_concurrent_save_wins_over_the_frontend_snapshot() {
        let snapshot = Settings {
            history_limit: 2000,
            panel_height: 540.0, // stale — snapshot from when settings opened
            autostart: true,
            hotkey: "Ctrl+Shift+V".into(),
            sensitive_keywords: vec!["pw".into()],
            paused: false,
        };
        let merged = merge_preserving_panel_height(snapshot, 812.0);
        assert_eq!(merged.panel_height, 812.0, "the fresher height must win");
        assert_eq!(merged.history_limit, 2000, "user edits still apply");
        assert_eq!(merged.hotkey, "Ctrl+Shift+V");
        assert_eq!(merged.sensitive_keywords, vec!["pw".to_string()]);
        // and the merge result survives a disk roundtrip intact
        let dir = temp_dir("merge-roundtrip");
        save_settings_to(&dir, &merged);
        let (back, fell_back) = read_settings_from(&dir);
        assert!(!fell_back);
        assert_eq!(back.panel_height, 812.0);
        assert_eq!(back.history_limit, 2000);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[tauri::command]
pub(crate) fn get_settings(state: tauri::State<SettingsState>) -> Settings {
    state.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

#[tauri::command]
pub(crate) fn set_settings(
    app: AppHandle,
    state: tauri::State<SettingsState>,
    settings: Settings,
) -> Result<(), String> {
    // hotkey first: on conflict (another app owns the combo) we fail loudly
    // and keep the previous settings — the fallback re-registers the OLD
    // hotkey so a rejected change never leaves the app shortcut-less
    let previous = state
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .hotkey
        .clone();
    register_hotkey_via_main(&app, &settings.hotkey, &previous)?;
    win::set_autostart(settings.autostart);
    // save while holding the state lock, MERGED with the current in-memory
    // state: `settings` is a frontend snapshot from when the settings UI
    // opened, and a save_panel_height persisted in the meantime must not be
    // rolled back (lost update — the lock only prevents torn writes)
    let mut current = state.0.lock().unwrap_or_else(|p| p.into_inner());
    let merged = merge_preserving_panel_height(settings, current.panel_height);
    // an explicit save means the settings are real again — unfreeze pruning
    state.1.store(false, std::sync::atomic::Ordering::SeqCst);
    save_settings(&app, &merged);
    *current = merged;
    drop(current);
    Ok(())
}

#[tauri::command]
pub(crate) fn save_panel_height(
    app: AppHandle,
    state: tauri::State<SettingsState>,
    h: f64,
) -> Result<(), String> {
    let h = h.clamp(360.0, 900.0);
    let mut s = state.0.lock().unwrap_or_else(|p| p.into_inner());
    s.panel_height = h;
    // an explicit save means the settings are real again — unfreeze pruning
    state.1.store(false, std::sync::atomic::Ordering::SeqCst);
    save_settings(&app, &s);
    Ok(())
}
