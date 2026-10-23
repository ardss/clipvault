# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.3] - 2026-10-23

### Fixed

- Pasting no longer touches the system clipboard when the original target window is gone — the paste is cancelled with `FOCUS_LOST` before any clipboard write (`IsWindow` + owner-PID identity check; this catches the common closed/recycled-target cases, though no Win32 check is fully race-free)
- Pasting is also cancelled when the foreground window stops matching the captured destination at inject time — the injector now aborts instead of sending the keystroke into whatever is focused
- A failed injector handshake write now refuses to signal the injector — previously the write result was ignored, so a stale handshake from the previous paste could redirect the keystroke
- A second launch while the panel already owns focus no longer replaces the captured paste destination with ClipVault's own window; second-instance now reuses the same show path as hotkey/tray, keeping `PASTE_TARGET` and panel visibility consistent
- Dismissing the settings panel (or switching to statistics) now cancels an armed clear-history confirmation — a later unrelated click can no longer wipe history
- JS error reporting logs only the event class, never message contents (clipboard text could leak into thrown error strings and source URLs)
- Failure-injection example restored after an editor corrupted its file encoding (stray BOM + mojibake)
- Listener window class name in `win/mod.rs` now uses a `\0` escape instead of a raw NUL byte that made tooling treat the source file as binary

### Documentation

- CHANGELOG repaired: the `[Unreleased]` header had drifted below 0.1.1 and the 0.1.2 section had lost its heading; both restored
- README: removed a duplicated "Technical notes" heading

## [0.1.2] - 2026-10-01

### Added

- History, settings and window robustness: hotkey fallback re-registration (never left without a summon shortcut), limit/height clamps with corrupt-settings backup, monotonic timestamps (clock-rollback safe), listener-window retry instead of panic
- Corrupt-db recovery tested by failure injection (kill-during-write, truncation, byte flips, WAL torture)
- Installer auto-launches the app after install (NSIS POSTINSTALL hook) — updating becomes run-installer-and-done, the app returns to the tray by itself

### Changed

- Dedup lookups use partial indexes — O(n) table scan per capture becomes O(log n); a guard test asserts index usage
- Clipboard snapshot and image decoding happen outside the DB lock — the panel no longer freezes while other apps hold the clipboard
- `PRAGMA synchronous=NORMAL` (WAL-safe): removes reader starvation under concurrent writes
- List query caps inline content at 4,096 characters (refresh payload bounded; the zoom window lazily fetches the full text)
- settings.json is written atomically (temp file + rename) with all writers serialized
- Startup reordered: tray and hotkey register before DB migrations; idle wakeups halved (merged poller thread); injector spawns on demand
- Sensitive-content confirm poll reduced from 360 ms to 120 ms per capture; clear-history zeroes deleted pages (`secure_delete` + WAL checkpoint + VACUUM)
- Release profile: codegen-units=1; landing-page images re-encoded (−2.1 MB)

## [0.1.1] - 2026-09-18

### Added

- Pause capture and clear-all-history controls in settings (both with explicit confirmation for destructive actions; pause is session-only and always resets on relaunch)
- Lazy zoom preview window — created on first hover instead of at boot (~50 MB idle memory saved)
- Corrupt-database quarantine: an unusable clips.db is backed up as clips.db.corrupt-<ts> and recreated instead of blocking startup
- Configurable summon hotkey, sensitive-keyword filter, thumbnail & retention cleanup
- Oversized text (>256 KB) is stored as a local side file with a 2,000-character preview — nothing is silently dropped

### Fixed

- Clipboard handle left open when `GetClipboardData` failed, which could stall clipboard capture until restart
- Deleting a clip now removes its thumbnail and oversized-text side files from disk
- The sensitive-keyword filter now also covers oversized text (previously bypassed)
- Pasting rich text no longer records its own output as a new clip (clipboard sequence-number based self-write detection)
- The resident paste injector self-heals: stale event handles are reopened and a dead injector is respawned; paste failures surface as errors instead of silently doing nothing

## [0.1.0] - 2026-09-13

### Added

- First public release: clipboard history for text, images, files, rich text and links, with deduplication
- Search, pinning, categories, keyboard navigation, hover preview, stats panel
- Paste-back into the original window, dark/light theme, Chinese/English UI, tray, optional autostart
- Password-manager sensitive copies are auto-skipped; panel self-healing after display sleep
