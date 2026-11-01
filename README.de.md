# ClipVault

[![Release](https://img.shields.io/github/v/release/ardss/clipvault)](../../releases)
[![CI](https://img.shields.io/github/actions/workflow/status/ardss/clipvault/ci.yml?label=CI)](../../actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/ardss/clipvault)](LICENSE)

**Deutsch** | [English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Français](README.fr.md)

Das verbesserte Win+V — ein aufgeräumter, minimalistischer Windows-Zwischenablage-Verlauf.

`Alt+V` drücken, um das Panel aufzurufen → suchen → klicken, um dort einzufügen, wo du warst. Vollständig lokal: kein Netzwerk, kein Konto, keine Telemetrie.

## Funktionen

- **Langlebiger Verlauf**: Text, Bilder, Dateien, Rich-Text (Web/Word-Formate), Links — überlebt Neustarts, Aufbewahrung konfigurierbar
- **Suche**: sofortige Stichwortfilterung (Chinesisch und Englisch)
- **Anheften**: häufig genutzte Einträge bleiben oben; von der Aufräumung ausgenommen
- **Filter**: alle / Text / Bilder / Links / Dateien / angeheftet
- **Zurück-einfügen**: das Panel klaut nie den Fokus deiner Arbeit; ein Klick und es landet, wo du warst
- **Hover-Vorschau**: abgeschnittener Text und Bildvorschauen expandieren in einer Seitenvorschau (scrollbar, markierbar, teilweise kopierbar)
- **Tastatur zuerst**: `↑↓` navigieren, `Enter` einfügen, `Entf` löschen, `Esc` schließen
- **Dunkles & helles Design, UI in 7 Sprachen** (de/en/zh/ja/ko/es/fr): im Panel umschaltbar, wird gemerkt
- **Statistik**: Summen / Kategorien / 7-Tage-Trend / Top-5-Einfügungen
- **Optionaler Autostart**, Tray-Symbol, ziehbare Panelhöhe die gemerkt wird
- **Aufnahme pausieren** (Ein-Klick im Tray, optionale Auto-Fortsetzung) und **gesamten Verlauf leeren**, jeweils mit einem Klick
- **Datenschutz-Abfangen, alles Opt-in**: Filter für sensible Wörter (mit Schalter), App-Ausschluss, Bild-Aufnahme-Schalter; von Passwort-Managern markierte Kopien werden nie aufgezeichnet (Windows-Standard)
- **Konfigurierbarer Aufruf-Hotkey** (konfliktsichere Neuregistrierung)
- **Übergroßer Text** (>256 KB) wird mit Inline-Vorschau in eine lokale Datei ausgelagert — nichts geht still verloren

## Datenschutz

- 100 % lokal. Daten liegen in `%APPDATA%\com.clipvault.app\` (SQLite WAL + PNG-Dateien)
- Kopien aus Passwort-Managern werden nie aufgezeichnet (sensible Zwischenablage-Marker werden respektiert)
- Nutzerdefinierter Filter für sensible Wörter
- Kein Netzwerk-Code in der Binärdatei. Details in [SECURITY.md](SECURITY.md)

## Installation

Installer aus den [Releases](../../releases) laden, oder aus dem Quellcode bauen:

```bash
npm install
npx tauri build        # Ausgabe in src-tauri/target/release/bundle/
```

## Bauen

Tauri 2 + Vue 3 + Rust (rusqlite / handgeschriebene Win32-Schicht).

> **Hinweis:** Releases immer mit `npx tauri build` bauen. Ein nacktes `cargo build --release`
> lässt die Dev-Konfiguration aktiv und das Fenster versucht, die UI vom vite-Devserver
> zu laden (siehe [docs/postmortem-2026-09-11.md](docs/postmortem-2026-09-11.md)).

Entwicklungsmodus:

```bash
npx vite --port 5173        # Terminal 1
npx tauri dev               # Terminal 2
```

Tests: `cd src-tauri && cargo test` (Zwischenablage-Roundtrip / DIB-Dekodierung / DROPFILES-Parsing)

## Testen & Abnahme

Siehe [docs/TESTING.md](docs/TESTING.md) und [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md).

## Technische Hinweise

- Kompatibilität des Zwischenablage-Schreibens: Bilder werden sowohl als Standard-CF_DIB (bottom-up) als auch im registrierten „PNG“-Format angeboten — Chromium-basierte Apps (Chrome/VS Code/Electron) und GDI-Apps lesen beide korrekt; die DIB-Dekodierung unterstützt BI_BITFIELDS-Kanalmasken (Screenshot-Tools)
- Der Einfüge-Tastenanschlag kommt von einem On-Demand-Injector-Subprozess (selbstheilend) (bei manchen Setups werden synthetische Tasten im eigenen Prozess verschluckt)
- WebView-IPC-Heartbeat-Wächter: das Panel erkennt einen toten IPC-Kanal und lädt sich selbst neu (JS-Check alle 5 s, nativer Fallback binnen 60 s)
- Alle Daten liegen in `%APPDATA%\com.clipvault.app\`

## Lizenz

[MIT](LICENSE)
