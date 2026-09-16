# Notch — 3 つ目のサーフェス

> Status: Draft (要件・設計。実装は P4 以降)
> Related: [`launcher-requirements.md`](./launcher-requirements.md) / [`launcher.md`](./launcher.md) /
> [`explorer-essentials.md`](./explorer-essentials.md) / [`os-integration.md`](./os-integration.md) / [`migration.md`](./migration.md)

本書は MacBook の **notch (カメラハウジング周辺)** を nohrs の 3 つ目のサーフェスとして扱うための要件と設計を定めます。
explorer window / launcher window に続く常駐サーフェスで、**「今なにが起きているか」と「一時的に物を置く場所」**を担当します。

---

## 1. なぜ notch を第一級にするのか

notch アプリ (NotchNook / Boring Notch / Alcove など) は現在、**ランチャーとは別プロセスの 4 つ目の常駐アプリ**として
存在しています。nohrs は既に

- 常駐してファイルインデックスを持ち ([`search.md`](./search.md))、
- ファイル操作を実行し ([`explorer-essentials.md`](./explorer-essentials.md))、
- クリップボード履歴を持ち ([`launcher-requirements.md`](./launcher-requirements.md) §5.4)、
- ドラッグ & ドロップの送り手と受け手の両方である

ため、**notch に出したいものをすでに全部持っています**。別プロセスを増やさずに出せるのは構造的な優位です。

逆に言えば、nohrs の notch は「メディアコントロールの置き場所」ではなく、
**Explorer × Launcher の常駐面**として設計します。他社の notch アプリが持てないのは次の 3 つです。

1. **シェルフと explorer が同じファイル操作基盤を共有する** (シェルフから explorer へ、explorer からシェルフへ)。
2. **長時間のファイル操作 / インデックス作成の進捗が出る** (コピー・移動・ゴミ箱・プラグイン導入)。
3. **クリップボード履歴とシェルフが地続き** (Supaste の "notch shelf" が単体で売れている領域)。

---

## 2. サーフェスモデル

notch は 4 つの状態を持ちます。状態遷移はすべて**ポインタとドラッグだけ**で完結し、キーボードは補助です。

| 状態 | 見え方 | 入り方 | 出方 |
|------|--------|--------|------|
| **Hidden** | 何も描かない (物理 notch のまま) | 既定 / 設定で無効化時 | — |
| **Idle (pill)** | notch の左右にごく小さいインジケータ | 常駐中で、通知すべきものがあるとき | 数秒で Hidden に戻る (設定可) |
| **Hover** | notch がひと回り広がり、要約 1 行 + 主要ボタン | ポインタが notch 領域に入る | ポインタが離れる |
| **Open** | パネルが下に展開。タブでシェルフ / クリップ / アクティビティ / ウィジェット | クリック / ファイルをドラッグして notch に近づける / ホットキー | `Esc` / 外側クリック / ドロップ完了 |

**ドラッグ中の特別扱い**: ファイルをドラッグしたまま画面上端に近づけると、Hover を飛ばして **Open のシェルフタブ**に
直行します (NotchNook / Supaste と同じ体験)。これが notch の最頻ユースケースなので、他のどの遷移よりも速くする。

### 2.1 非 notch マシン / Linux でのフォールバック

notch が無い Mac (Air 2020, 外部ディスプレイ, Mac mini) と Linux でも**同じ機能が使える**必要があります。
無いものを理由に機能ごと落とすと、クロスプラットフォーム原則 (原則 5) が崩れます。

| 環境 | 形態 |
|------|------|
| notch あり (内蔵ディスプレイ) | 物理 notch に重ねる |
| notch なし / 外部ディスプレイ | 画面上端中央に**同じ形の浮遊バー**を出す (Idle 時は数 px の細い帯) |
| Linux (X11) | 同上。`_NET_WM_WINDOW_TYPE_DOCK` + `_NET_WM_STATE_ABOVE` |
| Linux (Wayland) | `wlr-layer-shell` が使えるコンポジタでのみ同等。使えなければ **notch サーフェスを提供しない**と明示し、シェルフは launcher 内のタブとして出す |

