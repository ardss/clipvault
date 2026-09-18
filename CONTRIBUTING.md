# Contributing

Thanks for your interest in ClipVault!

## How to help

- **Bug reports**: open an issue with the bug report template. Attach
  `%APPDATA%\com.clipvault.app\js-errors.log` when the panel misbehaves.
- **Feature ideas**: open an issue described as a user story ("As a … I want …
  so that …"). Features are prioritized by how often the situation happens.
- **Pull requests**: keep each PR one logical change; `cargo fmt`, `cargo
  clippy -- -D warnings` and `cargo test` must pass (CI enforces all three).

## Ground rules

- No network code. ClipVault is local-only by design; any PR that adds network
  access, telemetry or accounts will be rejected.
- English for code, comments and docs; Simplified Chinese is welcome in UI
  strings (the app is bilingual by design).
- MIT-licensed; by contributing you agree your work is released under it.
