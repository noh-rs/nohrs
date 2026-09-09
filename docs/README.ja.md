<div align="center">
  <img src="../assets/doc/icon.png" alt="Nohrs アイコン" width="128" height="128">

  # Nohrs

  **Launcher × Explorer** — Rust 製の高速・拡張可能・プラグイン対応な macOS 向けファイルワークスペース。

  [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE)
  [![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](../rust-toolchain.toml)
  [![Platform](https://img.shields.io/badge/platform-macOS-lightgrey.svg)](https://www.apple.com/macos)
  [![Discord](https://img.shields.io/badge/Discord-join-5865F2?logo=discord&logoColor=white)](https://discord.gg/dZM7fUtE94)

  [クイックスタート](#クイックスタート) · [なぜ-nohrs](#なぜ-nohrs) · [ロードマップ](ROADMAP.md) · [English README](../README.md)

  <img src="../assets/doc/screen-shot.jpeg" alt="Nohrs スクリーンショット" width="800">
</div>

Nohrs は、Raycast 風ランチャーとモダンでキーボード中心のファイルエクスプローラを 1 つのアプリに統合します。高速でスクリプタブル、かつサンドボックス化されたプラグインで拡張できる、Finder の代替を目指しています。

## デモ

<div align="center">
  <img src="../assets/doc/demo.gif" alt="Nohrs の動作画面 — ファイル閲覧・プレビュー・全文検索" width="760">
</div>

## なぜ Nohrs?

- **Launcher first-class** — グローバルホットキーから呼び出せるランチャーを内蔵。後付けではありません。
- **Explorer first-class** — スプリットビュー / タブ / ドラッグ＆ドロップ / バルク操作を備えた現代的なファイラー。
- **WASM Component Model プラグイン** — Rust / TypeScript / Python で拡張可能。サンドボックス + 明示同意の権限モデルで動作します。
- **Spotlight に依存しない検索** — SQLite + Tantivy のハイブリッドインデックスで、OS の検索デーモンに依存せず、コードベースにも対応。

これらの柱がどのリリースに対応するかは[ロードマップ](ROADMAP.md#ビジョン)を参照してください。

## クイックスタート

### インストール (macOS)

Nohrs は **pre-alpha** であり、まだ公開されていません。最初のリリース公開後は次のように導入できる予定です。

```sh
# 予定 — 現時点では未提供
cargo install nohrs
```

ビルド済みの macOS バイナリは [Releases](https://github.com/noh-rs/nohrs/releases) ページで提供予定です。現時点ではソースからビルドしてください。

### ソースからビルド

```sh
# コアライブラリのみ
cargo build

# GUI バイナリ
cargo build --features gui
cargo run --features gui --bin nohrs
```

#### gpui のための macOS 前提条件

gpui は macOS 上で Metal を使用するため、Xcode と Metal ツールチェーンが必要です。

1. App Store から Xcode をインストールします（一度起動してセットアップを完了させてください）。
2. コマンドラインツールをインストールします: `xcode-select --install`
3. CLI がインストール済みの Xcode を使用するよう設定します: `sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer`
4. Metal ツールチェーンが見つからないとエラーが出る場合: `xcodebuild -downloadComponent MetalToolchain`

> Linux native / Nix / Docker でのセットアップは[推奨セットアップ早見表](dev-environment.md#1-推奨セットアップ早見表)を参照してください。

## ステータス

**Pre-alpha (v0.x)。** 活発に開発中であり、API・UI・データ形式は予告なく変更されます。現在の GUI は gpui に接続中の初期エントリーポイントです。粗削りな点が多いため、不具合は Issue でお知らせください。

## ロードマップ

Nohrs は 6 つのシリアルなフェーズで開発します。P1 は `0.0.x` で iterate し、P2 完了で最初の MVP を `0.1.0` として切り、P6 で `0.5.0`、安定化で `1.0.0` に到達します。概要は次のとおりです。

| Phase | Milestone | テーマ |
|-------|-----------|--------|
| **P1** | `0.0.x` | Foundation — 品質改善・workspace 化・開発/CI 基盤・web MVP |
| **P2** | `0.1.0` | Explorer Essentials — DnD・ファイル操作・スプリットビュー・タブ・永続化 |
| **P3** | `0.2.0` | Launcher & Search — グローバルホットキーランチャー・SQLite FTS5 検索 |
| **P4** | `0.3.0` | Plugin Host — WASM Component Model・3 言語テンプレ |
| **P5** | `0.4.0` | Ecosystem — Plugin Store・コミュニティプラグイン |
| **P6** | `0.5.0` | Stabilization — 多 OS 戦略・パフォーマンスゲート・ドキュメント |

ビジョン・各フェーズの詳細・設計ドキュメントは [`docs/ROADMAP.md`](ROADMAP.md) にあります。

## コミュニティ

- **Discord**: https://discord.gg/dZM7fUtE94
- **X (Twitter)**: https://x.com/nohrsdotapp
- **GitHub**: https://github.com/noh-rs/nohrs

コントリビューションを歓迎します。Issue や Pull Request をお気軽にどうぞ。

## ライセンス

[MIT License](../LICENSE) のもとで公開しています。
