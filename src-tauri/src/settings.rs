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

pub struct SettingsState(pub Mutex<Settings>);

pub(crate) fn load_settings(app: &AppHandle) -> Settings {
    let dir = app.path().app_data_dir().unwrap_or_default();
    let raw = std::fs::read_to_string(dir.join("settings.json")).ok();
    let mut s: Settings = match &raw {
        Some(txt) => match serde_json::from_str(txt) {
            Ok(s) => s,
            Err(_) => {
                // corrupt file: keep it for recovery instead of letting the
                // next save overwrite the user's only copy
                let _ = std::fs::copy(dir.join("settings.json"), dir.join("settings.json.bak"));
                eprintln!("[cv] settings.json unreadable — defaults applied (backup at settings.json.bak)");
                Settings::default()
            }
        },
        None => Settings::default(),
    };
    // hand-edited values must not wipe history or break the window
    s.history_limit = s.history_limit.clamp(1, 5000);
    if s.panel_height.is_finite() {
        s.panel_height = s.panel_height.clamp(360.0, 900.0);
    } else {
        s.panel_height = 540.0;
    }
    // a "pause for this meeting" must never turn into a permanent silent
    // stop after a reboot — capture always resumes on launch
    s.paused = false;
    s
}

pub(crate) fn save_settings(app: &AppHandle, s: &Settings) {
    let dir = app.path().app_data_dir().unwrap_or_default();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(dir.join("settings.json"), json);
    }
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
    let app2 = app.clone();
    let hk = hotkey.to_string();
    let fb = fallback.to_string();
    app.run_on_main_thread(move || {
        let _ = tx.send(register_hotkey_now(&app2, &hk, &fb));
    })
    .map_err(|e| e.to_string())?;
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(r) => r,
        Err(_) => Err("hotkey apply timed out".into()),
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
    let mut settings = settings;
    settings.history_limit = settings.history_limit.clamp(1, 5000);
    if settings.panel_height.is_finite() {
        settings.panel_height = settings.panel_height.clamp(360.0, 900.0);
    } else {
        settings.panel_height = 540.0;
    }
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.set_size(tauri::LogicalSize::new(380.0, settings.panel_height));
    }
    win::set_autostart(settings.autostart);
    save_settings(&app, &settings);
    *state.0.lock().unwrap_or_else(|p| p.into_inner()) = settings;
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
    save_settings(&app, &s);
    Ok(())
}
