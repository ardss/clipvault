> ⚠ **WARNING — ACCIDENTALLY PUBLISHED RELEASE (2026-10-26). DO NOT USE THIS BUILD.**
>
> v0.1.4 was published by accident during a release-ops incident. GitHub does not support converting a published release back to draft (verified 2026-10-27), so this release remains public with this warning instead of being re-drafted.
>
> **This build contains a P1 defect: PNG-only clipboard captures were silently dropped** (the "PNG" clipboard-format registration was missing its NUL terminator, so the PNG decode path never engaged). The candidate was superseded before proper release.
>
> → Please use **[v0.1.3](https://github.com/ardss/clipvault/releases/tag/v0.1.3)** (latest stable), or wait for **v0.1.5** — it contains the PNG fix plus every change listed below.

---
> ⚠ **SUPERSEDED — DO NOT PUBLISH (2026-10-26).** A P1 PNG-capture defect was confirmed in this build; v0.1.5 supersedes it. Kept as a draft for audit trail — do not publish.

> **Release candidate — not yet published.** Acceptance in progress; this draft will be published after all runtime acceptance gates pass. Latest stable release remains [v0.1.3](https://github.com/ardss/clipvault/releases/tag/v0.1.3).

## Fixed

- Focus identity is now verified end-to-end on the paste path: the captured child control handle is validated (alive, same thread, descendant of the target) before `SetFocus`, the actual focus is re-read after the restore, and the live focus attribution must still name the verified child when the injector handshake is written — focus that moved again cancels the paste instead of landing in the wrong control
- The paste destination (top-level HWND + owner PID) is re-validated immediately before the injector handshake is written, so a window that died or an HWND that was recycled during the restore round-trip can no longer take part in the injection
- v0.1.3 defect fixed: the injector handshake always carried focus = 0 (the captured child slot was read after the restore had cleared it), so strict child matching silently never engaged; the captured child is now consumed up-front and trusted only after verification
- Restore outcomes are separated into five states (restored / no captured child / child destroyed / identity mismatch / restore failed); only the documented fallback degrades to top-level matching, everything uncertain fails closed

## Changed

- Cancel reasons are counted with distinct `cv.log` tokens (`focus_moved`, `child_destroyed`, `child_identity_mismatch`, `restore_failed`, `focus_moved_after_restore`, `target_lost_before_handshake`) — handles only, never titles or clipboard content — so false-cancel rates are observable
- Safety wording stays bounded: common stale/recycled-handle cases are intercepted, but Win32 focus can still change after any check — the injector keeps its final focus match gate and no race-free claim is made

## Security & robustness audit (external review, all items verified then fixed)

- **Images from mainstream apps were silently lost (P1)**: copies written with the registered "PNG" clipboard format (Snipping Tool, browsers, Office) were fed to the DIB decoder, which misread the PNG signature as image geometry and dropped every such capture. PNG and DIB payloads now take separate, tested decode paths; a corrupt PNG falls back to the DIB the same write usually carries
- The DIB decoder refuses geometry claims the pixel payload cannot plausibly hold (a 20 MB payload declaring 1.6 GB no longer triggers a huge zeroed allocation)
- The single-instance plugin's internal event window is hidden at startup — it no longer leaks into Alt-Tab / Task View; second-instance delivery is unaffected
- Database dedup is keyed on a stable content hash instead of a full-text index that stored every ≤256 KB entry twice; existing databases migrate in place
- `cv.log` rotates to `cv.log.1` instead of deleting itself, and the 5-second heartbeat no longer writes a log line
- Stored images have a 512 MB total-size quota (oldest unpinned evicted) on top of the entry count limit
- Single-row delete and history pruning run with `secure_delete`, so deleted clipboard plaintext no longer survives in freed database pages
- Startup image reconcile scans the filesystem outside the database lock (two-phase); the low-level mouse hook is installed only while the panel is visible (self-healing) instead of once forever with the result discarded
- Clipboard capture runs on a dedicated worker (latest-wins) so a slow capture no longer delays the listener thread

## Known limitations

- Search matches the stored preview text; full-content search (FTS5) and WebView2 suspend are queued as future work (webview TrySuspend would reclaim ~30–60 MB of hidden-panel memory but needs a COM event-handler wrapper)
- Browser multi-tab input locations cannot be fully distinguished by HWND alone — the paste lands in the tab that was focused when the panel was summoned; verify focus before pasting in multi-tab browsers

## Install

- Windows 10/11 · `ClipVault_0.1.4_x64-setup.exe` (installer, auto-relaunches the app after update) or `ClipVault_0.1.4-portable.exe`
- Verify downloads against `SHA256SUMS.txt` — setup `f14245caf3eb8bd1b4e7b5484c132047c788421998dfe3cd6845ab7e41008ad6`
- All data stays in `%APPDATA%\com.clipvault.app` — upgrading keeps your history

