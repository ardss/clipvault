# ClipVault

[![Release](https://img.shields.io/github/v/release/ardss/clipvault)](../../releases)
[![CI](https://img.shields.io/github/actions/workflow/status/ardss/clipvault/ci.yml?label=CI)](../../actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/ardss/clipvault)](LICENSE)

**English** | [简体中文](README.zh-CN.md)

The enhanced Win+V — a clean, minimal Windows clipboard history manager.

Press `Alt+V` to summon the panel → search → click to paste back where you came from. Fully local: zero network, zero account, zero telemetry.

## Features

- **Long-lived history**: text, images, files, rich text (web/Word formats), links — survives reboot, with configurable retention
- **Search**: instant keyword filtering (Chinese and English)
- **Pin**: keep frequently used entries on top; exempt from cleanup
- **Filters**: all / text / images / links / files / pinned
- **Paste back**: the panel never steals focus from your work; click an entry and it lands where you were
- **Hover preview**: truncated text and image thumbnails expand in a side preview (scrollable, selectable, partially copyable)
- **Keyboard-first**: `↑↓` navigate, `Enter` paste, `Del` delete, `Esc` close
- **Dark & light theme, Chinese & English UI**: switch inside the panel, remembered
- **Stats**: totals / categories / 7-day trend / top 5 pasted
- **Optional autostart**, tray icon, draggable panel height that sticks
- **Pause capture** and **wipe all history** with one click each
- **Configurable summon hotkey** (conflict-safe re-registration)
- **Oversized text** (>256 KB) is offloaded to a local file with an inline preview — nothing is silently dropped

## Privacy

- 100% local. Data lives in `%APPDATA%\com.clipvault.app\` (SQLite WAL + PNG files)
- Password-manager copies are never recorded (sensitive clipboard markers are honored)
- User-defined sensitive keyword filter
- No network code in the binary. Details in [SECURITY.md](SECURITY.md)

## Install

Grab an installer from [Releases](../../releases), or build from source:

```bash
npm install
npx tauri build        # output in src-tauri/target/release/bundle/
```

## Building

Tauri 2 + Vue 3 + Rust (rusqlite / hand-written Win32 layer).

> **Note:** Always build releases with `npx tauri build`. A bare `cargo build --release`
> leaves the dev configuration on and the window will try to load the UI from
> the vite dev server (see [docs/postmortem-2026-09-11.md](docs/postmortem-2026-09-11.md)).

Development mode:

```bash
npx vite --port 5173        # terminal 1
npx tauri dev               # terminal 2
```

Tests: `cd src-tauri && cargo test` (clipboard round-trip / DIB decoding / DROPFILES parsing)

## Testing & Acceptance

See [docs/TESTING.md](docs/TESTING.md) and [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md). (测试与验收规范见 [docs/TESTING.md](docs/TESTING.md) 与 [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md)。)

## Technical notes

## Technical notes

- Clipboard write compatibility: images are offered both as standard CF_DIB
  (bottom-up) and the registered "PNG" format — Chromium-based apps
  (Chrome/VS Code/Electron) and GDI apps both read them correctly; DIB
  decoding supports BI_BITFIELDS channel masks (screenshot tools)
- The paste keystroke is issued by an on-demand injector subprocess (self-healing) (in-process
  synthetic keys are swallowed on some setups)
- WebView IPC heartbeat watchdog: the panel detects a dead IPC channel and
  reloads itself (JS check every 5 s, native fallback within 60 s)
- All data lives in `%APPDATA%\com.clipvault.app\`

## License

[MIT](LICENSE)
