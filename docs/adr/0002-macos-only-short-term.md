# 0002 — macOS 専用を当面維持し、Linux/Windows 対応は P6 で判断

> Status: Accepted
> Date: 2026-05-28

## Context

nohrs は Finder の代替を目指して macOS で開発が始まった。現状 (v0.1.0):

- gpui の Metal バックエンドを使用、Linux サポートは gpui 側で段階的に進行中
- 既存依存・実装に macOS 固有 API (`trash` crate、Spotlight ベースの初期検索) を含む
- 作者の主開発環境が macOS

一方で、ROADMAP では:
- Cargo workspace 化、tokio 撤去、SQLite/Tantivy 採用といった **OS 非依存の選択**を進める
- web ホスティングは Cloudflare で OS 非依存
- 開発環境として Linux + docker / nix の手順を整備

これらは Linux サポートの素地を作るが、**v0.7.0 (P6) まで macOS のみを正式サポート対象とする**。

## Decision

- **v0.2.0〜v0.6.0 は macOS のみ正式サポート**
- Linux ビルド (`cargo build --features gui` on Linux) は **動けばラッキー** の位置付け。CI で fail させない、issue で報告されたら best effort で fix
- Windows は当面非対応
- **v0.7.0 (P6) で多 OS 戦略を再評価**: gpui の Linux 完成度・開発リソース・ユーザー需要を元に「Linux も tier 1 にする / macOS のみ継続」を決定 (本 ADR を Superseded で更新)

## Consequences

### Positive

- 開発リソースを 1 OS に集中、機能開発の速度を最大化
- macOS 固有 API (Spotlight 残骸の除去、QoS / IOPM / NSProcessInfo) を躊躇なく使える
- バグ修正の対象が 1 OS で、品質改善が進めやすい

### Negative

- Linux / Windows コミュニティへのアプローチが遅れる
- 一部の機能 (Tantivy / SQLite / ureq 等は OS 非依存だが) でクロスプラットフォーム対応の検証が後回しになる
- 「macOS だけ?」という質問が GitHub Discussions に頻発する可能性 → README に明示

## Alternatives Considered

| 案 | 棄却理由 |
|----|---------|
| Day 1 から Linux / Windows 並列対応 | gpui の Linux 完成度がまだ過渡期、Windows は更に先。並列開発は機能開発を 3 倍遅らせる |
| Linux のみ追加対応 (Windows は無視) | gpui の Linux 完成度を見極めるには P5 までの実観察が必要 |
| macOS 専用の永続化 | エコシステム拡大時に痛む。ROADMAP の P6 で再評価する余地を残す |
