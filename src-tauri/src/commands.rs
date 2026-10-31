//! Tauri commands plus the panel show/hide choreography they share with the
//! hotkey handler and the tray menu.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::db;
use crate::win;

// A second launch while the panel already owns focus must not replace the
// previously captured paste destination with ClipVault's own window.
fn should_capture_paste_target(foreground: isize, panel: isize, zoom: isize) -> bool {
    foreground != 0 && foreground != panel && foreground != zoom
}

// Owner PID of a window (0 when the handle is gone).
fn window_pid(target: isize) -> u32 {
    if target == 0 {
        return 0;
    }
    let mut pid = 0u32;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
            windows::Win32::Foundation::HWND(target as _),
            Some(&mut pid),
        );
    }
    pid
}

fn validated_paste_target(target: isize, want_pid: u32) -> Result<isize, String> {
    // The previously captured editor may have closed while the panel stayed
    // open, and its HWND value may even have been recycled by an unrelated
    // window. Reject anything that is not provably the same window — the
    // (handle, owner pid) pair captured together at show time — before any
    // system clipboard write.
    let alive = target != 0
        && want_pid != 0
        && unsafe {
            windows::Win32::UI::WindowsAndMessaging::IsWindow(windows::Win32::Foundation::HWND(
                target as _,
            ))
        }
        .as_bool()
        && window_pid(target) == want_pid;
    if !alive {
        Err("FOCUS_LOST".into())
    } else {
        Ok(target)
    }
}

/// Contract decision (verified against GPT review 2026-10-24, P1-1/P1-2):
/// which child value the injector handshake carries for a restore outcome.
/// `None` cancels the paste — there is NO path that turns an uncertain state
/// into a looser match. Pure, so the whole matrix is unit-testable.
fn handshake_child(
    restore: win::FocusRestore,
    child: isize,
    focus_in_target: Option<isize>,
) -> Option<isize> {
    match restore {
        // Verified restore: proceed ONLY if the live attribution still names
        // the verified child — focus that moved after the restore cancels
        // the paste (contract: a state change during the recovery window
        // aborts immediately; it never rides on a stale identity). The
        // self-focus case must report the target itself as current focus.
        win::FocusRestore::Restored => (focus_in_target == Some(child)).then_some(child),
        // Nothing captured → the established top-level-only strategy.
        win::FocusRestore::NoCapturedChild => Some(0),
        // Dead control: degrade to top-level matching ONLY when the live
        // focus provably belongs to the verified target (the downgraded
        // handshake value is 0); unknown attribution fails closed.
        win::FocusRestore::ChildDestroyed => focus_in_target.map(|_| 0),
        // Alive-but-foreign identity, or the restore didn't achieve its
        // expectation: never downgrade an uncertain state into a loose
        // match — a restore failure is not a reason to skip restoring.
        win::FocusRestore::IdentityMismatch | win::FocusRestore::RestoreFailed => None,
    }
}

/// Cancel-reason logging with distinct, greppable statistic tokens
/// (child_destroyed / child_identity_mismatch / restore_failed — counted
/// separately from the injector's focus_moved). Handles only: never titles
/// or clipboard content.
fn log_paste_cancel(reason: &str, target: isize, child: isize) {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst) + 1;
    cvlog!("[cv] paste cancelled: {reason} #{n} target={target:x} child={child:x}");
}

/// The text that goes on the system clipboard for a text row. Oversized rows
/// (>256KB) keep the full text in a side file — the content column only ever
/// holds a 2000-char preview, so pasting the stored content would silently
/// truncate the user's paste. A missing/unreadable side file is a loud error.
fn oversized_text_payload(text_path: Option<&str>, stored_content: &str) -> Result<String, String> {
    match text_path {
        Some(p) => {
            let full = std::fs::read_to_string(p).unwrap_or_default();
            if full.is_empty() {
                Err("clip content file is gone".into())
            } else {
                Ok(full)
            }
        }
        None => Ok(stored_content.to_string()),
    }
}

