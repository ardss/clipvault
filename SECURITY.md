# Security Policy

## Reporting a vulnerability

Please use GitHub's private security advisories
([Security → Report a vulnerability](https://github.com/ardss/clipvault/security/advisories/new))
instead of opening a public issue. You can expect a response within 7 days.

## What ClipVault does with your data

- Clipboard content is stored **unencrypted** in `%APPDATA%\com.clipvault.app\`
  (SQLite database + PNG/text files). Anyone with access to your user account
  can read it. ClipVault is not a vault.
- Copies flagged as sensitive by password managers
  (`ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory = 0`)
  are **never recorded**.
- Copies containing user-defined sensitive keywords are never recorded.
- Capture can be paused with one click; the whole history can be wiped with
  one action (files removed from disk, not just the list).
- The app contains **no network code**: no account, no sync, no telemetry.

## Supported versions

Only the latest release receives security fixes.

## Scope notes

- The resident injector helper (`app.exe --injector`) exists because synthetic
  Ctrl+V from the app's own process is swallowed in some environments. It only
  waits on a same-user named event and sends a paste keystroke; some antivirus
  tools may flag this behavior.