**設計上の帰結**: シェルフ / クリップ / アクティビティの**中身のビューは notch 専用にしない**。
launcher のタブとしても、explorer のサイドバーパネルとしても同じビューを出せるようにする
(= `nohrs-ui` のコンポーネントとして作り、`nohrs-notch` はそれを載せる器に徹する)。

---

## 3. 機能要件

### 3.1 ファイルシェルフ (P4, 最優先)

**要件**

- 任意のアプリからドラッグしたファイル / テキスト / 画像を**一時的に保持**する。実体は移動せず、パス参照
  (アプリ外から来た生データのみ `$XDG_DATA_HOME/nohrs/shelf/` に退避)。
- シェルフからのドラッグアウトは、**元アプリが期待する形** (`NSFilenamesPboardType` / `text/uri-list`) で出す。
- アクション: explorer で開く / Reveal / まとめてコピー・移動 (explorer のファイル操作基盤を呼ぶ) / AirDrop 送信 (macOS) /
  クリップボードにパスをコピー / 削除 (シェルフからだけ外す) / まとめて zip。
- **explorer 側との往復**: explorer の選択を `Cmd+Shift+S` 等でシェルフに送れる。シェルフから explorer の
  現在ディレクトリにドロップできる。
- シェルフの内容は再起動を跨いで復元する (redb `shelf.items`)。ただし存在しなくなったパスは復元時に落とす。

**受け入れ基準**

- ドラッグ開始から notch が Open になるまで **< 150ms**。
- 100 件のファイルをシェルフに入れても一覧のスクロールが 60fps を維持する。
- シェルフ経由の移動 / コピーは explorer と**同一の** conflict 解決 UI (Rename/Overwrite/Skip + Apply to all) を使う。

### 3.2 ライブアクティビティ (P4)

nohrs 自身が抱えている長時間処理を出します。**他アプリの監視はしません** (原則 1: 常駐は軽く)。

| 種類 | 出すもの |
|------|---------|
| ファイル操作 | コピー / 移動 / ゴミ箱 / 復元 の進捗、対象数、残り時間、キャンセル |
| インデックス | 初回スキャン / 再インデックスの進捗、一時停止 |
| プラグイン | インストール / 更新の進捗 (P4 以降) |
| 検索 | 長い全文検索の実行中表示 (中断可能) |
| OS 由来 (P5) | 充電開始 / AirDrop 受信 / 音量・輝度 (§3.4) |

**受け入れ基準**: アクティビティが無いときは**何も描かず、タイマーも回さない** (アイドル CPU 0 に落ちる)。

### 3.3 クリップボード覗き見 (P4)

- 直近 N 件 (既定 10) を notch から一覧・貼り付け。Supaste の `Ctrl+Cmd+0-9` に相当する番号アクセスを持つ。
- 機微情報の除外ルールは launcher 側と**同一実装**を使う ([`launcher-requirements.md`](./launcher-requirements.md) §5.4)。
- notch では**中身のプレビューを既定で伏せる** (画面共有中に履歴が映る事故を避ける)。ホバーで開く。

### 3.4 OS 表示の置き換え (P5, オプトイン)

| 機能 | 既定 | 備考 |
|------|------|------|
| Now Playing (再生中 + 操作) | オフ | macOS の `MediaRemote` は非公開 API。**公開手段がない場合は実装しない** (要調査 R7) |
| 音量・輝度 HUD の置換 | オフ | OS の HUD を消す方法が公開 API に無いなら「重ねて出す」に留める |
| バッテリー / 充電 | オフ | |
| カレンダーの次の予定 | オフ | EventKit 権限 |
| システム統計 (CPU / メモリ / ネット) | オフ | 常時ポーリングになるため、**開いているときだけ**更新する |

**原則**: このカテゴリは全部オフで出荷する。notch が「勝手に何かを表示する場所」になると、原則 1 (無音) が崩れます。

### 3.5 提供しないもの

