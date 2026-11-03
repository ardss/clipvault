//! L3 IPC contract tests (TESTING_ARCHITECTURE_PLAN §3.3): invoke all 16
//! registered commands on a tauri mock runtime (`app_lib::ipc_test_app`).
//!
//! Side-effect gating rules (plan §3.3, "无桌面环境归属与副作用门控"):
//! - Pure/state-only commands (db-backed, settings) get real invoke + value
//!   assertions. The Db is an in-memory copy of the production schema —
//!   mutating it is safe everywhere.
//! - System-side-effect commands (paste_clip → injector/SendInput,
//!   show_panel_cmd → real WH_MOUSE_LL hook, ...) are only tested through
//!   their validation / error branches, which return BEFORE the side effect;
//!   the side-effect paths belong to the L3 probe layer behind the
//!   CV_E2E_ISOLATED gate.
//! - Where even the validation branch cannot be reached without the side
//!   effect, the command degrades to a type/signature probe with a comment.
//!
//! Design constraint: this target must be fully green on a GH windows runner
//! with NO interactive desktop — it is pulled in by ci.yml's bare
//! `cargo test` (plan §3.3 acceptance item).

use app_lib::ipc_test_app;
use serde_json::json;
use tauri::test::{get_ipc_response, MockRuntime};
use tauri::webview::InvokeRequest;
use tauri::WebviewWindow;

/// One "main" webview per app instance — tauri enforces unique labels, and
/// every helper below routes its invokes through this webview's on_message.
fn webview(app: &tauri::App<MockRuntime>) -> WebviewWindow<MockRuntime> {
    tauri::WebviewWindowBuilder::new(app, "main", Default::default())
        .build()
        .expect("mock webview")
}

