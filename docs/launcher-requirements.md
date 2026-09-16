# Launcher Requirements — 第一級ランチャーの要件定義

> Status: Draft (要件定義。実装は P3 以降に分割して着手)
> Related: [`launcher.md`](./launcher.md) (実装仕様) / [`notch.md`](./notch.md) / [`migration.md`](./migration.md) /
> [`ROADMAP.md`](./ROADMAP.md) / [`plugin-api.md`](./plugin-api.md) / [`search.md`](./search.md) / [`persistence.md`](./persistence.md)

[`launcher.md`](./launcher.md) は「ウィンドウをどう出し、何をどう描くか」の実装仕様です。本書はその一段上で、
**何を作り、何を作らないか / どの順で作るか**を決めます。軸は 2 つだけです。

1. **包含**: Raycast / Tinycast / Supaste / notch 系アプリが提供している機能を、nohrs の 1 プロセスで代替できること。
2. **乗り換え容易性**: 上記から移ってくるユーザーが、**設定を作り直さずに**初日から同じ速度で使えること
   (詳細は [`migration.md`](./migration.md))。

---

## 0. 用語

| 用語 | 定義 |
|------|------|
| **サーフェス (surface)** | ユーザーが nohrs に触れる入口。現在は explorer window と launcher window の 2 つ。本書で **notch** を 3 つ目として追加する |
| **ルート検索 (root search)** | launcher を開いた直後の、全カテゴリ横断の検索状態 |
| **アイテム (item)** | 結果リストの 1 行。ファイル / アプリ / コマンド / クリップ / スニペット等 |
| **アクション (action)** | アイテムに対して実行できる操作。`Enter` が primary、`Cmd+K` でアクションパネル |
| **コマンド (command)** | それ自体がアイテムとして検索でき、実行すると何かが起きる単位。core / plugin の両方が提供する |
| **ビュー (view)** | コマンドが launcher 内に push する画面。list / detail / form / grid |
| **エイリアス (alias)** | コマンドに付ける短い別名。入力するとそのコマンドが最上位に来る |
| **フォールバック (fallback)** | 結果ゼロのときに提示される「このクエリを引数として実行するコマンド」 |

---

## 1. なぜ今これを決めるのか

現状 (`crates/nohrs-launcher`) は **ファイル名検索専用のウィンドウ**です。グローバルホットキー・IME・
fuzzy ランキング・結果リストという「土台」は動いていますが、`Command` trait も履歴も設定もありません
([`launcher.md`](./launcher.md) §13)。

この土台の上に何を載せるかは、次の理由で**今決める価値**があります。

