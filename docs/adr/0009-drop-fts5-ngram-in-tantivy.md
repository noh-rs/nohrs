# 0009 — 全文・部分一致とも tantivy に集約し SQLite FTS5 を採用しない

> Status: Accepted
> Date: 2026-09-20
> Amends: [ADR 0001](./0001-sqlite-tantivy-hybrid-search.md) の段階移行表 (Decision 本体は有効)

## Context

[ADR 0001](./0001-sqlite-tantivy-hybrid-search.md) と [`docs/search.md`](../search.md) §2 は、検索基盤を三段階で進めると定めていた。

| 版 | Phase | 内容 |
|----|-------|------|
| V1 (ripgrep) | 現状 | オンデマンド検索、永続インデックスなし |
| V2 (SQLite FTS5) | P3 | trigram tokenization、増分更新、SQLite で完結 |
| V3 (SQLite + Tantivy) | P4 | BM25 + code-aware ngrams、identifier 分解 |

この段階分けの前提は「tantivy の大きな依存を払う前に、SQLite だけで永続全文検索を得る」という踏み台であった。

その前提は現在のコードでは成立しない。`crates/nohrs-services/src/search/` の実装を確認した結果は以下のとおりである。

- `IndexManager` が tantivy のインデックスと writer を保持し、全体インデックスと増分インデックスの双方を実装している
- スキーマは `path` (STRING) / `filename` (TEXT) / `content` (TEXT) / `last_modified` (FAST) / `is_directory` (FAST) であり、**本文を全文インデックスしている**
- `SearchEngine` は `SearchScope::Home` を `IndexManager::search` へ、`SearchScope::Root` を Spotlight (macOS) / ripgrep へ振り分ける
- `FileWatcher` の変更通知が専用スレッド経由で `process_changes` に渡り、増分更新が動作している
- 初回インデックスは `InitialIndexingJob` として GPUI の background executor へ委譲され、進捗を `postage::watch` で報告する

すなわち V3 の中核である「永続全文インデックス + 増分更新」は home スコープについて既に出荷済みであり、V2 が踏み台として担うはずだった役割は残っていない。ripgrep は非 macOS の root スコープで使われているのみで、FTS5 はどこにも存在しない。

残る論点は、FTS5 に踏み台以外の独立した存在理由があるかであった。当初「部分一致 (`*abc*`) は tantivy のトークナイザでは扱えず、FTS5 の trigram が必要」と判断したが、これは誤りであった。

## Decision

**SQLite FTS5 を採用しない。全文検索と部分一致の双方を tantivy に集約する。**

部分一致は tantivy の `NgramTokenizer` で賄う。tantivy 0.26.1 に対して実測した。

```text
ngram field, substring 'er_fil' -> 1 hit(s)
plain TEXT field, substring 'er_fil' -> 0 hit(s)
```

文字列 `"explorer_file_ops.rs"` (ファイル名を模した 1 語のトークン) を両フィールドに投入し、トークン境界をまたぐ部分文字列 `er_fil` を照会した結果である。`NgramTokenizer::all_ngrams(3, 3)` を `index.tokenizers().register("tri", ...)` で登録したフィールドは一致し、既定の `TEXT` は一致しない。FTS5 の trigram と同一の手法がライブラリ側に用意されており、部分一致はトークナイザの選択の問題であって原理的制約ではない。

実装上の要点として、ngram フィールドは `IndexRecordOption::WithFreqsAndPositions` を要する。クエリ文字列も同じトークナイザで 3-gram 列に分解され、`QueryParser` がそれをフレーズクエリとして扱うため、位置情報がないと `The field ... does not have positions indexed` で失敗する。gram の順序で部分文字列を再構成する点は FTS5 trigram と同じである。

加えて tantivy 0.26.1 は `RegexQuery` を備え、その doc がワイルドカード (`ho*se`) を regex へ変換して実現する旨を明示している。

SQLite の役割は検索から外し、**状態管理に専念させる**。

| 担当 | 役割 |
|------|------|
| **tantivy** | 全文検索 (BM25)、ファイル名・パスの部分一致 (ngram)、code-aware ngrams・identifier 分解 (P4) |
| **SQLite (`nohrs-store`)** | メタデータ (path / mtime / size / inode / hash)、削除追跡、状態管理、差分検出 |
| **notify-debouncer-mini** | ファイルシステム変更検出 |

改訂後の段階移行は以下とする。

| 版 | Phase | 内容 |
|----|-------|------|
| V1 (ripgrep) | 現状 | 非 macOS の root スコープで存続 (macOS の root は Spotlight) |
| **V2 (ngram フィールド追加)** | **P3** | `filename` / `path` に ngram トークナイザのフィールドを追加し部分一致を提供。SQLite はメタデータ・差分検出に限定 |
| V3 (code-aware) | P4 | identifier 分解 (camelCase / snake_case)、plugin への WIT 公開 |

## Consequences

### Positive

- 本文インデックスを二重に持たずに済む (ディスク・インデックス CPU の重複を回避)
- P4 で捨てる前提の FTS5 全文検索を書かずに済む。ADR 0001 自身が tantivy を「全文検索インデックスの本命」と位置づけている
- 検索経路が一つになり、ランキングと一致規則がバックエンド間で食い違わない
- SQLite の責務が「関係クエリが要るもの」に限定され、ADR 0001 の役割分担表とむしろ整合する

### Negative

- ngram フィールドは索引が膨らむ。3-gram は位置情報を要するため posting list はさらに大きくなる。対象を `filename` / `path` に限定して緩和する。`content` への ngram 適用は行わない
- SQLite 側に全文検索の退避経路がなくなるため、tantivy インデックスの破損時は再構築が必要となる (既存の `InitialIndexingJob` が空インデックスを検出して再構築する経路を持つ)
- `search.backend` の選択肢から `sqlite-fts` を削除する。未実装の綴りであり、この設定を書いたファイルは存在し得ないが、書かれていた場合は警告付きで既定値 (`auto`) に落ちる

## Alternatives Considered

| 案 | 棄却理由 |
|----|---------|
| 当初計画どおり FTS5 を実装 | 本文インデックスの重複。P4 で置換される前提の作業であり、踏み台としての前提が既に失われている |
| FTS5 を部分一致専用で導入 | `NgramTokenizer` が同一手法を提供するため、二つ目のインデックスと同期の責務を増やすだけになる |
| `path` を `STRING` のまま `RegexQuery` で部分一致 | 語彙全体への regex 走査となり、ngram の転置インデックスより遅い。`RegexQuery` は補助手段として残す |
| SQLite を検索から完全排除 | メタデータ・削除追跡・差分検出は関係クエリが適する。ADR 0001 の判断を維持する |