#[cfg(test)]
mod paste_threading_tests {
    /// P0-deadlock-class regression guard. A SYNC `#[tauri::command]` runs
    /// inline on the WebView2 host thread; blocking there deadlocks every
    /// later IPC (the documented P0 "paste is dead" bug). paste_clip sleeps
    /// up to ~1.4s in focus-retry loops and must stay async so tauri spawns
    /// it on the IPC runtime thread (create_zoom precedent). Compile-only:
    /// `.await` below fails to compile the moment paste_clip is a plain
    /// `fn`. Deliberately NOT an `async fn` itself: the explicit
    /// `impl Future + Send` return type IS the Send-spawnability assertion
    /// under test (clippy::manual_async_fn allowed on purpose).
    #[allow(clippy::manual_async_fn)]
    fn paste_clip_future<'a>(
        app: tauri::AppHandle<tauri::Wry>,
        state: tauri::State<'a, crate::db::Db>,
        id: i64,
    ) -> impl std::future::Future<Output = Result<(), String>> + Send + 'a {
        async move { super::paste_clip(app, state, id).await }
    }

    #[test]
    fn paste_clip_is_an_async_command_running_off_the_main_thread() {
        // referencing the function forces the type check above
        let _probe = paste_clip_future;
    }
}

#[cfg(test)]
mod paste_payload_tests {
    /// Pasting a >256KB copy must put the FULL side-file text on the system
    /// clipboard — the old branch guarded on image_path (never set for text
    /// rows) so the 2000-char preview went out instead, silently truncating
    /// the user's paste while replacing their clipboard.
    #[test]
    fn oversized_text_pastes_the_full_side_file_not_the_preview() {
        let dir = std::env::temp_dir().join(format!(
            "cv-payload-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let full = format!("{}-TAIL", "A".repeat(300 * 1024));
        let side = dir.join("big.txt");
        std::fs::write(&side, &full).unwrap();
        let preview: String = full.chars().take(2000).collect();

        let got =
            super::oversized_text_payload(side.to_str(), &preview).expect("side file readable");
        assert_eq!(got, full, "paste payload must be the full side-file text");

        // gone file (deleted between listing and paste) → loud error, never
        // a silent truncated paste
        assert!(super::oversized_text_payload(
            Some(dir.join("gone.txt").to_str().unwrap()),
            &preview
        )
        .is_err());

        // inline rows carry their stored content
        assert_eq!(
            super::oversized_text_payload(None, "inline text").unwrap(),
            "inline text"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod paste_target_tests {
    use super::{should_capture_paste_target, validated_paste_target};

    #[test]
    fn missing_target_is_rejected_before_clipboard_mutation() {
        assert_eq!(
            validated_paste_target(0, 4242),
            Err("FOCUS_LOST".to_string())
        );
        // A nonzero HWND may already belong to a closed application.
        // Never overwrite the clipboard when the destination no longer exists.
        assert_eq!(
            validated_paste_target(1, 4242),
            Err("FOCUS_LOST".to_string())
        );
        // An unknown captured owner (0) fails closed even for a live window.
        let desktop = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow() };
        let live = desktop.0 as isize;
        assert_eq!(
            validated_paste_target(live, 0),
            Err("FOCUS_LOST".to_string())
        );
        // Identity is the (handle, owner pid) pair: a live window whose owner
        // differs from the captured pid is rejected.
        let pid = super::window_pid(live);
        assert_ne!(pid, 0);
        assert_eq!(validated_paste_target(live, pid), Ok(live));
        let other = if pid == u32::MAX { pid - 1 } else { pid + 1 };
        assert_eq!(
            validated_paste_target(live, other),
            Err("FOCUS_LOST".to_string())
        );
    }

    #[test]
    fn second_launch_keeps_original_target_when_clipvault_is_foreground() {
        assert!(!should_capture_paste_target(10, 10, 20)); // panel focused
        assert!(!should_capture_paste_target(20, 10, 20)); // zoom focused
        assert!(!should_capture_paste_target(0, 10, 20)); // no foreground
        assert!(should_capture_paste_target(30, 10, 20)); // external editor
    }
}

#[cfg(test)]
mod focus_handshake_tests {
    use super::handshake_child;
    use crate::win::FocusRestore;

    #[test]
    fn restored_proceeds_only_while_attribution_still_names_child() {
        // restore verified AND the live focus is still the verified child
        assert_eq!(handshake_child(FocusRestore::Restored, 9, Some(9)), Some(9));
        // self-focus stays strict: attribution must report the target itself
        assert_eq!(
            handshake_child(FocusRestore::Restored, 101, Some(101)),
            Some(101)
        );
    }

    #[test]
    fn restored_with_unknown_attribution_cancels() {
        // GPT round-4 regression: restore succeeded, but by handshake time
        // the OS focus attribution is unknown → cancel, never blind-fire
        assert_eq!(handshake_child(FocusRestore::Restored, 9, None), None);
    }

    #[test]
    fn restored_with_focus_moved_to_sibling_cancels() {
        // GPT round-4 regression: restore succeeded, but focus already
        // moved to a different live control → cancel
        assert_eq!(handshake_child(FocusRestore::Restored, 9, Some(77)), None);
    }

    #[test]
    fn no_captured_child_uses_top_level_only() {
        assert_eq!(
            handshake_child(FocusRestore::NoCapturedChild, 0, None),
            Some(0)
        );
    }

    #[test]
    fn destroyed_child_downgrades_only_on_verified_attribution() {
        // live focus provably inside the verified target → the documented
        // downgrade to top-level matching
        assert_eq!(
            handshake_child(FocusRestore::ChildDestroyed, 9, Some(9)),
            Some(0)
        );
        assert_eq!(
            handshake_child(FocusRestore::ChildDestroyed, 9, Some(101)),
            Some(0)
        );
        // attribution unknown → fail closed (never a loose pass)
        assert_eq!(handshake_child(FocusRestore::ChildDestroyed, 9, None), None);
    }

    #[test]
    fn identity_mismatch_and_restore_failure_never_loosen() {
        assert_eq!(
            handshake_child(FocusRestore::IdentityMismatch, 9, Some(9)),
            None
        );
        assert_eq!(
            handshake_child(FocusRestore::RestoreFailed, 9, Some(101)),
            None
        );
        assert_eq!(handshake_child(FocusRestore::RestoreFailed, 9, None), None);
    }
}

pub(crate) fn show_panel<R: tauri::Runtime>(app: &AppHandle<R>) {
    win::set_mouse_hook(true);
    if let Some(w) = app.get_webview_window("panel") {
        cvlog!("[cv] panel url: {:?}", w.url());
        let fg = win::get_foreground();
        let panel_hwnd = w.hwnd().map(|h| h.0 as isize).unwrap_or(0);
        // validated live: a stale/recycled zoom HWND must not suppress paste-
        // target capture for an unrelated window that reused the value
        let zoom_hwnd = win::live_zoom_hwnd(win::ZOOM_HWND.load(Ordering::SeqCst));
        if should_capture_paste_target(fg, panel_hwnd, zoom_hwnd) {
            win::PASTE_TARGET.store(fg, Ordering::SeqCst);
            win::PASTE_TARGET_PID.store(window_pid(fg) as usize, Ordering::SeqCst);
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

pub(crate) fn hide_panel<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
    }
    if let Some(z) = app.get_webview_window("zoom") {
        let _ = z.hide();
    }
    win::PANEL_VISIBLE.store(false, Ordering::SeqCst);
    win::PASTE_TARGET.store(0, Ordering::SeqCst);
    win::PASTE_TARGET_PID.store(0, Ordering::SeqCst);
    win::set_mouse_hook(false);
}

// NOTE (audit P2-6, deferred): WebView2 TrySuspend would reclaim ~30-60MB of
// hidden-panel memory. webview2-com-sys models TrySuspend as async with a
// COM event handler, so the wrapper needs a callback implementation — logged
// in the risk pool instead of shipping a handler just for this.

/// Lazily creates the zoom preview window with a fixed, code-defined config —
/// the frontend never needs the broader create-webview-window permission.
#[tauri::command]
pub(crate) async fn create_zoom<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
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
pub(crate) fn register_zoom<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
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
pub(crate) fn report_error<R: tauri::Runtime>(app: AppHandle<R>, msg: String) {
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
    // deliberately NOT logged: the 5s IPC ping once wrote a line per call and
    // rotated cv.log within ~12 hours, destroying real diagnostics (audit P2-3)
    // monotonic Instant: the watchdog's elapsed comparison is immune to
    // sleep/resume wall-clock jumps and NTP corrections
    *crate::LAST_BEAT.lock().unwrap_or_else(|p| p.into_inner()) = Some(std::time::Instant::now());
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
pub(crate) fn clear_history<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: tauri::State<db::Db>,
) -> Result<usize, String> {
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

/// async => tauri spawns this on the IPC runtime thread (same pattern as
/// create_zoom). It MUST NOT run on the main/WebView2-host thread: this body
/// sleeps up to ~1.4s in focus-retry loops, and a blocked host thread
/// deadlocks every later IPC (the P0 "paste is dead" bug class). The Win32
/// focus/injection sequence is legal from a worker thread — SetFocus rides
/// AttachThreadInput, and the actual keystroke goes to the injector process.
#[tauri::command]
pub(crate) async fn paste_clip<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, db::Db>,
    id: i64,
) -> Result<(), String> {
    let (kind, content, image_path, text_path) = {
        let conn = state.0.lock().unwrap_or_else(|p| p.into_inner());
        db::get_clip(&conn, id)
    }?;
    // No external paste destination: fail before changing the system clipboard.
    // A tray/second-instance re-show may have preserved a zero target; an
    // outdated target must also match the owner pid captured with it.
    let want_pid = win::PASTE_TARGET_PID.load(Ordering::SeqCst) as u32;
    let target = validated_paste_target(win::PASTE_TARGET.load(Ordering::SeqCst), want_pid)?;
    // UIPI: a keybd_event from our (medium-IL) process is silently dropped by
    // a higher-integrity foreground target — the paste would "succeed"
    // (clipboard overwritten, row marked used, panel closed) while nothing is
    // pasted. Refuse BEFORE touching the system clipboard, with a reason the
    // UI surfaces.
    if win::target_elevated(target) {
        cvlog!(
            "[cv] paste cancelled: target_elevated target={target:x} (UIPI blocks synthetic input)"
        );
        return Err("paste target is elevated — UIPI blocks the synthetic keystroke".into());
    }
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
        "text" => {
            // oversized rows store the full text in a side file; the content
            // column only holds a 2000-char preview. An empty/unreadable read
            // means the row's file was deleted between listing and paste
            // (e.g. by clear_history) — fail loudly instead of pasting nothing.
            let full = oversized_text_payload(
                text_path.as_deref(),
                content.as_deref().unwrap_or_default(),
            )?;
            win::write_clipboard_text(&full)
        }
        _ => win::write_clipboard_text(&content.unwrap_or_default()),
    };
    cvlog!("[cv] paste: kind={kind} write_ok={ok}");
    if !ok {
        return Err("clipboard write failed".into());
    }
    // The target was validated before writing; hide_panel clears the target
    // globals (the focus slot is consumed below, before anything restores).
    hide_panel(&app);
    // Take the captured child BEFORE any restore step: the slot is
    // single-consumer, and v0.1.3 read it after the restore had zeroed it,
    // so the handshake always carried 0 and child matching was silently
    // disabled (defect fixed under the GPT focus contract, 2026-10-24).
    let child = win::focus_hwnd_take();
    let mut restored = win::restore_foreground(target);
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
    let restore = win::restore_focus_verified(target, child);
    match restore {
        win::FocusRestore::Restored | win::FocusRestore::NoCapturedChild => {}
        win::FocusRestore::ChildDestroyed => log_paste_cancel("child_destroyed", target, child),
        win::FocusRestore::IdentityMismatch => {
            log_paste_cancel("child_identity_mismatch", target, child)
        }
        win::FocusRestore::RestoreFailed => log_paste_cancel("restore_failed", target, child),
    }
    let attribution = win::current_focus_attribution(target);
    if matches!(restore, win::FocusRestore::Restored) && attribution != Some(child) {
        // restore succeeded but live focus already moved again — cancel and
        // count it separately (contract: recovery-window state changes abort)
        log_paste_cancel("focus_moved_after_restore", target, child);
    }
    let Some(handshake_focus) = handshake_child(restore, child, attribution) else {
        // cancellation paths are logged above with distinct tokens; the
        // clipboard content was written but we refuse to blind-fire the
        // keystroke at an unverified destination
        return Err("FOCUS_LOST".into());
    };
    if matches!(restore, win::FocusRestore::ChildDestroyed) {
        cvlog!(
            "[cv] paste: child destroyed, attribution verified — top-level-only handshake focus={handshake_focus:x}"
        );
    }
    // the target app needs a beat to finish re-activating (WinForms/WPF
    // restore their control focus asynchronously); 30ms was too early
    std::thread::sleep(Duration::from_millis(20));
    // Re-verify the target identity immediately before issuing the
    // handshake: the window may have closed or its handle been recycled
    // while focus was being restored. Identity = the captured
    // (handle, owner pid) pair, same rule as paste entry.
    if validated_paste_target(target, want_pid).is_err() {
        log_paste_cancel("target_lost_before_handshake", target, child);
        return Err("FOCUS_LOST".into());
    }
    win::log_send_time();
    cvlog!("[cv] paste: sending ctrl+v handshake target={target:x} focus={handshake_focus:x}");
    if !win::request_paste_keystroke(target, handshake_focus) {
        return Err("paste keystroke could not be delivered (injector unavailable)".into());
    }
    db::mark_used(&state.0.lock().unwrap_or_else(|p| p.into_inner()), id);
    cvlog!("[cv] paste: sent");
    Ok(())
}

#[tauri::command]
pub(crate) fn show_panel_cmd<R: tauri::Runtime>(app: AppHandle<R>) {
    show_panel(&app);
}

#[tauri::command]
pub(crate) fn hide_panel_cmd<R: tauri::Runtime>(app: AppHandle<R>) {
    hide_panel(&app);
}