fn invoke(
    webview: &WebviewWindow<MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> Result<tauri::ipc::InvokeResponseBody, serde_json::Value> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            // windows runner / mock runtime resolves the app origin here
            url: "http://tauri.localhost".parse().unwrap(),
            body: body.into(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
}

fn invoke_ok(
    webview: &WebviewWindow<MockRuntime>,
    cmd: &str,
    body: serde_json::Value,
) -> tauri::ipc::InvokeResponseBody {
    invoke(webview, cmd, body).unwrap_or_else(|e| panic!("{cmd} must succeed, got error: {e:?}"))
}

fn invoke_err(webview: &WebviewWindow<MockRuntime>, cmd: &str, body: serde_json::Value) -> String {
    match invoke(webview, cmd, body) {
        Ok(b) => panic!("{cmd} must return Err, got Ok({b:?})"),
        Err(e) => e.to_string(),
    }
}

fn get_json(webview: &WebviewWindow<MockRuntime>, cmd: &str) -> serde_json::Value {
    invoke_ok(webview, cmd, serde_json::Value::Null)
        .deserialize::<serde_json::Value>()
        .expect("response deserializes to JSON")
}

fn seed_text(app: &tauri::App<MockRuntime>, id: i64, content: &str) {
    app_lib::ipc_test_db(app)
        .execute(
            "INSERT INTO clips(id, kind, content, preview, created_at) VALUES(?1,'text',?2,?3,1)",
            rusqlite::params![id, content, &content[..content.len().min(20)]],
        )
        .expect("seed row");
}

#[test]
fn clip_content_returns_seeded_text() {
    let app = ipc_test_app();
    seed_text(&app, 7, "hello contract");
    let w = webview(&app);
    let got: String = invoke_ok(&w, "clip_content", json!({ "id": 7 }))
        .deserialize::<String>()
        .expect("clip_content yields a String");
    assert_eq!(got, "hello contract");
}

#[test]
fn clip_content_unknown_id_is_a_loud_err() {
    let app = ipc_test_app();
    let w = webview(&app);
    invoke_err(&w, "clip_content", json!({ "id": 424242 }));
}

#[test]
fn list_clips_returns_seeded_rows() {
    let app = ipc_test_app();
    seed_text(&app, 1, "alpha row");
    seed_text(&app, 2, "beta row");
    let w = webview(&app);
    let clips: Vec<serde_json::Value> =
        invoke_ok(&w, "list_clips", json!({ "filter": "all", "query": "" }))
            .deserialize::<Vec<serde_json::Value>>()
            .expect("list_clips deserializes to Vec<Clip>");
    assert_eq!(clips.len(), 2, "both seeded rows must be listed");
    let ids: Vec<i64> = clips
        .iter()
        .map(|c| c["id"].as_i64().expect("clip id"))
        .collect();
    assert_eq!(ids, vec![2, 1], "newest first");
}

#[test]
fn stats_reflects_seeded_rows() {
    let app = ipc_test_app();
    seed_text(&app, 1, "counted once");
    let w = webview(&app);
    let stats = get_json(&w, "stats");
    assert_eq!(stats["total"].as_i64(), Some(1));
    assert_eq!(stats["text"].as_i64(), Some(1));
    assert_eq!(stats["image"].as_i64(), Some(0));
}

#[test]
fn toggle_pin_flips_pinned_flag() {
    let app = ipc_test_app();
    seed_text(&app, 5, "pin me");
    let w = webview(&app);
    invoke_ok(&w, "toggle_pin", json!({ "id": 5 }));
    // unknown id → idempotent Ok (the DELETE/UPDATE affects 0 rows and the
    // command does not treat that as an error); the pin flag of a seeded row
    // is the value contract asserted below
    invoke_ok(&w, "toggle_pin", json!({ "id": 9999 }));
    let pinned: i64 = app_lib::ipc_test_db(&app)
        .query_row("SELECT pinned FROM clips WHERE id=5", [], |r| r.get(0))
        .unwrap();
    assert_eq!(pinned, 1, "toggle_pin must flip the flag");
}

#[test]
fn delete_clip_removes_the_row() {
    let app = ipc_test_app();
    seed_text(&app, 6, "delete me");
    let w = webview(&app);
    invoke_ok(&w, "delete_clip", json!({ "id": 6 }));
    // deleting an already-gone id is an idempotent Ok (0 rows affected)
    invoke_ok(&w, "delete_clip", json!({ "id": 6 }));
    let n: i64 = app_lib::ipc_test_db(&app)
        .query_row("SELECT count(*) FROM clips WHERE id=6", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "row must be gone");
}

#[test]
fn clear_history_wipes_rows_and_reports_count() {
    let app = ipc_test_app();
    seed_text(&app, 1, "one");
    seed_text(&app, 2, "two");
    let w = webview(&app);
    let removed: usize = invoke_ok(&w, "clear_history", serde_json::Value::Null)
        .deserialize::<usize>()
        .expect("clear_history yields a count");
    assert_eq!(removed, 2, "must report how many rows it removed");
    let n: i64 = app_lib::ipc_test_db(&app)
        .query_row("SELECT count(*) FROM clips", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn heartbeat_responds_without_side_effects() {
    let app = ipc_test_app();
    let w = webview(&app);
    // returns () — contract = a successful round trip, no error branch
    invoke_ok(&w, "heartbeat", serde_json::Value::Null);
}

#[test]
fn report_error_accepts_any_string() {
    let app = ipc_test_app();
    let w = webview(&app);
    // returns () — the privacy contract (only event classes persisted, never
    // raw text) is covered by the unit tests in commands.rs; here the IPC
    // round trip must simply succeed. The mock app has no app_data_dir, and
    // report_error tolerates that (if-let path).
    invoke_ok(
        &w,
        "report_error",
        json!({ "msg": "password=secret should never be persisted raw" }),
    );
}

#[test]
fn get_settings_returns_defaults() {
    let app = ipc_test_app();
    let w = webview(&app);
    let s = get_json(&w, "get_settings");
    assert_eq!(s["history_limit"].as_i64(), Some(1000));
    assert_eq!(s["panel_height"].as_f64(), Some(540.0));
    assert_eq!(s["hotkey"].as_str(), Some("Alt+V"));
}

#[test]
fn set_settings_rejects_an_unparseable_hotkey() {
    // SIDE-EFFECT GATING: a VALID hotkey registers a real OS global hotkey
    // (RegisterHotKey) — never in CI. register_hotkey_now validates the combo
    // BEFORE any register call, and set_settings keeps previous settings on
    // rejection; that validation branch is what this asserts. The mock
    // runtime's in-memory shortcut map makes the plugin itself desktop-free.
    let app = ipc_test_app();
    let w = webview(&app);
    let err = invoke_err(
        &w,
        "set_settings",
        json!({ "settings": {
            "history_limit": 1000, "panel_height": 540.0, "autostart": false,
            "hotkey": "Not A Valid Combo!!", "sensitive_keywords": [], "paused": false
        }}),
    );
    assert!(
        err.contains("unparseable hotkey"),
        "must fail on hotkey validation, got: {err}"
    );
    let s = get_json(&w, "get_settings");
    assert_eq!(
        s["hotkey"].as_str(),
        Some("Alt+V"),
        "rejected save keeps the old hotkey"
    );
}

#[test]
fn save_panel_height_clamps_and_updates_state() {
    // Side effect check: save_panel_height only clamps, mutates managed state
    // and writes settings.json under app_data_dir — on the mock app the path
    // resolver returns Err and the disk write is silently skipped (its code
    // tolerates that); the registry/autostart path is NOT touched here.
    let app = ipc_test_app();
    let w = webview(&app);
    invoke_ok(&w, "save_panel_height", json!({ "h": 760.0 }));
    let s = get_json(&w, "get_settings");
    assert_eq!(
        s["panel_height"].as_f64(),
        Some(760.0),
        "height must round-trip"
    );
}

#[test]
fn paste_clip_unknown_id_fails_before_any_system_side_effect() {
    // SIDE-EFFECT GATING: paste_clip's happy path writes the real system
    // clipboard, restores foreground focus and injects Ctrl+V — never in CI.
    // With an unknown id the command returns Err at the FIRST step
    // (db::get_clip) before validated_paste_target / clipboard write /
    // injector handshake; that error branch is the contract asserted here.
    // The paste chain itself is proven by the L4 paste e2e behind
    // CV_E2E_ISOLATED (plan §3.4).
    let app = ipc_test_app();
    let w = webview(&app);
    let err = invoke_err(&w, "paste_clip", json!({ "id": 31337 }));
    assert!(
        !err.is_empty(),
        "unknown clip id must fail loudly before touching the clipboard"
    );
}

#[test]
fn register_zoom_is_inert_without_a_zoom_window() {
    // SIDE-EFFECT GATING: the command stores the zoom window's HWND into the
    // global slot; on the mock app there is no "zoom" window, so the invoke
    // is a verified no-op round trip (returns ()). The real HWND path belongs
    // to the probe/e2e layer.
    let app = ipc_test_app();
    let w = webview(&app);
    invoke_ok(&w, "register_zoom", serde_json::Value::Null);
}

#[test]
fn create_zoom_on_mock_runtime_completes_the_round_trip() {
    // SIDE-EFFECT GATING: create_zoom's real job is creating a WebView2
    // window — on the mock runtime the WebviewWindowBuilder runs entirely
    // in-process (tauri::test's own doctests build windows this way), so this
    // invoke stays desktop-free on CI. The real-window behavior is covered by
    // the L4 layer, not here.
    let app = ipc_test_app();
    let w = webview(&app);
    invoke_ok(&w, "create_zoom", serde_json::Value::Null);
}

#[test]
fn hide_panel_cmd_round_trips_without_windows() {
    // SIDE-EFFECT GATING: hide_panel_cmd with no panel/zoom windows on the
    // mock app only clears process-global flags and toggles the mouse hook
    // desired-state from false → false (set_mouse_hook early-returns without
    // spawning the hook thread — win/mod.rs:281). No system input is touched.
    let app = ipc_test_app();
    let w = webview(&app);
    invoke_ok(&w, "hide_panel_cmd", serde_json::Value::Null);
}

#[test]
fn show_panel_cmd_is_a_signature_probe() {
    // SIDE-EFFECT GATING (plan §3.3 rule ③): show_panel_cmd takes no
    // arguments, has no validation/error branch, and its ONLY behavior is the
    // panel choreography — which starts with set_mouse_hook(true): a REAL
    // system-wide WH_MOUSE_LL hook install (win/mod.rs:281). Invoking it in a
    // test would hook mouse input on whatever machine runs the suite —
    // exactly the side effect the gating rule forbids. Degraded to a
    // type/signature probe; the show path is proven on real machines by the
    // probe/e2e layer behind CV_E2E_ISOLATED.
    let show_panel_cmd: fn(tauri::AppHandle<tauri::Wry>) = app_lib::__probe_show_panel_cmd_sig;
    let _ = show_panel_cmd; // compile-time: the registered signature is intact
}

// ------------------------------------------------- 0.2 additions (drag-out + grouping) ---

#[test]
fn set_auto_hide_suppressed_round_trips() {
    let app = ipc_test_app();
    let w = webview(&app);
    // pure atomic flip, no system side effects
    invoke_ok(&w, "set_auto_hide_suppressed", json!({ "on": true }));
    invoke_ok(&w, "set_auto_hide_suppressed", json!({ "on": false }));
}

#[test]
fn set_panel_pinned_round_trips() {
    let app = ipc_test_app();
    let w = webview(&app);
    invoke_ok(&w, "set_panel_pinned", json!({ "on": true }));
    invoke_ok(&w, "set_panel_pinned", json!({ "on": false }));
}

#[test]
fn start_drag_out_unknown_id_fails_before_any_system_side_effect() {
    // SIDE-EFFECT GATING: the happy path runs a real OLE DoDragDrop session —
    // never in CI. With an unknown id the command returns Err at the FIRST
    // step (db::get_clip) before any drag thread starts; that error branch is
    // the contract asserted here. The full drag chain is proven by the L4
    // probe layer behind CV_E2E_ISOLATED.
    let app = ipc_test_app();
    let w = webview(&app);
    let err = invoke_err(&w, "start_drag_out", json!({ "id": 424242 }));
    assert!(
        !err.is_empty(),
        "unknown clip id must fail loudly before touching OLE"
    );
}

#[test]
fn merge_split_ungroup_round_trip_preserves_all_rows() {
    let app = ipc_test_app();
    seed_text(&app, 10, "alpha");
    seed_text(&app, 11, "beta");
    seed_text(&app, 12, "gamma");
    let w = webview(&app);
    // merge 10 into 11 → one group of 2
    invoke_ok(&w, "merge_clips", json!({ "sourceId": 10, "targetId": 11 }));
    {
        let db = app_lib::ipc_test_db(&app);
        let (g11, g10): (Option<i64>, Option<i64>) = {
            let mut stmt = db
                .prepare("SELECT group_id FROM clips WHERE id=?1")
                .unwrap();
            (
                stmt.query_row([11], |r| r.get(0)).unwrap(),
                stmt.query_row([10], |r| r.get(0)).unwrap(),
            )
        };
        assert_eq!(g10, g11, "both clips share one group");
        assert!(g11.is_some());
    }
    // split 10 out → group auto-dissolves (1 member left)
    invoke_ok(&w, "split_clip", json!({ "id": 10 }));
    {
        let db = app_lib::ipc_test_db(&app);
        let members: i64 = db
            .query_row(
                "SELECT count(*) FROM clips WHERE group_id IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let groups: i64 = db
            .query_row("SELECT count(*) FROM clip_groups", [], |r| r.get(0))
            .unwrap();
        assert_eq!((members, groups), (0, 0), "auto-dissolve after split");
        let n: i64 = db
            .query_row("SELECT count(*) FROM clips", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 3, "纯逻辑分组: no clip row is ever deleted");
    }
    // rebuild the group and ungroup explicitly
    invoke_ok(&w, "merge_clips", json!({ "sourceId": 10, "targetId": 11 }));
    invoke_ok(&w, "merge_clips", json!({ "sourceId": 12, "targetId": 11 }));
    let g: i64 = {
        let db = app_lib::ipc_test_db(&app);
        db.query_row("SELECT group_id FROM clips WHERE id=11", [], |r| r.get(0))
            .unwrap()
    }; // guard dropped BEFORE the invoke: ungroup takes the same db mutex
    invoke_ok(&w, "ungroup", json!({ "groupId": g }));
    {
        let db = app_lib::ipc_test_db(&app);
        let groups: i64 = db
            .query_row("SELECT count(*) FROM clip_groups", [], |r| r.get(0))
            .unwrap();
        let n: i64 = db
            .query_row("SELECT count(*) FROM clips", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            (groups, n),
            (0, 3),
            "ungroup deletes the group, keeps every clip"
        );
    }
}

#[test]
fn merge_into_missing_target_is_a_loud_err() {
    let app = ipc_test_app();
    seed_text(&app, 20, "solo");
    let w = webview(&app);
    let err = invoke_err(
        &w,
        "merge_clips",
        json!({ "sourceId": 20, "targetId": 999 }),
    );
    assert!(
        err.contains("target clip 999 missing"),
        "must fail loudly on a missing target, got: {err}"
    );
}

#[test]
fn reorder_groups_and_clips_round_trip() {
    let app = ipc_test_app();
    seed_text(&app, 30, "a30");
    seed_text(&app, 31, "a31");
    seed_text(&app, 32, "a32");
    let w = webview(&app);
    invoke_ok(&w, "merge_clips", json!({ "sourceId": 30, "targetId": 31 }));
    let g: i64 = app_lib::ipc_test_db(&app)
        .query_row("SELECT group_id FROM clips WHERE id=31", [], |r| r.get(0))
        .unwrap();
    invoke_ok(
        &w,
        "reorder_clips",
        json!({ "groupId": g, "ids": [31, 30] }),
    );
    invoke_ok(&w, "reorder_groups", json!({ "ids": [g] }));
    let (o31, o30): (i64, i64) = {
        let db = app_lib::ipc_test_db(&app);
        let o31 = db
            .query_row("SELECT group_order FROM clips WHERE id=31", [], |r| {
                r.get(0)
            })
            .unwrap();
        let o30 = db
            .query_row("SELECT group_order FROM clips WHERE id=30", [], |r| {
                r.get(0)
            })
            .unwrap();
        (o31, o30)
    };
    assert!(o31 < o30, "reorder_clips must apply the requested order");
}