- **`Command` trait の置き場所が crate 依存の向きを決める**。explorer / notch / plugin が同じレジストリに
  コマンドを登録できないと、後から直すコストが跳ね上がる ([§7.1](#71-command-レジストリの置き場所))。
- **クリップボード履歴・スニペット・ウィンドウ管理は、いずれも「常駐プロセス」を前提にする**。
  常駐をどう持つかは launcher 単体ではなくアプリ全体の設計判断 ([§7.4](#74-常駐プロセスと権限))。
- **Raycast 拡張の互換方針は、plugin API (WIT) の形を変える**。one-shot な `run-command → view-node` では
  React ベースの拡張を受け止められない ([`migration.md`](./migration.md) §4)。
- **notch は新しいサーフェス**であり、launcher と同じコマンド基盤に乗るかどうかで実装量が数倍変わる。

---

## 2. 参照プロダクトの棚卸し

> 調査日 2026-09-16。各社の機能は変わるため、数値や価格は参考値として扱うこと。

### 2.1 Raycast — 母集合

機能の**母集合**であり、乗り換え元として最大。マニュアルの章立てがそのまま機能一覧になっている。

| 分類 | 機能 |
|------|------|
| 基本 | 検索バー / アクションパネル (`Cmd+K`) / エイリアス / コマンド単位ホットキー / 引数 / フォールバックコマンド / お気に入り・並び替え / import & export / 設定 |
| コア | Snippets / Quicklinks / Clipboard History / Notes / Focus / File Search / Extensions / Translate / Emoji & Symbols / Calendar / Calculator / Screenshots (画像内テキスト検索) / Window Management / Navigation (push-pop) |
| パワー | Hyper Key / Cloud Sync / Dynamic Placeholders / System Commands / Script Commands / Themes / Auto Quit / Run / Games |
| AI | Quick AI / AI Chat / Dictation / Screen Awareness / AI Commands / AI Extensions / Automations / Agents / MCP / Skills / Projects / Local Models / Custom Providers / BYOK / BYOS |
| 配布・組織 | Store (数千の拡張) / Private Store / Teams 共有 (commands / quicklinks / snippets) / SAML・SCIM / allow-list |
| プラットフォーム | macOS / Windows / iOS / ブラウザ拡張 |
| 価格 | Free (クリップボード履歴 3 ヶ月・Notes 5 件・AI なし) / Pro $10〜$50 月 / Teams $15〜$65 per seat / Enterprise |

**読み取れること**: 無料枠で削られているのは **AI・Cloud Sync・テーマ・翻訳・クリップボード履歴の保持期間**。
つまりここが nohrs の「ローカルで全部無料」が効く場所。

### 2.2 Tinycast — ネイティブ最小 OSS + Raycast 拡張互換

AGPL-3.0 / SwiftUI + AppKit / 依存ゼロ / メモリ 100MB 未満 / テレメトリ・アカウントなしを掲げる macOS 26+ 専用ランチャー。

Raycast との差分で注目すべきもの:

| 機能 | 内容 |
|------|------|
| **Raycast 拡張の実行** | 既存の Raycast 拡張を JavaScriptCore 上で動かし、SwiftUI としてレンダリング |
| **Raycast 設定の import** | 「Bring your setup with you」を明示的に売りにしている |
| **per-app hotkey** | アプリごとにキーを割り当て、フォーカス / 隠すをトグル |
| **Window & menu search** | 開いているウィンドウとメニューバー項目を検索して実行 |
| **カスタムシェルコマンド** | 名前付きシェルコマンドを fuzzy 検索 or 専用ホットキーで実行 |
| **Apple Shortcuts** | Shortcuts.app のショートカットを検索・実行 |
| **アプリアンインストーラ / 入力ソース切替 / 31 のシステムアクション / 34 のウィンドウ管理コマンド** | ネイティブ API を薄く撒いたコマンド群 |
| **AI** | Apple Intelligence / Codex / Claude / OpenCode / 任意 API。**既定オフ** ("Nothing turns on until you do") |
| **Tab でモード循環** | launcher → AI chat → clipboard |

**読み取れること**: 「OSS + ネイティブ + 依存ゼロ + Raycast 互換」は nohrs のポジションと**正面衝突する**。
差別化は Explorer 統合・WASM サンドボックス・クロスプラットフォーム・notch の 4 点に置く ([§3](#3-プロダクト原則))。

### 2.3 Supaste — クリップボードの深さ + notch shelf

「local-first なクリップボード & スクリーンショット履歴」。買い切り ($15〜29)、macOS 14+。

| 機能 | 内容 |
|------|------|
| タイムライン UI | コピーしたものを時系列カードで一覧。Library view / グループ (アプリ別) / 種別フィルタ / Smart Filter / カスタムカテゴリ |
| **Notch shelf** | notch から履歴に手が届く。nohrs が notch を第一級にする直接の根拠 |
| 対応種別 | text / link / image / file / code / color / screenshot / SVG / asset / gradient |
| **OCR** | スクリーンショット・画像内テキストを自動認識して検索対象に |
| **Multi-Clip Copy** | 複数アイテムを 1 カードに結合 |
| **Inline Shortcuts** | `;welcome` のようなキーワードで展開 (= スニペット) |
| **Clip Reminders** | クリップにリマインダを設定 |
| 同期 | iCloud |
| 安全性 | Sensitive detection (パスワード等の除外)、完全オフライン動作 |

**読み取れること**: クリップボード履歴は「あれば良い」ではなく**単独で有料プロダクトが成立する深さ**がある。
Raycast の Clipboard History より一段深いところ (OCR / カテゴリ / multi-clip / reminder) まで作って初めて
「Supaste を消せる」。

### 2.4 notch 系 (NotchNook / Boring Notch / Alcove / 他)

各社の機能の**和集合**:

| 分類 | 機能 |
|------|------|
| メディア | Now Playing + 操作 / ビジュアライザ / 歌詞同期 |
| ライブアクティビティ | 充電 / AirDrop / ダウンロード / タイマー / 進捗表示 |
| HUD 置換 | 音量・輝度の OS HUD を notch 表示に差し替え |
| **ファイルシェルフ** | ドラッグ & ドロップの一時置き場、AirDrop 送信、フォルダのピン留め |
| ウィジェット | カレンダー / リマインダー / Shortcuts / ミラー (カメラ) / システム統計 (CPU・メモリ・ネット・ディスク・GPU) / バッテリー (Bluetooth 機器含む) |
| 通知 | インライン返信 / 音声返信 |
| インジケータ | マイク・カメラ使用中 / Focus モード |
| ツール | クリップボード履歴 / カラーピッカー / ポモドーロ |
| 開発者向け | PR レビューキュー / CI 監視 / AI エージェント監視 |
| 体験 | ジェスチャ (ホバー・スワイプ・スクロール) / テーマ / 触覚フィードバック / 多言語 |

価格帯は無料 (Boring Notch, OSS) 〜 買い切り $15-25。

**読み取れること**: notch アプリは「ランチャーとは別プロセスで常駐する 4 つ目のアプリ」になっている。
nohrs は既に常駐してインデックスを持つので、**同じプロセスで出せる**のが構造的な優位。詳細は [`notch.md`](./notch.md)。

### 2.5 出典

- Tinycast: <https://abue-ammar.github.io/tinycast/> / <https://github.com/abue-ammar/tinycast>
- Raycast: <https://www.raycast.com/> / <https://manual.raycast.com/> / <https://www.raycast.com/pricing> / <https://developers.raycast.com/>
- Supaste: <https://www.supaste.com/>
- notch 系比較: <https://macnotch.io/compare/best-mac-notch-apps> / <https://notchy.dev/best-mac-notch-apps/>

---

## 3. プロダクト原則

要件の優先度は、迷ったとき次の順で決めます。上にあるものが下を上書きします。

| # | 原則 | 具体的に意味すること |
|---|------|--------------------|
| **1** | **ローカルファースト / 無音** | ネットワークに出る機能は**すべてオプトイン**。既定でアカウント不要・テレメトリなし・外部送信なし。AI も既定オフ (Tinycast の "Nothing turns on until you do" と同じ立場) |
| **2** | **Explorer × Launcher が差別化** | 他社が持てないのは「ファイラーとランチャーが同じインデックス・同じ選択状態・同じ履歴を共有していること」。この結線を持つ機能を優先する |
| **3** | **ロックインしない** | 全ユーザーデータは平文の JSON / SQLite で出し入れできる。乗り換えて**来られる**ことと、乗り換えて**出て行ける**ことは同じ 1 つの約束 |
| **4** | **サンドボックス** | 拡張は WASM Component Model + 明示同意の権限モデル ([`plugin-permissions.md`](./plugin-permissions.md))。「速いから」で任意コード実行を許さない |
| **5** | **クロスプラットフォーム前提で設計** | 当面 macOS 優先 ([ADR 0002](./adr/0002-macos-only-short-term.md)) だが、OS 固有能力は trait の裏に置き、Linux で「無い」と言えるようにする |
| **6** | **常駐は軽く** | 常駐時アイドル CPU < 1%、常駐 RSS < 150MB を予算とする ([§6](#6-非機能要件)) |

---

## 4. 機能包含マトリクス

「参照元の機能 → nohrs でどう実現するか」の一覧です。**Phase** は [§8](#8-フェーズ計画-roadmap-改訂案) の改訂案に対応します。

凡例: ◎ = 差別化として強く作る / ○ = 同等に作る / △ = 縮小版 / ✕ = 作らない (理由を [§10](#10-non-goals) に記載)

### 4.1 ランチャーの土台

| # | 機能 | Raycast | Tinycast | nohrs | Phase | 備考 |
|---|------|:-------:|:--------:|:-----:|:-----:|------|
| 1.1 | グローバルホットキー | ✔ | ✔ | ✔ 実装済 | P3 | Wayland は portal 経由 ([`launcher.md`](./launcher.md) §13) |
| 1.2 | ルート検索 (横断) | ✔ | ✔ | ○ | P3 | セクション分け・カテゴリ順 |
| 1.3 | アクションパネル `Cmd+K` | ✔ | ✔ | ○ | P3 | アイテム単位の `Vec<Action>` |
| 1.4 | エイリアス | ✔ | ✔ | ○ | P3 | 完全一致で最上位固定 |
| 1.5 | コマンド単位ホットキー | ✔ | ✔ | ○ | P3.5 | プロセス全体のホットキーレジストリが要る ([§7.2](#72-ホットキーレジストリ)) |
| 1.6 | 引数つき実行 (`> cmd arg`) | ✔ | ✔ | ○ | P3 | `ArgSpec` |
| 1.7 | フォールバックコマンド | ✔ | — | ○ | P3.5 | 結果ゼロ時に「このクエリで検索 / 翻訳 / AI」 |
| 1.8 | お気に入り / 並び順の固定 | ✔ | ✔ (`Cmd+1-0`) | ○ | P3.5 | |
| 1.9 | push-pop ナビゲーション | ✔ | ✔ | ○ | P3 | [`launcher.md`](./launcher.md) §8 |
| 1.10 | 詳細ペイン (`Tab`) | ✔ | ✔ | ○ | P3 | [`launcher.md`](./launcher.md) §7 |
| 1.11 | 使用履歴による並び替え | ✔ | ✔ | ◎ | P3 | explorer の開封履歴と**同じテーブル**を使う (原則 2) |
| 1.12 | モード循環 (`Tab` で launcher→AI→clipboard) | — | ✔ | ○ | P4 | nohrs は launcher → clipboard → notch shelf |
| 1.13 | テーマ | ✔ (Pro) | ✔ | ○ | P5 | 無料。config の `[theme]` を拡張 |
| 1.14 | キーマッププリセット | — | — | ◎ | P3.5 | `raycast` / `spotlight` / `alfred` / `tinycast` ([`migration.md`](./migration.md) §6) |

### 4.2 起動・ウィンドウ・システム

| # | 機能 | Raycast | Tinycast | nohrs | Phase | 備考 |
|---|------|:-------:|:--------:|:-----:|:-----:|------|
| 2.1 | アプリ起動 (fuzzy) | ✔ | ✔ | ○ | **P3** | 現状ファイルのみ。`/Applications` + `~/Applications` + Linux `.desktop` |
| 2.2 | 起動中アプリ一覧 / 終了 / 全終了 | ✔ | ✔ | ○ | P3.5 | |
| 2.3 | per-app hotkey (フォーカス / 隠す) | ✔ | ✔ | ○ | P3.5 | ホットキーレジストリ前提 |
| 2.4 | ウィンドウ検索 | — | ✔ | ○ | P4 | macOS は Accessibility 権限が要る |
| 2.5 | メニューバー項目の検索・実行 | — | ✔ | ○ | P4 | 同上。Linux 等価は無し (グローバルメニューが無い) |
| 2.6 | ウィンドウ管理 (halves/thirds/quarters/display 移動) | ✔ | ✔ (34) | ○ | P4 | Accessibility 権限。Linux は WM 依存で `△` |
| 2.7 | 保存レイアウト | ✔ (Pro) | ✔ | ○ | P4 | 無料 |
| 2.8 | システムアクション (sleep/lock/空にする/外観切替/Bluetooth/…) | ✔ | ✔ (31) | ○ | P3.5 | OS 抽象の trait 裏に置く |
| 2.9 | 入力ソース切替 | — | ✔ | ○ | P4 | macOS: TIS / Linux: ibus・fcitx は要調査 |
| 2.10 | アプリアンインストーラ | — | ✔ | △ | P5 | 削除対象の提示までは安全。実削除はゴミ箱経由のみ |
| 2.11 | Apple Shortcuts 実行 | ✔ | ✔ | ○ | P4 | macOS のみ |
| 2.12 | Focus (集中モード) | ✔ | — | ✕ | — | [§10](#10-non-goals) |

### 4.3 ファイルとエクスプローラ連携 (nohrs の主戦場)

| # | 機能 | Raycast | Tinycast | nohrs | Phase | 備考 |
|---|------|:-------:|:--------:|:-----:|:-----:|------|
| 3.1 | ファイル名検索 | ✔ (Spotlight) | ✔ (Spotlight) | ✔ 実装済 | P3 | **自前インデックス**。Spotlight 非依存 ([ADR 0001](./adr/0001-sqlite-tantivy-hybrid-search.md)) |
| 3.2 | 全文検索 | △ | — | ◎ | P3/P4 | FTS5 → Tantivy ([`search.md`](./search.md)) |
| 3.3 | 検索結果 → explorer で開く | — | — | ◎ | P3 | `Cmd+Enter` = Reveal (実装済) |
| 3.4 | explorer の選択を launcher のコンテキストに渡す | — | — | ◎ | P3.5 | `CommandContext.selected_paths` |
| 3.5 | 最近開いたファイル / フォルダ | ✔ | — | ◎ | P3 | explorer と共有 |
| 3.6 | ブックマーク / ピン留めフォルダ | ✔ | ✔ | ◎ | P3.5 | explorer のサイドバーと共有 |
| 3.7 | ファイルアクション (コピー・移動・改名・ゴミ箱・パスコピー) | △ | △ | ◎ | P3.5 | **explorer の実装をそのまま呼ぶ** ([`explorer-essentials.md`](./explorer-essentials.md)) |
| 3.8 | Quick Look / プレビュー | ✔ | — | ◎ | P3 | 詳細ペイン + OS Quick Look ([`os-integration.md`](./os-integration.md) #6) |
| 3.9 | スクリーンショット内テキスト検索 | ✔ | — | ○ | P5 | OCR ([§5.4](#54-クリップボード履歴)) をファイルにも適用 |
| 3.10 | 長時間ファイル操作の進捗 | — | — | ◎ | P4 | notch のライブアクティビティ ([`notch.md`](./notch.md)) |

### 4.4 クリップボード・テキスト

| # | 機能 | Raycast | Supaste | nohrs | Phase | 備考 |
|---|------|:-------:|:-------:|:-----:|:-----:|------|
| 4.1 | クリップボード履歴 (text/image/file/color/link) | ✔ | ✔ | ○ | **P3.5** | Raycast Free は 3 ヶ月上限。nohrs は**無期限 + 容量上限** |
| 4.2 | 履歴検索 | ✔ | ✔ | ○ | P3.5 | FTS5 を再利用 |
| 4.3 | 元のアプリに貼り戻す | ✔ | ✔ | ○ | P3.5 | Accessibility 権限 + `Cmd+V` 合成 |
| 4.4 | ピン留め / お気に入り | ✔ | ✔ | ○ | P3.5 | |
| 4.5 | アプリ別グループ / 種別フィルタ / カスタムカテゴリ | △ | ✔ | ○ | P4 | |
| 4.6 | 画像・スクショの OCR | ✔ (別機能) | ✔ | ○ | P5 | macOS Vision / Linux は要調査。**ローカル処理のみ** |
| 4.7 | Multi-clip (複数結合) | — | ✔ | ○ | P4 | |
| 4.8 | クリップのリマインダ | — | ✔ | △ | P5 | 通知基盤が要る |
| 4.9 | 機微情報の自動除外 | △ | ✔ | ◎ | **P3.5** | `org.nspasteboard.ConcealedType` / パスワードマネージャ由来 / 正規表現ルール。**初日から必須** |
| 4.10 | スニペット (キーワード展開) | ✔ | ✔ (Inline Shortcuts) | ○ | P3.5 | どこでも展開するには Accessibility 権限 |
| 4.11 | 動的プレースホルダ (`{clipboard}` `{selection}` `{date}` `{uuid}` `{argument}`) | ✔ | — | ○ | P3.5 | スニペット / Quicklink / スクリプトで共通 |
| 4.12 | 絵文字・記号ピッカー | ✔ | — | ○ | P4 | 使用履歴つき |
| 4.13 | 翻訳 | ✔ (Pro) | ✔ (AI) | △ | P5 | ネットワーク必須なのでオプトイン。既定は AI プロバイダ経由 |
| 4.14 | 選択テキストへの Quick Action (校正・要約・書き換え) | ✔ (AI) | ✔ | ○ | P5 | AI 設定時のみ表示 |

### 4.5 コマンド・拡張

| # | 機能 | Raycast | Tinycast | nohrs | Phase | 備考 |
|---|------|:-------:|:--------:|:-----:|:-----:|------|
| 5.1 | Quicklink (URL/ファイル/deeplink + プレースホルダ) | ✔ | ✔ | ○ | P3.5 | |
| 5.2 | スクリプトコマンド (shell/python/…) | ✔ | ✔ | ○ | P4 | メタデータヘッダで宣言。**サンドボックス外**である旨を UI で明示 |
| 5.3 | 計算機 (四則・単位・日付・タイムゾーン) | ✔ | ✔ | ○ | P3 | ローカル完結 |
| 5.4 | 為替・暗号通貨レート | ✔ | ✔ | △ | P5 | ネットワークなのでオプトイン + キャッシュ |
| 5.5 | カレンダー / 次の会議に参加 | ✔ | ✔ | △ | P5 | macOS EventKit。Linux 等価は要調査 |
| 5.6 | 浮遊メモ (Notes) | ✔ | ✔ | ○ | P4 | **プレーン Markdown ファイル**として保存 (Tinycast と同じ。ロックインしない) |
| 5.7 | プラグイン (WASM) | ✔ (Node) | ✔ (JSC) | ◎ | P4 | [`plugin-overview.md`](./plugin-overview.md) |
| 5.8 | **Raycast 拡張の互換** | — | ✔ (実行) | ○ (ソース互換) | P5 | 方式は [`migration.md`](./migration.md) §4。**実行時互換ではなく再ビルド互換**を採る |
| 5.9 | プラグインストア | ✔ | — | ○ | P5 | [`plugin-distribution.md`](./plugin-distribution.md) |
| 5.10 | AI チャット / コマンド | ✔ (Pro) | ✔ (BYOK) | △ | P5 | **BYOK のみ**。nohrs 自身は AI を売らない |
| 5.11 | MCP / エージェント | ✔ | — | △ | Future | ROADMAP Future Work の「AI agent 統合」 |
| 5.12 | 設定の同期 (クラウド) | ✔ (Pro) | — | ✕ | — | [§10](#10-non-goals)。代わりに「同期可能なファイル配置」を保証 |
| 5.13 | バックアップ / 復元 | ✔ | ✔ | ◎ | P3.5 | 平文 JSON。`noh export` / `noh import` |
| 5.14 | チーム共有 / SSO | ✔ | — | ✕ | — | |

### 4.6 notch

詳細は [`notch.md`](./notch.md)。要件としてはここに要約のみ置きます。

| # | 機能 | 参照元 | nohrs | Phase |
|---|------|--------|:-----:|:-----:|
| 6.1 | ファイルシェルフ (DnD 一時置き場) | NotchNook / Supaste | ◎ | P4 |
| 6.2 | explorer ↔ シェルフのドラッグ往復 | — | ◎ | P4 |
| 6.3 | ファイル操作・インデックスの進捗表示 | — | ◎ | P4 |
| 6.4 | クリップボード履歴の覗き見 | Supaste | ○ | P4 |
| 6.5 | Now Playing | 全社 | △ | P5 |
| 6.6 | HUD 置換 (音量・輝度) | Alcove | △ | P5 |
| 6.7 | ウィジェット (カレンダー / 統計 / バッテリー) | NotchNook | △ | P5 |
| 6.8 | 非 notch マシン / Linux でのフォールバック | — | ◎ | P4 |

---

## 5. 機能要件 (詳細)

包含マトリクスのうち、**設計判断を含むもの**だけを詳細化します。単純な「コマンドを 1 つ足す」類は省略。

### 5.1 ルート検索とアクションモデル

**要件**

- ルート検索は次のカテゴリを横断する: `Application` / `File` / `Folder` / `Command` / `Clipboard` / `Snippet` /
  `Quicklink` / `Calculation` / `Plugin` / `Recent`。
- セクション見出しで区切り、**カテゴリ順は使用履歴で学習**する (固定順ではない)。
- 完全一致のエイリアスは常に最上位。
- 結果ゼロのときはフォールバック行を出す (「"foo" を全文検索」「"foo" を翻訳」「"foo" を AI に聞く」)。
  フォールバックの並びはユーザーが設定できる。
- すべてのアイテムは `Vec<Action>` を持ち、`Enter` = primary、`Cmd+Enter` = secondary、`Cmd+K` = 全アクション。

**受け入れ基準**

- 入力 1 文字ごとの再ランキングが **p95 < 50ms** (10 万エントリ・インデックス常駐時)。
- アクションパネルはキーボードだけで完結する (マウス不要)。
- 同じクエリを 3 回実行すると、4 回目に目的のアイテムが 1 位に来る (履歴 boost の体感基準)。

### 5.2 アプリ起動

**要件**

- macOS: `/Applications`, `/System/Applications`, `~/Applications` を走査。`.app` の `Info.plist` から表示名と
  ローカライズ名、アイコンを取得。
- Linux: XDG の `.desktop` (`/usr/share/applications`, `~/.local/share/applications`) を読み、`Name[ja]` 等の
  ローカライズ名も検索対象に含める。
- アプリ一覧は **FS watcher で追従**する (現在のファイル名インデックスは起動時 1 回きり。
  [`launcher.md`](./launcher.md) §13 の既知の制約と同根なので、同じ watcher に相乗りする)。
- 起動中アプリは別セクションで、アクションに「フォーカス」「隠す」「終了」を持つ。

**受け入れ基準**

- アプリのインストール / 削除が **10 秒以内**に検索結果へ反映される。
- 日本語名のアプリ (例: 「ミュージック」) がローマ字入力と日本語入力の両方でヒットする。

### 5.3 コマンドフレームワーク

`launcher.md` §4 の `Command` trait を、次の点で修正して確定させます。

**要件**

- `Command` trait と `inventory` レジストリは **`nohrs-launcher` ではなく共有レイヤ**に置く
  ([§7.1](#71-command-レジストリの置き場所))。explorer / notch / plugin host / CLI が同じレジストリに登録する。
- コマンドは次のメタデータを持つ: `id` / `title` / `subtitle` / `icon` / `keywords` / `category` / `mode` /
  `arguments` / `default_hotkey` / **`required_permissions`** / **`surfaces`** (どのサーフェスに出すか)。
- `mode` は `Instant` / `View` / `External` に加え、**`Background`** (結果を HUD / notch に出すだけで launcher を閉じる) を持つ。
- 実行結果は `CommandResult` で返し、UI 層が描画する。コマンドは GPUI に触らない
  (= plugin と core コマンドが同じ形になる)。

**受け入れ基準**

- core コマンドと plugin コマンドが**同じレジストリ**から列挙され、UI 側に分岐が無い。
- コマンド一覧を CLI (`noh commands --json`) と web (`/docs/commands`) に自動出力できる。

### 5.4 クリップボード履歴

Supaste を包含する、という前提で要件を置きます。**単なる履歴では足りません**。

**取り込み**

- 監視は macOS では `NSPasteboard.changeCount` のポーリング (OS がイベントを出さないため)。
  **間隔 500ms、画面ロック中・スリープ中・バッテリー省電力時は停止**する ([§6](#6-非機能要件) の電力予算)。
- Linux は X11 (`XFixesSelectionNotify`) と Wayland (`wlr-data-control` が使える場合のみ) で分岐。
  使えない環境では「クリップボード履歴は利用できません」と明示し、**黙って劣化させない**。
- 保存する種別: プレーンテキスト / リッチテキスト (元の RTF も保持) / 画像 / ファイル参照 / 色 / URL。
  種別判定は MIME / UTI から行い、テキスト内容の推測 (色コード・URL) は補助的に行う。

**機微情報の除外 (P3.5 で必須)**

- `org.nspasteboard.ConcealedType` (macOS の事実上の標準) が付いたクリップは**保存しない**。
- 送信元アプリが除外リスト (既定: 1Password / Bitwarden / KeePassXC / Keychain Access 等) の場合は保存しない。
- ユーザー定義の正規表現ルールで除外できる (既定で有効な例: よくある API キー形式)。
- 除外されたことは UI で**静かに 1 行だけ**示す (何が起きたか分かるが、内容は出さない)。

**保持と容量**

- 既定: 無期限保持 / 総容量 2GB / 1 アイテム 50MB 上限。超過は古いものから削除 (ピン留めは対象外)。
- 画像はオリジナルを `$XDG_DATA_HOME/nohrs/clipboard/` に blob として置き、SQLite にはメタデータとサムネイル。

**貼り戻し**

- 「元のアプリに貼る」は、launcher を閉じ → 直前のフロントアプリを復帰 → ペーストボードに書き → `Cmd+V` を合成、の順。
  Accessibility 権限が無い場合は「ペーストボードに入れるところまで」にフォールバックし、理由を出す。

**受け入れ基準**

- 常駐時の追加 CPU がアイドルで **< 0.5%**、追加 RSS が **< 30MB** (画像はディスク常駐)。
- パスワードマネージャからコピーしたパスワードが履歴に**一度も**現れない (統合テストで検証)。
- 10 万件の履歴に対して検索が **p95 < 100ms**。

### 5.5 スニペットとテキスト展開

**要件**

- スニペットは Markdown (Tinycast と同じ) + プレースホルダ。保存先は**プレーンファイル** (`$XDG_DATA_HOME/nohrs/snippets/*.md`)
  とし、フロントマターでキーワード・引数を宣言する。DB に閉じ込めない (原則 3)。
- プレースホルダは launcher / Quicklink / スクリプトコマンドで**共通の 1 実装**: `{clipboard}` `{selection}`
  `{date:...}` `{uuid}` `{argument:name}` `{snippet:id}` (入れ子)。
- 展開の発火は 2 経路: (a) launcher から選んで貼り付け、(b) **どのアプリでもキーワード入力で自動展開**。
  (b) はキー入力監視 = Accessibility 権限が要るため**明示的オプトイン**とし、既定オフ。

**受け入れ基準**

- 権限が無い状態でも (a) は完全に動く。
- `.alfredsnippets` / Raycast の snippets JSON をそのまま取り込める ([`migration.md`](./migration.md))。

### 5.6 ウィンドウ管理・メニュー検索・貼り戻し (Accessibility 権限グループ)

4.2 の 2.3-2.6 と 4.4 の 4.3 / 5.5 の (b) は、**すべて macOS の Accessibility 権限**に依存します。要件として:

- 権限は**機能を初めて使うときに**求める (起動時に求めない)。
- 権限が無い場合、該当コマンドは検索結果に出るが、実行すると「この機能には Accessibility 権限が必要です」
  という説明 + システム設定を開くアクションを出す。**黙って失敗しない**。
- 権限グループごとに設定でオフにでき、オフなら該当コマンドを一覧から消せる。
- Linux では Accessibility に相当するものが無いため、ウィンドウ管理は WM 依存 (`wmctrl` 相当の X11 / Wayland は
  コンポジタ次第) として `△` に落とし、メニュー検索は**提供しない**と明示する。

### 5.7 AI

**要件**

- 既定オフ。オンにするとき、ユーザーは **(a) プロバイダ、(b) API キー、(c) 送信されるデータの範囲**を必ず見る。
- 対応は BYOK のみ: OpenAI 互換 API / Anthropic / ローカル (Ollama / llama.cpp) / macOS の Apple Intelligence。
  nohrs 自身がクレジットを売ることはしない (原則 1)。
- AI 機能の単位は Raycast と同じ 3 つに絞る: **Quick AI** (ルート検索からその場で聞く) / **AI Chat** /
  **Quick Action** (選択テキストへの校正・要約・翻訳・書き換え)。
- 送信前に「何を送るか」のプレビューを出せるデバッグモードを持つ。

**受け入れ基準**

- AI を有効化していないビルド / 設定では、AI 関連のネットワーク接続が**一切発生しない** (テストで検証)。

### 5.8 設定・バックアップ

**要件**

- ランチャー関連の設定は `config.toml` の `[launcher]` / `[clipboard]` / `[snippets]` / `[notch]` / `[ai]` に置く
  ([`config.md`](./config.md) のスキーマに追加)。高頻度更新 (ウィンドウ位置・使用履歴) は redb / SQLite。
- `noh export --all` で **1 つの平文 zip** (JSON + blob) を吐く。`noh import` はその逆。
- Raycast と同様の**スケジュールバックアップ**を持つが、既定オフ・保存先はローカルのみ。

---

## 6. 非機能要件

| 観点 | 予算 | 測り方 |
|------|------|--------|
| **ホットキー → 表示** | < 100ms (p95) | 既存 ROADMAP の完了条件を維持 |
| **キー入力 → 結果更新** | < 50ms (p95) | 10 万エントリで計測 |
| **全文検索** | < 500ms (中央値) | [`search.md`](./search.md) |
| **常駐アイドル CPU** | < 1% (クリップボード監視込み) | 60 秒平均、`powermetrics` / `top` |
| **常駐 RSS** | < 150MB (インデックス 10 万件時) | Tinycast が 100MB 未満を掲げているので競合水準 |
| **バッテリー時** | インデクサ 1 スレッド以下、クリップボード監視は間隔 2 倍 | [`search.md`](./search.md) §7 のマトリクスを拡張 |
| **起動 (cold)** | < 1s でホットキー受付開始 | インデックスは非同期で後から |
| **プライバシー** | 既定で外向き通信ゼロ | 統合テストでソケット生成を監視 |
| **権限** | 機能単位で遅延要求、未許可でも劣化動作 | [§5.6](#56-ウィンドウ管理メニュー検索貼り戻し-accessibility-権限グループ) |
| **アクセシビリティ** | VoiceOver で結果リストを読める / コントラスト比 4.5:1 | 手動チェックリスト |
| **IME** | 変換中は ↑↓/Enter を IME に譲る (実装済)。**日本語・中国語・韓国語で実機確認** | [`launcher.md`](./launcher.md) §13 の未検証項目を解消する |
| **i18n** | UI 文字列は P6 の `fluent-rs` 前提で外出しできる形に (今は英語ハードコードでも、埋め込み方を固定する) | |

---

## 7. アーキテクチャへの影響

### 7.1 `Command` レジストリの置き場所

**問題**: [`launcher.md`](./launcher.md) §4 は `crates/nohrs-launcher/src/command.rs` に `Command` trait を置くと
書いていますが、[`architecture.md`](./architecture.md) §2 は「横方向の参照 (例: `pages` から `launcher`) は避ける。
必要なら `services` か `store` で trait 定義して両者から依存」と定めています。explorer (pages) や notch が
コマンドを提供した瞬間に、この 2 つは矛盾します。

**提案**: `Command` trait / `CommandContext` / `CommandResult` / `Action` / `ArgSpec` / `inventory` レジストリを
**`nohrs-services::command`** (または新 crate `nohrs-command`) に置く。

```text
          pages        launcher        notch        plugin-host
            └──────────────┴──────┬──────┴──────────────┘
                                  ▼
                        services::command  (Command trait + registry)
```

`view-node` 相当の描画データ型は `nohrs-models` に置き、UI 層 (`nohrs-ui`) がそれを描く責務を持つ。
これで **core コマンドと WASM plugin コマンドが同じ型で流れ、UI に分岐が無くなる**。

### 7.2 ホットキーレジストリ

現在の `hotkey.rs` は「summon キー 1 つ」を前提にしています。per-app hotkey (2.3)、コマンド単位ホットキー (1.5)、
Hyper Key 相当を載せると、**プロセス全体で N 個のホットキーを登録・衝突検出・再バインド**する必要があります。

**提案**: `hotkey` モジュールを `nohrs-launcher` から昇格させ、`KeyChord` 型・登録レジストリ・衝突検出・
Wayland portal 経路をまとめて持つ。macOS/X11/Windows の passive grab と Wayland portal の 2 経路の分岐は
そのまま維持 ([`launcher.md`](./launcher.md) §13)。**Wayland では portal のバインド数に上限がある可能性**が
あるため、コマンド単位ホットキーは Wayland で `△` になり得る (要実機検証)。

### 7.3 OS 抽象レイヤ

ウィンドウ管理・メニュー検索・貼り戻し・入力ソース・通知・notch は、いずれも **OS ネイティブ API** を必要とします。
[`os-integration.md`](./os-integration.md) §8 は「OS 固有コードは `nohrs` (アプリ層) に閉じ込める」としていますが、
これらは**コマンドから呼ばれる**ため、アプリ層に閉じ込めると `services → nohrs` の逆流が起きます。

**提案 (ADR 候補)**: `nohrs-platform` crate を新設し、`services` と同じ層に置く。

- 公開するのは trait と安全な型のみ (`WindowManager`, `Accessibility`, `Pasteboard`, `AppCatalog`, `NotchGeometry`)。
- `cfg(target_os)` で実装を切り替え、未対応 OS は `Err(Unsupported)` を返す (`unimplemented!()` は使わない)。
- **`unsafe` の扱い**: workspace は `unsafe_code = "deny"` です ([`architecture.md`](./architecture.md) §5)。
  objc2 系の `msg_send!` は `unsafe` なので、この crate だけ例外にするか、安全なラッパ crate に限定するかを
  決める必要があります ([§9](#9-決定が必要な論点-adr-候補))。

### 7.4 常駐プロセスと権限

クリップボード履歴・スニペット自動展開・per-app hotkey・notch は**常駐**を前提にします。現状 Linux では
「1px の keep-alive ウィンドウ」でプロセスを生かしています ([`launcher.md`](./launcher.md) §13)。

**要件**:
- 常駐モード (`nohrs --background` / ログイン項目) を第一級にし、explorer ウィンドウを閉じても死なない。
- macOS は `LSUIElement` + メニューバーアイコン (ROADMAP Future Work の「menubar 常駐モード」を **P4 に前倒し**)。
- 常駐時に何が動いているか (インデクサ / クリップボード監視 / ホットキー) を 1 画面で見られ、個別に止められる。

### 7.5 永続化スキーマの追加

[`persistence.md`](./persistence.md) に次を足す (P3.5)。

```sql
-- クリップボード履歴
CREATE TABLE clips (
    id            INTEGER PRIMARY KEY,
    kind          TEXT NOT NULL,      -- "text" | "rtf" | "image" | "file" | "color" | "url"
    preview       TEXT,               -- 一覧表示用の短いテキスト
    body          BLOB,               -- 小さい値は inline
    blob_path     TEXT,               -- 大きい値は外部ファイル
    source_app    TEXT,               -- bundle id / .desktop id
    source_title  TEXT,
    byte_size     INTEGER NOT NULL,
    pinned        INTEGER NOT NULL DEFAULT 0,
    category_id   INTEGER,
    copied_at     INTEGER NOT NULL
);
CREATE INDEX idx_clips_time ON clips(copied_at DESC);
CREATE VIRTUAL TABLE clips_fts USING fts5(preview, ocr_text, content='clips');

-- コマンド使用履歴 (既存 history テーブルの kind="command" を昇格)
CREATE TABLE command_usage (
    command_id   TEXT PRIMARY KEY,
    use_count    INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL
);
```

redb 側 (`state.redb`) には `launcher.window_position` / `notch.*` / `launcher.favorites` を
`kv_key!` の名前空間規約に従って追加します。

### 7.6 WIT (plugin API) への影響

Raycast 互換と、そもそもの対話的なプラグイン UI のために、`commands` interface を **1 往復から
セッション + イベント**に拡張する必要があります。詳細な WIT スケッチは [`migration.md`](./migration.md) §4.3。

---

## 8. フェーズ計画 (ROADMAP 改訂案)

現行 ROADMAP の P3 は「ランチャー + 検索」で 1 つですが、本書の範囲は P3 に収まりません。
**P3 を分割**し、notch とクリップボードを P4 に織り込む案:

| Phase | 版 | 追加する内容 |
|-------|----|------------|
| **P3** (現行の範囲を維持) | `0.2.0` | コマンドフレームワーク (5.3) / アプリ起動 (5.2) / 計算機 / 詳細ペイン / push-pop / 履歴 boost / 検索 V2 |
| **P3.5** (新規) | `0.2.x` | クリップボード履歴 + 機微情報除外 (5.4) / スニペット + プレースホルダ (5.5) / Quicklink / エイリアス / コマンド単位ホットキー / per-app hotkey / システムアクション / **移行ウィザード v1** ([`migration.md`](./migration.md)) / `noh export`・`import` |
| **P4** | `0.3.0` | plugin host (現行どおり) + **WIT のセッション/イベント拡張** / notch v1 (シェルフ・進捗・クリップ覗き見) / 常駐モードとメニューバー / ウィンドウ管理・メニュー検索 (Accessibility 群) / Notes / 絵文字 |
| **P5** | `0.4.0` | プラグインストア (現行どおり) + **Raycast 拡張ソース互換 SDK** / OCR / AI (BYOK) / notch v2 (メディア・HUD・ウィジェット) / テーマ |
| **P6** | `0.5.0` | 性能ゲートに **常駐アイドル CPU / RSS** を追加 / Linux での劣化マトリクスを文書化 |

> **なぜ P3.5 を切るか**: クリップボードとスニペットは「ランチャーを毎日使う理由」そのもので、
> plugin host (P4) を待つ必然性がない。一方で notch は常駐モデルが固まってからでないと作れないので P4。

---

## 9. 決定が必要な論点 (ADR 候補)

| # | 論点 | 選択肢 | 本書の推奨 |
|---|------|--------|-----------|
| **D1** | `Command` レジストリの置き場所 | (a) `nohrs-launcher` のまま / (b) `nohrs-services::command` / (c) 新 crate `nohrs-command` | **(b)**。crate 数を増やさず、依存の向きを守れる ([§7.1](#71-command-レジストリの置き場所)) |
| **D2** | OS 固有能力の置き場所 | (a) アプリ層に閉じ込め (現行 os-integration.md) / (b) `nohrs-platform` crate | **(b)**。コマンドから呼ぶ以上、アプリ層では逆流する |
| **D3** | `unsafe_code = deny` の例外 | (a) `nohrs-platform` のみ crate 単位で許可 + 安全性コメント必須 / (b) 安全ラッパ crate だけ使い例外を作らない / (c) 別プロセスのヘルパーに隔離 | **(a)**。(b) は objc2 の現実と合わず、(c) は複雑すぎる。ただし**許可は 1 crate のみ**に限定し、CI で他 crate への波及を検査 |
| **D4** | Raycast 拡張互換の方式 | (a) JS エンジン組み込みで実行時互換 (Tinycast 方式) / (b) `@raycast/api` 互換 shim + jco componentize でソース互換 / (c) 非対応 | **(b)**。[ADR 0005](./adr/0005-wit-bindgen-component-model.md) と矛盾せず、tokio 禁止 ([ADR 0004](./adr/0004-remove-tokio.md)) とも衝突しない。詳細は [`migration.md`](./migration.md) §4 |
| **D5** | クリップボード監視のポーリング | (a) 500ms 固定 / (b) 状態適応 (前面/背面/バッテリー/ロック) | **(b)**。[`search.md`](./search.md) §7 のリソースポリシーに相乗りする |
| **D6** | notch の実装単位 | (a) `nohrs-launcher` に間借り / (b) 新 crate `nohrs-notch` | **(b)**。別ウィンドウ・別ライフサイクル・別入力モデルで、launcher と共有するのはコマンドレジストリだけ |
| **D7** | AI の立ち位置 | (a) BYOK のみ / (b) nohrs がプロキシして課金 | **(a)**。原則 1 |
| **D8** | 同期 | (a) 提供しない (ファイル配置だけ同期可能にする) / (b) 自前クラウド | **(a)**。ROADMAP Future Work の「クラウド統合」に委ねる |

---

## 10. Non-goals

「作らない」ことを明示しておくもの。要望が来たときはここを指す。

| 項目 | 理由 |
|------|------|
| **独自クラウド同期・アカウント** | 原則 1。代わりに全データを iCloud Drive / Dropbox / git に置ける形にする |
| **チーム共有・SSO・MDM** | OSS 単体で背負うにはガバナンス要件が重すぎる。必要なら別レイヤ |
| **AI クレジットの販売** | 原則 1 |
| **Focus (集中モード・アプリブロック)** | OS 側の Focus / Screen Time と競合し、常駐の権限要求が増える割に Launcher × Explorer の軸から遠い |
| **ゲーム / Raycast Wrapped 的な遊び** | 優先度が低い。plugin でやれる |
| **Raycast 拡張のバイナリ互換実行** | D4。Node ランタイムを同梱すると「依存ゼロ・サンドボックス」の 2 原則を同時に壊す |
| **ブラウザ拡張 / iOS アプリ** | 対象外 (ROADMAP に無い) |
| **メニュー検索の Linux 対応** | グローバルメニューという概念が無い環境が大半。無いものを無いと言う |

---

## 11. 未解決 / 要調査

| # | 内容 | 影響 |
|---|------|------|
| R1 | gpui 0.2 に **ウィンドウレベル・非アクティブ化パネル (NSPanel `nonactivatingPanel`)** の公開 API があるか | notch と「フォーカスを奪わない launcher」の実現性 ([`notch.md`](./notch.md) §5) |
| R2 | Wayland portal の GlobalShortcuts が**何個までバインドできるか** | コマンド単位ホットキーの Linux 可否 ([§7.2](#72-ホットキーレジストリ)) |
| R3 | Linux のクリップボード監視 (Wayland `wlr-data-control` の普及度) | 4.1 の Linux 可否 |
| R4 | macOS の Vision framework を `unsafe` 最小で呼べるか | OCR (4.6) |
| R5 | Raycast の `.rayconfig` の暗号化形式 | 移行の網羅度 ([`migration.md`](./migration.md) §2) |
| R6 | Supaste / Paste / Maccy のローカル保存形式 | クリップボード移行 ([`migration.md`](./migration.md) §5) |
</content>
</invoke>
