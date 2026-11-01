# ClipVault

[![Release](https://img.shields.io/github/v/release/ardss/clipvault)](../../releases)
[![CI](https://img.shields.io/github/actions/workflow/status/ardss/clipvault/ci.yml?label=CI)](../../actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/ardss/clipvault)](LICENSE)

**日本語** | [English](README.md) | [简体中文](README.zh-CN.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Français](README.fr.md) | [Deutsch](README.de.md)

強化版 Win+V — クリーンでミニマルな Windows クリップボード履歴マネージャー。

`Alt+V` でパネルを呼び出す → 検索 → クリックで元のウィンドウに貼り付け。完全ローカル：ネットワークゼロ、アカウントゼロ、テレメトリゼロ。

## 機能

- **長期履歴**：テキスト・画像・ファイル・リッチテキスト（Web/Word 形式）・リンク — 再起動後も保持、保持期間は設定可能
- **検索**：即時キーワード絞り込み（中国語・英語対応）
- **ピン留め**：よく使う項目を最上位に。クリーンアップの対象外
- **フィルター**：すべて / テキスト / 画像 / リンク / ファイル / 固定
- **貼り戻し**：パネルは作業中のウィンドウからフォーカスを奪いません。項目をクリックすると元の場所に貼り付きます
- **ホバー プレビュー**：省略されたテキストや画像サムネイルが横のプレビューで展開（スクロール・選択・部分コピー可）
- **キーボード優先**：`↑↓` 移動、`Enter` 貼り付け、`Del` 削除、`Esc` 閉じる
- **ダーク/ライトテーマ、7 言語 UI**（日/英/中/韓/西/仏/独）：パネル内で切り替え、記憶されます
- **統計**：合計 / カテゴリ別 / 直近 7 日間の推移 / 貼り付け Top 5
- **オプションのスタートアップ起動**、トレイアイコン、ドラッグで調整でき記憶されるパネル高さ
- **取得の一時停止**（トレイからワンクリック、自動解除も設定可）と**履歴の全消去**
- **プライバシー保護はすべてオプトイン**：機密語フィルター（スイッチ付き）、除外アプリ、画像取得の ON/OFF。パスワードマネージャーがマークしたコピーは記録されません（Windows 標準機構）
- **呼び出しホットキー設定**（競合時も安全に再登録）
- **超大テキスト**（256 KB 超）はインライン プレビュー付きでローカル ファイルに退避 — 何も黙って失われません

## プライバシー

- 100% ローカル。データは `%APPDATA%\com.clipvault.app\` に保存（SQLite WAL + PNG ファイル）
- パスワード マネージャーのコピーは記録されません（機密クリップボード マーカーを尊重）
- ユーザー定義の機密語フィルター
- バイナリにネットワーク コードなし。詳細は [SECURITY.md](SECURITY.md)

## インストール

[Releases](../../releases) からインストーラーを入手するか、ソースからビルド：

```bash
npm install
npx tauri build        # src-tauri/target/release/bundle/ に出力
```

## ビルド

Tauri 2 + Vue 3 + Rust（rusqlite / 手書き Win32 レイヤー）。

> **注意**：リリースは必ず `npx tauri build` で。素の `cargo build --release` では
> 開発設定が残り、ウィンドウが vite 開発サーバーから UI を読み込もうとします
> （[docs/postmortem-2026-09-11.md](docs/postmortem-2026-09-11.md) 参照）。

開発モード：

```bash
npx vite --port 5173        # ターミナル 1
npx tauri dev               # ターミナル 2
```

テスト：`cd src-tauri && cargo test`（クリップボード ラウンドトリップ / DIB デコード / DROPFILES 解析）

## テストと受け入れ

[docs/TESTING.md](docs/TESTING.md) と [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md) を参照。

## 技術ノート

- クリップボード書き込み互換性：画像は標準 CF_DIB（ボトムアップ）と登録形式 "PNG" の両方で提供 — Chromium 系（Chrome/VS Code/Electron）も GDI アプリも正しく読めます。DIB デコードは BI_BITFIELDS チャネル マスク対応（スクリーンショット ツール）
- 貼り付けキーストロークはオンデマンドのインジェクター サブプロセスが発行（自己修復）（一部の環境ではプロセス内の合成キーが握りつぶされるため）
- WebView IPC ハートビート ウォッチドッグ：パネルは死んだ IPC チャネルを検出して自己リロード（JS は 5 秒ごと、ネイティブ フォールバックは 60 秒以内）
- すべてのデータは `%APPDATA%\com.clipvault.app\` に保存

## ライセンス

[MIT](LICENSE)