| 項目 | 理由 |
|------|------|
| 通知のインライン返信 / 音声返信 | 通知の傍受は macOS で公開 API が無く、権限も重い |
| カメラミラー / ポモドーロ / 健康系ウィジェット | Launcher × Explorer の軸から遠い。plugin でやれる |
| CI / PR 監視 | 同上。plugin (`nohrs-plugin-git` 系) の領域 |
| テーマ / GIF / 触覚フィードバックの作り込み | P5 のテーマ機構に相乗りする範囲に留める |

---

## 4. ジェスチャとキー

| 操作 | 動作 |
|------|------|
| ホバー | Idle → Hover |
| クリック | Open (最後に見ていたタブ) |
| ファイルをドラッグして上端へ | Open (シェルフタブ) |
| 下スワイプ / 二本指スクロール↓ | Open |
| 上スワイプ | Close |
| `Cmd+Shift+N` (既定、変更可) | Open / Close トグル |
| `Esc` | Close |
| `Tab` / `Shift+Tab` | タブ切り替え |
| `1`-`9` | クリップ / シェルフの n 番目を実行 |

ホットキーは launcher と**同じホットキーレジストリ**を使います ([`launcher-requirements.md`](./launcher-requirements.md) §7.2)。

---

## 5. 実装方式

### 5.1 ウィンドウ

| 観点 | macOS | Linux |
|------|-------|-------|
| ウィンドウ種別 | `NSPanel` + `.nonactivatingPanel` (フォーカスを奪わない) | X11: override-redirect / `_NET_WM_WINDOW_TYPE_DOCK`。Wayland: `wlr-layer-shell` の `top` レイヤ |
| レベル | `.floating` 以上 (メニューバーより上) | `_NET_WM_STATE_ABOVE` |
| スペース | `.canJoinAllSpaces` + `.fullScreenAuxiliary` (全画面アプリの上でも出る) | コンポジタ依存 |
| 背景 | 透過 + 角丸。notch の「黒」と継ぎ目なく繋ぐ | 同左 (コンポジタが無ければ不透明) |
| 入力 | Open 時のみクリックを受ける。Idle / Hover 時は**ホバー判定のみ**でクリックは透過させたい | 同左 |

**重要な未解決点 (R1)**: gpui 0.2 の `WindowOptions` は `window_bounds` / `kind` (`Normal` / `PopUp` / `Floating`) /
`window_background` / `window_decorations` を持ちますが、**ウィンドウレベルの直接指定・非アクティブ化パネル・
クリック透過・全スペース表示の公開 API は見当たりません**。launcher も同じ制約を受けていて、
「Wayland の activation token を渡せない」問題が既に出ています ([`launcher.md`](./launcher.md) §13)。

取り得る道は 3 つ:

| 案 | 内容 | 評価 |
|----|------|------|
| **A** | gpui に upstream で API を足す (`WindowOptions::level`, `nonactivating`, `mouse_passthrough`) | 本筋。ROADMAP Future Work の「GPUI コミュニティへの貢献」と整合。ただしマージ待ちが読めない |
| **B** | `nohrs-platform` ([`launcher-requirements.md`](./launcher-requirements.md) §7.3) から、gpui が作った `NSWindow` を取得して属性を後付けする | 実現可能だが gpui 内部表現に依存し、版上げで壊れる |
| **C** | notch 専用ウィンドウだけ gpui の外で作る | 描画基盤が二重になる。却下 |

**推奨は A を目標に B で先行**。B のコードは `nohrs-platform` の 1 モジュールに閉じ込め、
「gpui に API が入ったら消す」ことを doc comment に明記する。

### 5.2 notch の幾何

- macOS: `NSScreen.safeAreaInsets.top > 0` で notch の有無を判定し、`auxiliaryTopLeftArea` /
  `auxiliaryTopRightArea` から notch の幅・高さを求める (機種ごとの数値をハードコードしない)。
- ディスプレイ構成変更 (外部接続 / 解像度変更 / スケーリング変更) を監視して再配置する。
- 内蔵ディスプレイが閉じている (クラムシェル) ときは、フォールバックの浮遊バーに切り替える。
- どのディスプレイに出すかは設定 (既定: 内蔵 → 無ければメインディスプレイ)。

