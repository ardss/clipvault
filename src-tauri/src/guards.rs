//! L2 guard tests (docs/TESTING_ARCHITECTURE_PLAN.md §3.2): source-pattern
//! scans that fail the build when a known incident pattern is reintroduced.
//! Same proven pattern as win/mod.rs `hook_pump_must_not_sleep`:
//! include_str! + region scan + contains. Positioning (per plan §1.3-1):
//! these are *floor* guards, not proofs — equivalent spellings can evade the
//! string match; behavioral properties belong to the L3 probes.

#[cfg(test)]
mod guard_tests {
    const WIN_MOD: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/win/mod.rs"));
    const COMMANDS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/commands.rs"));
    const ROOT_CARGO: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"));
    const CRATE_CARGO: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));

    /// TESTING.md §3-2: the low-level hook callback must never do file I/O —
    /// the system holds every mouse event until it returns, so any I/O there
    /// stalls the whole system's input queue. Scans the `mouse_proc` callback
    /// body only (delimited by the next top-level item, `MOUSE_HOOK_DESIRED`).
    #[test]
    fn mouse_proc_must_not_do_file_io() {
        let start = WIN_MOD
            .find("fn mouse_proc")
            .expect("mouse_proc must exist in win/mod.rs");
        let end = WIN_MOD[start..]
            .find("static MOUSE_HOOK_DESIRED")
            .expect("MOUSE_HOOK_DESIRED must follow mouse_proc")
            + start;
        let body = &WIN_MOD[start..end];
        for banned in [
            "CreateFile",
            "OpenFile",
            "ReadFile",
            "WriteFile",
            "std::fs",
            "File::",
            "OpenOptions",
        ] {
            assert!(
                !body.contains(banned),
                "mouse_proc (LL hook callback) must not do file I/O: found {banned:?} \
                 in the callback body. TESTING.md §3-2 hard rule — the system holds \
                 all mouse input while the callback runs."
            );
        }
    }

    /// TESTING.md §3-3: a synchronous tauri command must not create windows or
    /// block waiting on UI — sync window creation on the main thread once
    /// deadlocked the whole app (P0). Window creation is allowed only from
    /// async/event-driven paths (e.g. the async `create_zoom` is fine). Scans
    /// the whole `#[tauri::command]` region of commands.rs for raw Win32
    /// window creation and blocking-wait tokens.
    #[test]
    fn sync_command_must_not_create_window() {
        let start = COMMANDS
            .find("#[tauri::command]")
            .expect("commands.rs must contain #[tauri::command]");
        let body = &COMMANDS[start..];
        for banned in [
            "CreateWindow",
            "WaitForSingleObject",
            "MsgWaitForMultipleObjects",
        ] {
            assert!(
                !body.contains(banned),
                "tauri command region must not contain {banned:?}: sync window \
                 creation / blocking wait inside a command deadlocks the main \
                 thread (P0 incident, TESTING.md §3-3). Use async commands."
            );
        }
    }

    /// Incident of 0.1.6: `[profile.release]` declared in a non-root workspace
    /// member is silently ignored by cargo — opt-level/lto/strip vanish from
    /// release builds with only a warning. Root Cargo.toml:7-8 carries the
    /// comment locking the rule; this guard machine-checks it.
    #[test]
    fn profile_release_only_in_root() {
        // The existing NOTE comment in src-tauri/Cargo.toml mentions
        // "[profile.release]" by name — only real TOML table headers count,
        // so comment lines are stripped before scanning.
        let code = CRATE_CARGO
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !code.contains("[profile."),
            "[profile] found in src-tauri/Cargo.toml — cargo ignores profiles in \
             a non-root member, silently dropping opt-level/lto/strip from release \
             builds. [profile.release] must live only at the workspace root \
             Cargo.toml (0.1.6 incident)."
        );
        let start = ROOT_CARGO
            .find("[profile.release]")
            .expect("[profile.release] missing from root Cargo.toml");
        let tail = &ROOT_CARGO[start..];
        assert!(
            tail.contains("lto"),
            "root [profile.release] must keep the lto setting (0.1.6 incident)"
        );
        assert!(
            tail.contains("strip"),
            "root [profile.release] must keep the strip setting (0.1.6 incident)"
        );
    }
}
