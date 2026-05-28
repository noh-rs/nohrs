# 0001 — SQLite + Tantivy ハイブリッド検索採用 (Spotlight 一本化棄却)

> Status: Accepted
> Date: 2026-05-28

## Context

旧 `QUALITY_IMPROVEMENT_PLAN.md` の Phase 3 は、当面 macOS 専用であることを根拠に **「Tantivy/notify を撤去し macOS の Spotlight (`mdfind`) に検索を一本化する」** という方針を提示していた。

しかし、以下の理由でこの方針は再検討された:

- **OS 依存が深まる**: Spotlight 一本化は将来の Linux / Windows 対応を阻害する (再実装 + 二系統メンテ)
- **コードベース検索が弱い**: `mdfind` はファイルメタデータの fuzzy 検索には強いが、コードの identifier (camelCase / snake_case) 検索や正規表現には不向き
- **Spotlight 除外パス**: ユーザー / OS が Spotlight から除外したパスはヒットしないため、開発者が `~/dev/` 等を除外しているとほぼ機能しない
- **依存撤去のメリットが過大評価**: Tantivy + notify の依存ツリーは大きいが、`bundled` SQLite + WAL モードで builds 安定性は確保可能
- **Gist の検索設計案** (作者本人) が SQLite + Tantivy ハイブリッドを前提に書かれており、ROADMAP の前提と整合させるべき

## Decision

**SQLite + Tantivy ハイブリッド検索を採用する。** Spotlight 一本化案は棄却する。

| 担当 | 役割 |
|------|------|
| SQLite | ファイルメタデータ (path / mtime / size / inode / hash)、削除追跡、状態管理、差分検出、trigram 全文検索 (FTS5、V2 段階) |
| Tantivy | 全文検索インデックス、BM25 ランキング、コード対応 ngrams、identifier 分解 (V3 段階) |
| notify-debouncer-mini | ファイルシステム変更検出 (debounce 500ms) |

段階移行:

- **V1 (現状)**: ripgrep オンデマンド検索
- **V2 (P3)**: SQLite FTS5
- **V3 (P4)**: SQLite + Tantivy 統合

詳細は [`docs/search.md`](../search.md) 参照。

## Consequences

### Positive

- OS 非依存の検索基盤を獲得 (将来の Linux / Windows 対応で再実装不要)
- コード対応 (identifier 分解 / ngrams / 正規表現) の検索が可能
- ユーザー設定の `.gitignore` / 除外パスを完全に nohrs 側で制御できる
- plugin (P4) から WIT 経由で search API を提供しやすい
- Gist 設計案と整合

### Negative

- 依存ツリーが大きくなる (`tantivy`, `notify`, `notify-debouncer-mini`)
- 初回 indexing 時のリソース消費を制御する必要 (詳細は [`docs/search.md`](../search.md) §7 リソース制限)
- インデックスストレージのディスク使用 (1-5 GB 程度を想定、設定で上限可)

## Alternatives Considered

| 案 | 棄却理由 |
|----|---------|
| Spotlight 一本化 (旧プラン) | 上記 Context 参照 |
| ripgrep のみ (V1) | 全文検索の永続化なしでは UX が劣化 (毎回 walk + scan は遅い) |
| Tantivy のみ (SQLite 無し) | メタデータ・削除追跡が弱い (Gist 設計案でも同様に却下) |
| Sled / RocksDB ベース | SQL 表現力なしで差分検出が複雑化 |