### 5.3 crate 配置

```text
crates/nohrs-notch/          # 新規 (D6)
├── src/nohrs_notch.rs       # サーフェスのライフサイクル、状態機械
├── src/window.rs            # ウィンドウ生成・幾何・ディスプレイ追従
├── src/shelf.rs             # シェルフのモデル (ビューは nohrs-ui)
└── src/activity.rs          # ライブアクティビティの購読
```

依存は `nohrs-ui` / `nohrs-services` / `nohrs-store` / `nohrs-platform` / `nohrs-core` の下向きのみ。
**`nohrs-launcher` と `nohrs-pages` は参照しません** ([`architecture.md`](./architecture.md) §2 の依存ルール)。
共有するのはコマンドレジストリ (`services::command`) と UI コンポーネント (`nohrs-ui`) だけです。

### 5.4 ライブアクティビティの購読

進捗は「ファイル操作サービスが `Progress` イベントを流し、notch が購読する」形にします。
explorer も同じイベントを購読して自分のステータスバーを更新するので、**進捗の実装は 1 つ**で済みます。

```text
services::fs::ops  ──(async-channel / postage broadcast)──┬──▶ pages::explorer  (ステータスバー)
                                                          └──▶ notch::activity  (ライブアクティビティ)
```

---

## 6. 非機能要件

| 観点 | 予算 |
|------|------|
| Idle (アクティビティ無し) の CPU | **0%** (タイマーを回さない。イベント駆動のみ) |
| Hover → 描画 | < 50ms |
| ドラッグ検知 → Open | < 150ms |
| 追加 RSS | < 25MB (シェルフのサムネイル込み) |
| ディスプレイ構成変更への追従 | < 1s |
| 画面収録権限 | **不要**。必要になる機能は作らない |
| フルスクリーン動画の上に出る | 出せること。ただし**動画再生中は Idle を抑制**する設定を持つ |

---

## 7. 設定 (`config.toml` 追加分)

```toml
[notch]
enabled = true
display = "builtin"        # "builtin" | "main" | "all"
fallback_bar = true        # notch が無い環境で浮遊バーを出すか
idle_indicator = true      # Idle (pill) を出すか
open_on_drag = true        # ドラッグで上端に近づけたら開くか
hotkey = "Cmd+Shift+N"

[notch.tabs]
shelf = true
clipboard = true
activity = true
widgets = false            # P5

[notch.shelf]
persist = true             # 再起動を跨いで復元するか
max_items = 100
```

---

## 8. フェーズ割当

| Phase | 範囲 |
|-------|------|
| **P4** | サーフェス基盤 (状態機械・ウィンドウ・幾何・フォールバック) / シェルフ / ライブアクティビティ / クリップボード覗き見 / 設定 |
| **P5** | ウィジェット (カレンダー・バッテリー・統計) / Now Playing・HUD 置換 (公開 API で可能な範囲) / テーマ連携 |
| **P6** | 性能ゲート (Idle CPU 0%) の CI 計測 / Linux の対応状況マトリクス確定 |

---

## 9. 未解決 / 要調査

| # | 内容 |
|---|------|
| R1 | gpui 0.2 でウィンドウレベル / 非アクティブ化パネル / クリック透過をどこまで表現できるか (§5.1) |
| R7 | Now Playing を**公開 API だけ**で取れるか (macOS の `MediaRemote` は非公開)。取れないなら §3.4 から落とす |
| R8 | `wlr-layer-shell` を gpui のウィンドウ生成経路から使えるか。使えないなら Linux は launcher タブ一本に倒す |
| R9 | Idle 時のクリック透過が無いと、notch 上のメニューバー操作を奪ってしまう。回避策 (領域を小さく保つ / Idle を既定オフ) の要否 |
| R10 | 複数ディスプレイでのドラッグ検知 (ポインタがどの画面の上端にいるか) の判定コスト |
</content>
</invoke>
