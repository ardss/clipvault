# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.4] - 2026-10-24

### Fixed

- Focus identity is now verified end-to-end on the paste path: the captured child control handle is validated (alive, same thread, descendant of the target) before `SetFocus`, the actual focus is re-read after the restore, and the live focus attribution must still name the verified child when the injector handshake is written — focus that moved again cancels the paste instead of landing in the wrong control
- The paste destination (top-level HWND + owner PID) is re-validated immediately before the injector handshake is written, so a window that died or an HWND that was recycled during the restore round-trip can no longer take part in the injection
- v0.1.3 defect fixed: the injector handshake always carried focus = 0 (the captured child slot was read after the restore had cleared it), so strict child matching silently never engaged; the captured child is now consumed up-front and trusted only after verification
- Restore outcomes are separated into five states (restored / no captured child / child destroyed / identity mismatch / restore failed); only the documented fallback degrades to top-level matching, everything uncertain fails closed

### Changed

- Cancel reasons are counted with distinct `cv.log` tokens (`focus_moved`, `child_destroyed`, `child_identity_mismatch`, `restore_failed`, `focus_moved_after_restore`, `target_lost_before_handshake`) — handles only, never titles or clipboard content — so false-cancel rates are observable
- Safety wording stays bounded: common stale/recycled-handle cases are intercepted, but Win32 focus can still change after any check — the injector keeps its final focus match gate and no race-free claim is made

### Security & robustness audit (external review, all items verified then fixed)

- **Images from mainstream apps were silently lost (P1)**: copies written with the registered "PNG" clipboard format (Snipping Tool, browsers, Office) were fed to the DIB decoder, which read the PNG signature as image geometry and dropped every such capture. PNG and DIB payloads now take separate, tested decode paths, and a corrupt PNG falls back to the DIB the same write usually carries
- The DIB decoder refuses geometry claims that the pixel payload cannot plausibly hold (a 20MB payload declaring 1.6GB no longer triggers a huge zeroed allocation)
- The single-instance plugin's internal event window (`*-siw`) is hidden at startup — it used to leak into Alt-Tab / Task View; second-instance delivery (by class name) is unaffected
- Database dedup is keyed on a stable content hash instead of a full-text index that stored every ≤256KB entry twice; existing databases are migrated in place (hash column + backfill, old index dropped)
- `cv.log` rotates to `cv.log.1` instead of deleting itself, and the 5-second IPC heartbeat no longer writes a log line — diagnostics now survive more than ~12 hours
- Stored images have a total-size quota (512 MB, oldest unpinned evicted) on top of the entry count limit
- Single-row delete and history pruning run with `secure_delete`, so deleted clipboard plaintext no longer survives in freed database pages
- The startup image reconcile scans the filesystem outside the database lock (two-phase); the low-level mouse hook is installed only while the panel is visible (self-healing on each show) instead of being installed once forever with the result discarded
- Clipboard capture runs on a dedicated worker (latest-wins) instead of inside the listener thread, so a slow capture (settle sleep, image decode) no longer delays the message pump

### Known limitations

- Search matches the stored preview text; full-content search (FTS5) and WebView2 suspend are queued as future work (webview TrySuspend would reclaim ~30–60 MB of hidden-panel memory but needs a COM event-handler wrapper)

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
