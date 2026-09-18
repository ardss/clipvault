# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Pause capture and clear-all-history controls in settings (both with explicit confirmation for destructive actions)
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
