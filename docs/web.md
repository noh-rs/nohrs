# Web — nohrs.app & noh.rs

> Status: Active (P1 で **フルスコープ・production-grade** で立ち上げ、後続 Phase でデータを充実)
> Related: [`ROADMAP.md`](./ROADMAP.md), [ADR 0006 (monorepo-web)](./adr/0006-monorepo-web.md), [ADR 0007 (cloudflare-hosting)](./adr/0007-cloudflare-hosting.md), [ADR 0008 (web-design-system)](./adr/0008-web-design-system.md)

本書は nohrs の web 公開面 (`nohrs.app` + `noh.rs`) の構成・ホスティング・コンテンツ運用を定めます。

## 0. スコープ方針 (重要)

当初 P1 は「web MVP (landing + redirect + blog/docs skeleton)」だったが、**P1 から本格的・production-grade で立ち上げる**方針に変更した (issue #55 を re-scope)。

- **見た目・構造はフル完成**: デザイン DNA は **zed.dev** を土台に、Vercel (タイポグラフィ規律) / Cursor (製品デモの見せ方) をアクセントとして借りる。詳細は [ADR 0008](./adr/0008-web-design-system.md) と §2.5。
- **機能スコープもフル**: blog 本格機能 (コメント / RSS / OG 自動生成) を P2 から **P1 に前倒し**。Plugin Store / コマンド一覧など本体未実装に依存するページは、**シードデータ + "Coming soon / Preview" 状態**で器を作り込み、バックエンド (P3–P5) が揃い次第データを差し込む。
- **品質基準**: a11y (WCAG AA) / パフォーマンス予算 (Lighthouse 95+) / フル SEO (sitemap・hreflang・OG/Twitter meta・JSON-LD) をローンチ条件に含める。
- **デリバリ**: M1 (顔) → M2 (知識) → M3 (動的) → M4 (インフラ) の段階的本番デプロイ。各 M で preview→本番が回る。マイルストーン詳細は issue #55 のサブイシュー (M1–M4) を参照。

---

## 1. ドメイン構成

| ドメイン | 役割 | 取得状況 |
|---------|------|---------|
| **`nohrs.app`** | 正式サイト (ランディング・ダウンロード・docs・blog・release・plugin store) | 取得済 |
| **`noh.rs`** | 短縮 URL / SNS 共有用 / CLI/開発者向け導線。すべて 301 で `nohrs.app/<path>` にリダイレクト + 一部短縮スキーム | 取得済 |

### noh.rs リダイレクト仕様 (Cloudflare Workers)

- `noh.rs/<any>` → `nohrs.app/<any>` に **path 保持で 301**
- 短縮スキーム:
  - `noh.rs/p/<plugin-id>` → `nohrs.app/<lang>/plugins/<plugin-id>` (**302 + `Vary`**。言語を判定するため)
  - `noh.rs/r/<tag>` → **GitHub の release ページ** (`github.com/noh-rs/nohrs/releases/tag/<tag>`) に 301。
    サイトには releases の一覧はあるが**個別 release のページが無い**ため、`nohrs.app/<lang>/releases/<tag>`
    に送ると 404 になる。個別ページを作った時点でこちらに切り替える
- **HTTPS 強制は Worker で行う** (`workers/site.ts`)。カスタムドメインは HTTP を自動でリダイレクトしないため、
  ダッシュボードの "Always Use HTTPS" に頼らずコード側で 301 する。スキームとホストの補正は 1 回の 301 にまとめる
- HSTS preload は P5 以降に検討

---

## 2. 技術スタック

| 項目 | 採用 |
|------|------|
| Framework | TanStack Start + Vite+ |
| ホスティング | **Cloudflare Workers + assets binding** (実装 2026-09-08。ADR 0007 の Pages から変更。理由は下記) |
| レンダリング | **全ページをビルド時に prerender** して静的配信。SSR は残すが、リクエストごとに決まるのは正規ホストと `/` の言語振り分けだけなので Worker 1 本で足りる |
| デプロイ | `main` ブランチ → 本番。加えて**週次で再ビルド**する (star 数・コミット・release はビルド時に読むため) |
| スタイリング | **Tailwind v4 + CSS 変数デザイントークン** (ライト/ダーク・warm neutral・tan アクセントを変数で一元管理) |
| コンポーネント | **shadcn / Radix headless プリミティブを取り込み、自前トークンで再スキン** (a11y を担保しつつ汎用 LP 感を回避) |
| フォント | self-host (Google Fonts 直リンクは禁止)。ラテン=`Inter` + `JetBrains Mono`、和文=`Noto Sans JP`。詳細 §2.5。**ビルド時にダウンロードして `public/fonts/` に置き、リポジトリには入れない** (Noto Sans JP はウェイトごとに約 120 のサブセットファイルになるため) |
| モーション | 控えめ・意味のある動きのみ (CSS 主体、一部 Motion)。`prefers-reduced-motion` 必須対応 |
| ドキュメント検索 | **Pagefind** (ビルド時に静的インデックス生成、CJK セグメンテーション内蔵で和文 docs も対応) |
| 分析 | **Cloudflare Web Analytics** (cookie 不要、cookie banner 不要) |
| OG 画像 | Satori で自動生成。**Worker ではなくビルド時に生成する** (入力はビルド時に確定しており、エッジでラスタライザを動かして毎回同じ画像を作る理由がない)。**P1 から有効** |
| コメント (blog) | GitHub Discussions を**保管場所として使い、描画は自前**。Worker が API で読み、サイトの組みで出す。**P1 から有効** |
| RSS / Atom | 両方提供 (`/<lang>/blog/rss.xml`, `/<lang>/blog/atom.xml`)、言語別。**ルートではなくビルドスクリプトで出力する** (全ページ prerender のため、フィードのためだけにサーバを残す理由がない)。**P1 から有効** |

ホスティングを Pages から Workers + assets binding に変えたのは、**成果物を 1 つにするため**。
Pages プロジェクトと別 Worker の 2 つをデプロイして両者のルーティングを合わせる必要がなくなり、
言語振り分けと正規ホストの規則がリポジトリ内のコードとして残る。ADR 0007 の判断根拠
(Cloudflare にエコシステムを一本化する・無料枠が広い) は変わらない。

R2 は他用途でも使用:
- `coverage.nohrs.app` (PR ごとの HTML カバレッジレポートを保管。詳細は [`docs/testing.md`](./testing.md))
- (将来) blog 画像・plugin アイコン CDN

---

## 2.5 デザインシステム

> 決定の根拠と棄却案は [ADR 0008 (web-design-system)](./adr/0008-web-design-system.md) を参照。

### 北極星

**zed.dev を土台 (DNA)** とする。職人的・エディトリアルなトーンが nohrs (Rust 製のパワーユーザ向けツール) の製品性格に最も合う。残り 2 つはアクセントとして部分採用:

- **Vercel から**: タイポグラフィの規律、mono ラベル、コードブロック表現
- **Cursor から**: ヒーローの製品デモ (GIF/動画) の見せ方

3 つを対等に混ぜず、1 つを土台・2 つを調味料にすることで一貫性を担保する。

### カラー

- **ライト主 / ダーク従** (トグルで切替。アプリ本体が `BG=WHITE` のライトテーマなのでブランド一致)
- ニュートラルは **warm 寄り** (純グレーでなく僅かに暖色。tan アクセントと調和)
- **アクセント = Rust tan `#DEA584`**。これはアプリ本体 (`src/ui/theme.rs` の `ACCENT`) かつ Rust 言語色であり、「Rust 製」アイデンティティと暖色を兼ねるブランドカラー。青系は web では使わずブランドを tan に一本化する。
  - 注: アプリ側 `theme.rs` は `ACCENT` のコメントが "Blue" と誤記され `ACCENT_HOVER`/`ACCENT_LIGHT` が青系のまま残っている。web を tan に一本化するのに合わせ、アプリ側のブランド統一は別 issue で扱う。

### タイポグラフィ

| 用途 | フォント方針 |
|------|------|
| 見出し / 本文 (ラテン) | `Inter` |
| アクセント / コード / ラベル | `JetBrains Mono` — zed/vercel 共通の「mono ラベル」が本格感の鍵 |
| 和文 | `Noto Sans JP` (改訂 2026-09-08。当初案は `Zen Kaku Gothic New`) |

全フォントを **Cloudflare に self-host** (FOUT・GDPR・edge 遅延の回避)。和文は必ずサブセット化する (Noto Sans JP はフルセットが重いため)。

### モーション

- スクロール連動の控えめな reveal + 繊細な hover のみ
- 3D/パララックス、および**背景全面を覆う**グラデーションは封印 (マーケ LP 化を避け職人トーンを維持)
- ただし **輪郭を持つ 1 個の形は可** (改訂 2026-09-08)。全面の滲みは「汚れ」に見えるが、円に閉じた orb は「意図して置かれた物体」として読める。境界は面か形かであって、シェーダーを使うか否かではない
  - **fluid orb は右の余白からリングの中心へ移した** (改訂 2026-09-09)。一度撤去したが、当時の濃度 (`0.46`) のまま中心に置いたのが原因で文字の背後の滲みに見えていた。**resting を薄く (`--orb-strength` 0.24 / ダーク 0.22) し、ポインタが中に入ったときだけ flare を立てる**と、リングが回る対象として成立する
- **要素そのものを動かす演出も封印** (改訂 2026-09-08)。ホバーで吸い付くボタン、回転するグリフの類は、罫線と余白で組む他セクションと語彙が食い違い、ページ全体の品位を下げる。動かしてよいのは「状態が変わったこと」を伝える場合に限る
  - **Hero のリングだけは例外** (改訂 2026-09-09)。ホバーでのせり出しも、開くときの回転も、「どのパネルを指しているか」「開いたものがどこから来たか」という状態を伝えている。装飾として動くものは他に置かない
- 実装は CSS 主体、オーケストレーションが要る所のみ軽量に Motion。`prefers-reduced-motion` 対応必須
- 基準値: `220ms` / `cubic-bezier(.16,1,.3,1)` / `translateY(6px)` / stagger `50ms`
- **スクロールスナップは使わない** (改訂 2026-09-08)。セクションごとの吸着 → 上端下端の 2 点だけ → 全廃、と 2 段階で差し戻した。`proximity` でも吸着点の近傍でホイールが引っ張られ、`scroll-behavior: smooth` を `html` に置くとキーボード / スクロールバー操作まで再タイミングされる。**ホイールの感触はブラウザに完全に任せる**。アンカー遷移のイージングは、クリックハンドラ側で `scrollIntoView({ behavior: "smooth" })` を呼んで与える (`prefers-reduced-motion` では `"auto"`)。`scroll-padding-top` は sticky ヘッダにアンカー先が潜らないよう残す
- **`overscroll-behavior: none` を `html` に置く**。ページ端でのラバーバンドを止め、オーバースクロールのジェスチャがブラウザ側 (pull-to-refresh・戻るスワイプ) に連鎖しないようにする。**端より先で起きることを変えるだけで、ホイールの感触自体には触らない**

### 品質基準 (ローンチ条件)

- **a11y**: WCAG AA・完全キーボード操作・コントラスト・reduced-motion
- **パフォーマンス**: Lighthouse 95+・edge SSR・画像最適化・font self-host で CLS 抑制
- **SEO**: sitemap・`hreflang` 自動生成・OG/Twitter meta・JSON-LD 構造化データ・canonical

---

## 3. ディレクトリ構成

```text
web/
├── package.json
├── vite.config.ts             # prerender 対象と sitemap の hreflang をここで組む
├── wrangler.jsonc             # nohrs.app (Worker + assets binding)
├── app/                       # TanStack Start app
│   ├── routes/
│   │   ├── __root.tsx
│   │   ├── index.tsx          # `/` — 言語振り分け (Worker が先に応答する。ここは fallback)
│   │   ├── $lang.tsx          # /en/... or /ja/... のレイアウト
│   │   ├── $lang/index.tsx    # landing
│   │   ├── $lang/about.tsx
│   │   ├── $lang/download.tsx
│   │   ├── $lang/roadmap.tsx
│   │   ├── $lang/releases.tsx
│   │   ├── $lang/blog/{index,$slug}.tsx
│   │   ├── $lang/docs.tsx     # サイドバーのレイアウト
│   │   ├── $lang/docs/{index,$slug}.tsx
│   │   └── $lang/plugins/{index,$id}.tsx
│   ├── components/            # Header / Footer / Section / Phases / HeroOrbit / FluidOrb / DocsSearch / Comments …
│   ├── data/github.json       # API が使えないときのフォールバック (コミット済)
│   ├── lib/
│   │   ├── content.ts         # mdx loader + plugin レジストリ
│   │   ├── discussion.ts      # コメントスレッドの取得と整形。Worker からも import する
│   │   ├── github.ts          # ビルド時に取得した GitHub のスナップショット
│   │   ├── negotiate.ts       # 言語判定。Worker からも import する (依存ゼロ)
│   │   ├── seo.ts             # canonical / hreflang / OG / JSON-LD
│   │   └── strings.ts         # UI 文言。`ja: typeof en` で対訳漏れを型エラーにする
│   └── styles/app.css         # トークン (3 状態テーマ) + コンポーネント層
├── content/
│   ├── en/{blog,docs}/*.mdx
│   ├── ja/{blog,docs}/*.mdx
│   └── plugins/<plugin-id>.toml
├── scripts/                   # fetch-fonts / fetch-github / build-og / build-feeds
│                              # + vite-plugin-discussion (`/api/discussion` を dev でも出す)
├── public/
│   └── shots/                 # Hero のリングに出す実在の画面 (demo.gif から抜いたフレーム)
└── workers/
    ├── site.ts                # 静的配信 + 正規ホスト + `/` の言語振り分け + `/api/discussion`
    ├── workers.test.ts        # 両 Worker のリダイレクトと `/api/discussion` の単体テスト (`npm test`)
    ├── noh-rs-redirect.ts     # noh.rs リダイレクト Worker
    └── wrangler.noh-rs.jsonc
```

**`prebuild` の 3 本 (`fetch-fonts` / `fetch-github` / `build-og`) は失敗してもビルドを止めない。**
ネットワークが無い環境では、システムフォント・コミット済みの GitHub スナップショット・OG 画像なしに
縮退する。欠けているのは装飾と鮮度であって、壊れたデプロイを出すより安全なため。

一方 **`build-feeds` と `pagefind` は失敗したらビルドを落とす**。こちらはページの中身そのもの
(フィードと検索インデックス) を作る工程で、黙って欠けると壊れたサイトを配信することになる。

---

## 4. i18n

| 観点 | 仕様 |
|------|------|
| ルーティング | **パス前置** (`/en/...` / `/ja/...`) |
| canonical 言語 | **`en`** (国際的リーチ優先) |
| `/` (ルート) アクセス | Worker 層で `Accept-Language` を見て振り分け、初回振り分け後 Cookie で記憶 |
| ローンチ時パリティ | **全ページ完全バイリンガル (en + ja)** で公開する。フォールバックは将来のコンテンツ差分救済用 |
| 翻訳手段 | **AI 全自動翻訳** を基本とする。ただし Hero タグライン + メーカーズノートだけは最終的に軽い人力推敲を推奨 |
| 翻訳欠落時 (ローンチ後の新規・差分コンテンツ) | en にフォールバック、UI で "翻訳募集中" バナーを表示。**ローンチ時点では全ページ en/ja が揃っているため発生しない**。新規記事追加〜翻訳完了の間など、ローンチ後の一時的な差分のみを救済する仕組み |
| canonical 上書き | blog 記事 frontmatter `canonical: ja` で例外可 (著者が ja で書いた場合) |
| `hreflang` | ビルド時に自動生成 |

---

## 5. コンテンツの場所 (リポジトリとの関係)

| カテゴリ | 場所 | 役割 |
|---------|------|------|
| 開発者・コントリビュータ向け docs | `nohrs/docs/` (Rust 本体リポジトリ内) | アーキテクチャ、ADR、WIT spec、permission モデル |
| エンドユーザ向け docs | `web/content/<lang>/docs/` | インストール手順、操作ガイド、plugin 作成チュートリアル |
| blog | `web/content/<lang>/blog/` | リリースアナウンス、技術記事 |
| release page | (動的) GitHub API + frontmatter | 一覧は自前 SSG、本文クリックで GitHub へ |
| plugin store | `web/content/plugins/<id>.toml` + 動的 enrich | PR ベース登録、ビルド時に GitHub API で metadata 取得 |
| README 翻訳 | `nohrs/docs/README.ja.md` (維持) | リポジトリ訪問者向け |

---

## 6. ページ仕様

### 6.0 グローバルナビ / フッタ (zed.dev 構造ベース、OSS 向けに調整)

zed.dev の IA から商用要素 (Pricing / Business / Sign up / Jobs / Team / Merch) を除いたものを採用。

- **トップナビ**: Docs · Blog · Plugins · Releases · 検索 · **主 CTA** · 言語切替 · テーマ切替
  - 主 CTA は **リリースの有無で切り替える** (改訂 2026-09-08)。公開 release が 0 件の間は `Star on GitHub`、初回 release 以降は `Download`。GitHub API から取得する release 件数で分岐させ、pre-alpha 中に「押しても何も無い」導線を作らない
  - **Features (landing 内アンカー) はトップナビに置かない** (実装 2026-09-08)。全ページに出るナビからランディング内のアンカーへ飛ばすのは行き先が一貫しない。項目数も、和文ラベル + 検索 + トグル 2 つ + CTA を 1440px の 1 行に収める上限が 4 だった
  - 1024px 未満ではリンク行をバーの下に折り返す。ドロワーは作らない (4 項目のために 2 つ目のナビゲーションモデルを維持する価値がない)
- **フッタ (zed 風 4 列)** — 実装に合わせて改訂 (2026-09-08):
  - Product: Download · Releases · Plugins · Roadmap
  - Resources: Docs · Blog · Discussions
  - Project: About · Contributing · License
  - Social: GitHub · Discord · X
  - **同じ宛先を 2 列に置かない** (実装 2026-09-08)。当初案では Docs が Product と Resources に、
    Blog が Resources と Project に、GitHub が Product と Social に重複していた。宛先ごとに列を 1 つ決める
  - **存在しないページを列に書かない** (実装 2026-09-08)。FAQ / Privacy / Brand は「(将来)」付きで
    載っていたが、リンク先が無い項目はフッタに置きようがない。作ってから足す

### 6.1 `/` (ランディング)

ページ全体像 (上から下のスクロール、zed.dev のホーム構成を nohrs 流に):

1. **Hero**: 5 枚の画面をリング状に置き、その中心に tagline (大タイポ) + サブコピー + 主 CTA を置く (**改訂 2026-09-09**。それまでは「製品スクショは Hero に置かない」= 大タイポのみだった)。実装は `components/HeroOrbit.tsx` + `.orbit*` (app.css)
   - tagline は **`Launcher × Explorer`** で確定。`×` のみ mono + Rust tan で組み、他は Inter。リポジトリ description の冒頭と一致させる
   - サブコピーは事実のみ 1〜2 行 (何であるか・何で書かれているか・ライセンス)。バッジや煽り文句を足さない
   - **背景は無地**。ステータスバッジの類は置かない。サブコピーで述べるのは「何であるか・何で書かれているか・ライセンス」で、開発段階の自己申告は書かない (改訂 2026-09-10。それまでは pre-alpha であることをここで述べていた)
   - **リングに置くのは実在の画面だけ** (改訂 2026-09-09)。素材は `assets/doc/demo.gif` から抜いたフレーム (`public/shots/*.png`、900×549、英語ロケール) で、Explorer / Preview / Search / Matches / Source の 5 状態。**Launcher・Plugin のパネルは作らない** — 本体が未実装であり、§6.1 の正直主義はスクショを Hero に出しても変わらない
   - **縦は 1 つの単位 (ステージ高) から引く**。パネルとタイポの大きさもこれに従う — 幅だけから引くとノート PC の縦で破綻する。一方 **横半径 (`--rx`) はウィンドウの半幅に追従させる**。これは意図で、どの幅でも左右のパネルが縁で切れるようにするため。狭い幅と縦長のウィンドウでは幾何そのものを差し替える (下記)
     - **パネルの中心はステージの外に置く**。画面に出るのは各パネルの 4〜6 割で、残りは画面の外にある。`--rx` はウィンドウの半幅に追従させ、どの幅でも左右のパネルが縁で切れるようにする
     - リングは楕円ではなく**少し角を張った超楕円** (指数 3)。楕円だと斜めのパネルだけが画面内に丸ごと浮き、矩形まで振ると逆に角へ埋まって細片になる
     - 名前は**独立した小さなリング**に乗せる。パネルに付けて追従させると窓の縁まで連れて行かれる。ただしリングにも超楕円の張りを掛ける — 下の 2 枚が CTA ボタンの角を横切るため。縦半径は、ホバーで滑り込んでくる上のパネルより内側に収める
     - パネルも名前も同じ角度で回す。開くときはその角度が 0 に戻る回転として見える。角度は (-180, 180] に畳む (288° は逆回りで 72°)
     - パネルはカードの中で拡大して**細部を見せる**。900px のウィンドウを 400px に縮めると白い矩形にしかならない。開くと同じ点を軸にズームが戻り、ウィンドウ全体が現れる
     - 切り抜き枠は**画像の上端より上にはみ出してよい** (`SPILL`)。パネルの上端は必ず切られていて画面に出ないためで、はみ出た空白は見えない。アプリの中身はウィンドウ上部に集まっているので、この余裕がないと「下半分の空白」しか見せられなくなる
   - **開いたパネルは最終サイズで組み、カードの上に縮小して置く** (改訂 2026-09-09)。逆 (カードのサイズから拡大) にすると、読む側が見る状態の罫線と角丸が倍率ぶん太る
   - **飛行は CSS transition ではなくキーフレーム (`element.animate()`) で書く** (改訂 2026-09-09)。transition の開始値は「終了値が変わった瞬間にエンジンが解決していた値」であり、**その瞬間がいつかはエンジンごとに違う**。この 1 点で同じパネルが 2 回壊れた:
     - Chromium: カードの位置へ移す中間ステップが transition を消費してしまい、開くアニメーションはその続きから始まって何も動かない。`transition: none` を 1 フレームだけ当てて回避していた
     - WebKit (実機の Safari): その回避が効かず、**パネルが最終サイズで一度描かれ、その場で少し縮んでから元に戻る**。カードから飛んでくる動きは一度も走らない。閉じるときも transition が走らず、パネルはカードの位置へ瞬間移動してフォールバックのタイマー (700ms) が来るまで居座る — これが「タップのあとパネルが出たまま残る」の正体で、せり出し (下記) とは別の原因
     - キーフレームは両端を明示するので、解決のタイミングに依存する余地が無い。**測定用の不可視フレーム (`measuring` フェーズ) も要らなくなり**、`transitionend` の bubbling も reduced-motion 用のタイマーも消えた。CSS 側に残るのは「読む側が最後に見る状態」だけで、JS が動かないページでは開いたパネルがそのまま出る
     - **カードに合わせるのはパネルの箱ではなく `.orbit-shot`** (改訂 2026-09-10)。パネルは「スクショ + その下のキャプション」なので、箱の中心はスクショの中心より下にある。パネルの中心をカードの中心へ運ぶと、**最後に残る絵だけが 12〜40px ずれた場所に着地して入れ替わる**。往きは絵が小さい状態から始まるので気づかないが、帰りは終端で見るため出る。ずれ量はレイアウトの定数 (`shot.offsetTop + shot.offsetHeight / 2 - node.offsetHeight / 2`) を `k` 倍し、パネルと同じ角度で回して translate に足す (回転は translate の後に掛かるため)。**検証はパネルの矩形ではなくスクショの矩形で測る** — パネルで測ると、この補正が入った状態が「ずれている」と読めてしまう
     - **帰りのイージングは往きの使い回しにしない** (改訂 2026-09-10)。`EASE` も `--ease` も「到着するための曲線」= 緩やかな立ち上がりと長い減速で、減速の尾が帰りの終端に来ると**カードの数 px 手前でほぼ静止したまま入れ替わる** (実測: 最後の 93ms が 256px 中 2px)。かといって単純に反転すると欠陥が先頭に移り、**タップ後 90ms 何も起きない**。両端が浅い曲線を別に持つ (`BACK_EASE`)。判定は距離/時間のプロファイルで行う: 時間 15% で距離 8% 以上 (頭が死んでいない)、70% で 88% 以下 (早着していない)、最後の 15% で 4% 以上 (尾が死んでいない)
     - **帰りのスクリムはパネルと同じ長さにする** (改訂 2026-09-10)。暗転は「開いている状態」に属するので、パネルより先に終わらせない。往きの 380ms を帰りにも使うと 1/3 の地点で 0.03 まで落ち、**戻り切っていないパネルだけが、もう戻ったページの上を飛ぶ**
     - **帰りは `reverse()` せず、長さだけを往きから取る** (改訂 2026-09-10)。`reverse()` は自分のキーフレームを巻き戻すので、開いている間にウィンドウがリサイズされると**動く前のカードの位置へ着地する** (実測 181px ずれ)。かといって毎回 620ms を掛けると、開いた直後に閉じたパネルは残り 2% の距離を 620ms かけて這う。**着地点は閉じる瞬間に測り直し、長さは往きが再生し終えた割合を掛ける** — `reverse()` と同じ尺で、着地はずれない (実測: 62ms で閉じると帰りは 62ms、着地 0px)。スクリム・キャプションの尺にも同じ割合を掛ける。でないと本体が着地して unmount した後もスクリムがフェード途中で消える
   - **検証は WebKit でも行う** (改訂 2026-09-09)。上記の 2 個目は Chromium のモバイルエミュレーションでは再現せず、実機で報告されて初めて分かった。`npx playwright install webkit` + `npx playwright install-deps webkit` で Linux でも **Playwright の WebKit** が動く。ただしこれは Safari そのものではない (コーデック・フォント・GPU 合成は OS 側に依存し、ITP など Apple 固有の統合も入っていない) ので、**Safari 固有の挙動は macOS の Safari か実機で確かめる**。ここで捕まえられるのはレイアウトと JS とアニメーションの、エンジン共通の部分。アニメーションは `getAnimations()` を `pause()` してから `effect.getKeyframes()` を読む — ヘッドレスの WebKit は rAF を数百 ms 止めることがあり、フレーム単位のサンプリングは信用できない
   - **押せることは静止状態で示す** (改訂 2026-09-10)。ページの中のスクショは「絵」として読まれ、コントロールには見えない。カードの縁取り・せり出し・ポインタの形はどれも**すでにポインタを乗せた人にしか届かず**、タッチにはそもそも hover が無い。実機で「クリックできることがわかりづらい」と報告されたのはこれ
     - **印はカードのローカル下端に置く** (`.orbit-open`)。どのパネルも外を向いて縁で切られるので、画面に残るのは必ずその側。形は**円**にする — 角丸のカードに切り取られず、どの角度でも傾いて見えない唯一の形。中のグリフには `--a` の逆回転を掛ける (±144° の 2 枚が逆さになるため)
     - **加えて中央に 1 行だけ文で言う** (文言は `hero.hint`、組みは `.orbit-hint`)。印は「この 1 枚が押せる」、文は「まわりの画面はどれも押せる」で、役割が違う。組みは他の傍注と同じ mono ラベル (11px・muted) にして、タグラインと競わせない
     - 印はカードの側にあり、開いたパネルには無い。パネルは着地したカードをちょうど覆うので、印が出るのは unmount の瞬間 — そこはページ全体が戻ってくる瞬間でもあり、印だけが飛び出して見えることはない
   - ホバーでパネルが中心方向へ 56px せり出す。`prefers-reduced-motion` では全部即時
     - **せり出しは「離れられるポインタ」だけに答える** (改訂 2026-09-09)。`@media (hover: hover) and (pointer: fine)` で囲う。`:hover` はタッチだとタップした要素に残り、`:focus-within` は閉じたときに元のカードへフォーカスを戻す実装と噛み合う。両方を無条件に効かせていたため、**タップ 1 回ごとにパネル 1 枚がコピーの上に 56px 出たまま残っていた**
     - **フォーカスには枠線と outline で答える**。`:focus-visible` の判定はエンジンごとにヒューリスティックが違い、プログラム的な `.focus()` を含めるかどうかも一致しない。**判定が外れても位置が動かないもの** (罫線の色・ページ共通の outline) に割り当て、レイアウトを動かすせり出しの側は推測に依存させない
   - **狭い幅でもリングは組む** (改訂 2026-09-09。当初は 1024px 未満を横スクロールの帯にしていた)。ただし**縦長のウィンドウ (`max-aspect-ratio: 5/4`) と 640px 未満ではリングを立てる**: ステージを縦に伸ばし、パネルは上下の縁から入り、左右はコピーの脇を細く通る。横長の値をそのまま使うとリングがウィンドウより広くなり、左右のパネルは角しか画面に残らない (768×1024 で実測 1,500px²。他が 20,000px² 前後)。逆に幅から引くとリングがコピーを囲い込み、コピーの置き場所が無くなる (中央に置けるのは 200×214px 程度、という計算になる)
     - **パネルの大きさは px でも上限を掛ける** (`min(66vw, 320px)`)。幅からだけ引くと、この範囲の広い側でパネルがリングより大きくなり、隣同士が接触する (639px で実測)
     - **下側の 2 枚だけ外へ出す** (`[data-low] { --rx: 70vw }`)。リング上でいちばん近いのはこの 2 枚で、他と同じ半径だと画面下の中央で角が交差し、2 枚が 1 つの楔に見える
     - **640px 未満では名前を出さない**。中央のコピーの角とパネルの内側の縁の間に、名前が乗る帯が残らない。パネル自体がボタンであり、名前は開いたときのキャプションに出る
     - パネルの中心はどの幅でも画面外にある。**タップの当たり判定は画面に出ている部分だけ** (`overflow: clip` は切り落とした部分をヒットテストからも外す)。E2E で座標を打つときは中心ではなく見えている縁を狙う
     - **重なりは目視で判断せず測る**。各パネルの回転後の 4 隅を transform 行列から出し、ステージの可視矩形でクリップして、総当たりで交差面積を出す。ついでに可視面積も出せば「1 枚だけ極端に小さい」も同時に見つかる。375〜2560px の 16 サイズで確認する
   - リングの中心には **fluid orb** を置く (改訂 2026-09-09。それまでは右側の余白)。WebGL の circle 内で domain-warped fbm を流す。参照は [rareui FluidOrb](https://www.rareui.com/components/fluidorb)。実装条件:
     - 濃度は 2 つのトークンで持つ。**resting = `--orb-strength`** (ライト `0.24` / ダーク `0.22`)、**flare の上限 = `--orb-bloom`** (ライト `0.16` / ダーク `0.16`。シェーダは `strength + bloom` を flare 色に使う)
     - **`--orb-bloom` の上限はコントラストで決まる**。タグラインとサブコピーがこの上に乗るため、ダークは地色から離せる余地が少ない。実測 (1440×800, en): ライト flare 時 h1 `10.7` / サブ `6.4`、ダーク flare 時 h1 `7.5` / サブ `5.1` (AA = 4.5)。**変えたら測り直す** — テキストを隠したスクリーンショットの画素から出せる
     - **ホバーで flare が立つ** (`u_bloom` を 0→1 に ease、上りを下りより速く)。流れが速くなり、warp が深くなり、同じ warp から明るい舌が伸びる。**別の形を上に重ねない** — 明るくなるのは流体そのもの
     - ポインタの判定は中央のコピー (`.orbit-core`) に付ける。パネルはパネル自身の応答 (せり出し) を持つ
     - `IntersectionObserver` で画面外なら rAF を止める。タブ非表示でも止める。解像度は DPR 2 倍で上限 460px
     - `prefers-reduced-motion` では静止 1 フレームのみ。ホバーは**アニメーションなしの 1 回の再描画**として反映し、離れたら resting のフレームに完全に戻る。WebGL が使えなければ canvas ごと削除する
     - 1080px 未満では非表示 (リングがコピーの周りまで詰まっており、背後に置く余地がない)
   - **バッジ・タグ・中黒区切りを禁止する** (改訂 2026-09-08)。`Pre-alpha · macOS · MIT` のような属性の羅列、枠線付きの小ラベル、見出し上のカテゴリタグは使わない。伝えるべき属性は本文の文として書くか、罫線で区切った行に落とす
   - CTA の下に罫線を挟んで **build from source のコマンド** を置く (改訂 2026-09-08)。公開 release が 0 件の間、「では今どう試すのか」に答える導線がページ上に存在しないため。release が出たら、このブロックは `/download` へのリンクに差し替える
   - **正直主義の対象は「載せる証拠」であって「文章の語り口」ではない** (改訂 2026-09-10)。リングに出すのは実在する画面のみ、Launcher/Plugins のパネルは**偽装せず mock も作らない** — ここは変えない。一方で**文章は完成した製品として書く**: サイトは「ある程度完成してから世に出す」前提で公開するので、pre-alpha の自己申告 (「まだ pre-alpha」「まだユーザーがいません」「スクショが無いので載せていません」) は置かず、製品として何であるかを書く
     - **実物に従わせるもの**: スクリーンショット (実際にビルドできるアプリのもののみ)、ロードマップ各フェーズの状態、リリース一覧、GitHub の数字。これらはデータであり、**公開前に実物へ合わせて更新する**。文章だけを先に完成形にしてある
     - **正直であることと、正直さを宣言することは別** (改訂 2026-09-10)。「モックではなく実際のスクショです」「推薦の言葉もロゴの列も置きません」「下の数字はビルド時に読んでいます」の類は、**疑われている前提で喋る文**であって、製品サイトには載らない。方針はコードとこのドキュメントに書き、**ページには製品のことだけを書く**
       - 判定は「同じ文を、完成した競合製品のサイトに置けるか」。置けないなら、それはサイトが自分の振る舞いを説明している文
       - **サイトの仕組みを訪問者に説明しない**。ビルド時に取得している・同じ一覧を読んでいる・ここに何が並ぶ予定か、はこちらの都合であって読者の関心ではない
     - **ただし「今できること」を名乗る文は別** (改訂 2026-09-10)。製品が何であるかを述べる文 (tagline・`hero.sub`・`why` の 4 本柱・`note`) は完成形で書いてよいが、読者が**手元で今できることとして読む**文 — docs の `getting-started`、`plugins` の `installHint` — では、**未実装のランチャー (P3) とプラグインホスト (P4) を「これから入るもの」として書く**。判定基準は「読んだ人がこれから 5 分でそれを試そうとするか」。ロードマップが両者を `planned` と表示している以上、docs だけが現在形で語ると自サイト内で矛盾する
     - 併せて `preview` セクションは「未実装の一覧」から**アプリの説明**に変わった (`upcoming` → `parts`。P ラベルは 01/02/03 に置き換え)。docs の `getting-started` / `plugin-authoring` の callout、`plugins` の `previewNotice` と `installHint`、`releases` / `download` の空状態も同じ方針で書き直した
     - Preview セクションのスクショはまだ **ja ロケールの静止画** のままなので **en で撮り直す** (README 約束の操作 GIF も後日)
   - **スクショに額装をしない** (改訂 2026-09-08)。スクショには実物の macOS ウィンドウ (信号ボタン・角丸・影) が既に写っているため、外側にウィンドウクロームを模した枠・タイトルバー・影を重ねると二重になる。画像をそのまま置き、キャプションを罫線で受ける。撮影時に背景を含めて整えるのが正しい対処であり、web 側で飾って補うのは誤り
2. **"Why nohrs?"** — 3-4 ポイントで差別化 (Launcher first-class / Explorer first-class / WASM plugins / Spotlight 非依存の検索。README の柱を流用)
3. **アプリケーションの説明** (改訂 2026-09-10)。実在するスクショ 1 枚 + 機能行 3 つで組む。**"Coming" の mock カードは作らない** — 当初は Launcher・Plugin・Search をそれで見せる案だったが、上の「実在する画面しか出さない」と正面から矛盾する
4. **Built in Rust / craft セクション** (tan ブランド・性能の語り。zed の care & craftsmanship 相当)
5. **OSS 透明性 = 社会的証明の置換** (pre-alpha でユーザがいないため testimonials は作らない):
   - live GitHub シグナル (star 数・最近のコミット activity feed・contributors) — zed の activity feed の nohrs 版
   - **メーカーズノート** (「なぜ nohrs を作るのか」。zed の team-letter 相当、個人/初期プロジェクトの信頼構築)
6. **Roadmap ティーザー** (P1–P6) + `/roadmap` リンク
7. **Community** (Discord / X / GitHub)
8. **最終 CTA** (Download)

### 6.2 `/releases`

- GitHub API (`/repos/noh-rs/nohrs/releases`) からビルド時に取得
- 一覧表示 (バージョン・日付・ハイライト一行)
- 各カードクリックで GitHub の release URL に遷移
- macOS バイナリの直接ダウンロードリンク (release asset 経由)
- 主要 release は frontmatter で `highlight: true` を付けて目立たせる
- **週次再ビルドは GitHub Actions の `schedule`** で行う (`.github/workflows/web.yml`、月曜 06:00 UTC)。
  latest release を追従するため。Pages の cron は使わない (§7)

### 6.3 `/blog`

- MDX (`web/content/<lang>/blog/<slug>.mdx`)
- frontmatter: title / date / author / tags / canonical / og_image
- カスタムコンポーネント: `<Callout>`, `<Screenshot>`, `<CodeTabs>`, `<YouTube>`
- タグページ (`/blog/tags/<tag>`)、年別アーカイブ (`/blog/2026/`)
- コメント (GitHub Discussions を自前で描画。下記)
- RSS / Atom feed (言語別)

**コメントは「保管場所」と「見た目」を分ける** (改訂 2026-09-10)。当初は giscus を埋め込んでいたが、giscus は **iframe** であり、中身は giscus.app のドキュメントなので `--paper`・`--tan`・`--mono` が一切届かない。渡せるのはテーマ名 1 つだけで、結果としてページの中に GitHub のサイトが埋まっている状態になっていた。設定では直らないので iframe をやめた。

- **保管は GitHub Discussions のまま**。モデレーション・通報・スパム処理・通知メールを GitHub 側に置いたままにでき、訪問者の名前もアドレスもこちらには保存されない。完全自前 (D1) にすると、これを全部自分で持つことになる
- **スレッドの同定は giscus の `specific` マッピングを踏襲**し、記事 1 本につき `blog/<slug>` という**タイトルのディスカッション 1 つ**。giscus 時代に書かれたスレッドがそのまま読める
- **読み取りは Worker の `/api/discussion?term=blog/<slug>`**。GraphQL の検索はランキングであって一致ではない (`blog/nohrs` は `blog/nohrs-and-gpui` も返す) ので、**タイトルの完全一致で選び直す**
  - `term` は `THREAD_TERM` で検証してから検索文字列に埋める。ここが**インジェクション境界**であって、単なる入力チェックではない
  - **エッジキャッシュ 60 秒**が事実上のレートリミッタ。人気記事でも GitHub への呼び出しは 1 分に 1 回
  - トークンは **Worker シークレット** (`GITHUB_TOKEN`)。ビルド時に HTML へ焼かれる `VITE_*` 系とは種類が違う。CI が `DISCUSSIONS_TOKEN` (repo 単位・read-only・Discussions のみの fine-grained PAT) を deploy 後に押し込む
  - 未設定なら 503。fork では giscus 時代と同じく**黙って GitHub へのリンクに落ちる**
- **本文は GitHub が返す `bodyHTML` をそのまま入れる**。GitHub 側でサニタイズ済みで、giscus の iframe が出していたものと同一。ここで生 Markdown を描くと、パーサとサニタイザを自前で持つことになる
- **リンクは全状態で描く**。prerender される静止状態 (`idle`) では読み込み中の文言を出さない — JavaScript が無い読者を、来ない fetch の前で待たせないため
- 取得は `IntersectionObserver` で**セクションが近づいてから**。記事はコメントより手前で閉じられる方が多い
- アバターは出さず、名前は mono。リアクションは絵文字を `aria-hidden` にせず、読み上げに任せる (文字自身の名前が読まれるので、こちらで書くラベルより正確)
- OG 画像: Satori で frontmatter から自動生成

### 6.4 `/docs`

- MDX (`web/content/<lang>/docs/<slug>.mdx`)
- 左サイドバーにナビゲーション、右に見出し toc
- Pagefind 検索 (`Ctrl+K` でモーダル起動)
- カテゴリ: Getting Started / Usage / Plugin Authoring / API Reference

### 6.5 `/plugins` (Plugin Store)

詳細は [`docs/plugin-distribution.md`](./plugin-distribution.md) §Plugin Store を参照。

要点:
- `web/content/plugins/<id>.toml` に最小情報 (repo / category / tags) を PR で登録
- ビルド時に GitHub API で stars / last commit / README / license / `plugin.toml` を fetch して enrich
- 5 カテゴリ (productivity / developer-tools / media / cloud / theme)
- 各カードに permission バッジ
- Install ボタン: `nohrs://install?source=user/repo` で deeplink
  - **プラグインホスト (P4) が入るまでは `<a>` にしない** (改訂 2026-09-10)。スキームを登録しているものが無い状態でアンカーにすると、押しても何も起きない死んだコントロールになる。アドレスとして読める `<span>` で出し、`installHint` 側で「P4 で入る」と述べる。ホストが入ったらアンカーに戻す
- **P1 ではシードの `<id>.toml` 数件**でカード/グリッド/カテゴリの器を作り込む。**器に "Coming soon" とは書かない** (改訂 2026-09-10)。ページ上部の注記はレジストリがまだ小さいことを述べるにとどめ、登録の増減で嘘にならない文にする

### 6.6 `/about`

- プロジェクトの物語・哲学・values (zed.dev の信頼感の源。実装が安く効果が高い)
- **メーカーズノート** (landing と共有可。なぜ nohrs を作るのか)
- 注: Hero タグライン + メーカーズノートは翻訳が硬くなりやすいため、AI 訳でも最終的に軽い人力推敲を推奨

### 6.7 `/download`

- OSS 最重要 CTA。macOS バイナリ (release asset) · build from source 手順 · システム要件を集約
- **release の有無はデータで分岐させる** (`hasDownloads`)。無い間は「ソースからビルドする」を主導線にし、出たら `/releases` と連動する。開発段階そのものの説明は書かない (改訂 2026-09-10)

### 6.8 `/roadmap`

- 既存 [`docs/ROADMAP.md`](./ROADMAP.md) を web 化。P1–P6 のフェーズと進捗を可視化
- zed.dev も Roadmap を持つ。透明なロードマップは §6.1 の社会的証明置換の一部

---

## 7. ビルド・デプロイ

### ビルドの流れ

```text
npm run build
├── prebuild
│   ├── fetch-fonts.mjs    → public/fonts/          self-host する woff2 と @font-face CSS
│   ├── fetch-github.mjs   → app/data/…generated    star / release / 直近コミット
│   └── build-og.mjs       → public/og/             OG 画像 (Satori)
├── vite build             → dist/client/           全ページ prerender + sitemap.xml
├── build-feeds.mjs        → dist/client/<lang>/blog/{rss,atom}.xml
└── pagefind               → dist/client/_pagefind/ docs 検索インデックス
```

### CI (GitHub Actions)

- ワークフローは `.github/workflows/web.yml`。`paths: web/**` で Rust の CI と分離する (ADR 0006)
- PR → typecheck + build + prerender 出力の存在チェック
- `main` への push → `wrangler deploy` で nohrs.app と noh.rs の両 Worker を更新
- **週次 (月曜 06:00 UTC) に再ビルド + デプロイ**。star 数・コミット・release はビルド時に読むため、
  再ビルドしない限り公開サイトはリポジトリに追従しない

### 環境変数 (GitHub Actions secrets)

**影響範囲で置き場所を分ける**。

| 変数 | 置き場所 | 用途 | 無いとどうなるか |
|------|---------|------|-----------------|
| `CLOUDFLARE_API_TOKEN` / `CLOUDFLARE_ACCOUNT_ID` | **Environment (`production`)** | デプロイ | デプロイできない |
| `DISCUSSIONS_TOKEN` | **Environment (`production`)** | コメント読み取り。deploy 後に Worker シークレット `GITHUB_TOKEN` として押し込む | コメントが GitHub へのリンクに落ちる |
| `CF_ANALYTICS_TOKEN` | Repository | Cloudflare Web Analytics | ビーコンを埋め込まない |
| `GITHUB_TOKEN` | (自動供給) | ビルド時の GitHub API rate limit 回避 | コミット済みスナップショットにフォールバック |

`DISCUSSIONS_TOKEN` だけは**ビルド時ではなくリクエスト時**に要る唯一の資格情報で、ブラウザには一切届かない。だから Repository ではなく Environment に置く (改訂 2026-09-10)。fine-grained PAT には期限があり、切れるとコメントがリンクに落ちる — 他は何も壊れない。

Environment に置く 3 つ (Cloudflare の 2 つと `DISCUSSIONS_TOKEN`) は、**`environment: production`
を宣言したジョブ (= deploy ジョブのみ) からしか見えない**。public リポジトリでは、repository
secret は push できるブランチのワークフローから読み出せてしまうので、これは実質的な境界になる。

残る `CF_ANALYTICS_TOKEN` は build ジョブが読み、build ジョブは `environment:` を持たないので
Environment では届かない。そして**これはそもそも秘密ではない** — ビルド後の HTML に入って全訪問者に
配られる。`secrets` に置いているのは、fork でビルドしたときに本家の Analytics に計上しないため
だけで、**secret が無い場合は機能ごと出さない**方に倒す。

`DISCUSSIONS_TOKEN` を **`production` Environment から消したら Cloudflare 側からも消える** (改訂
2026-09-10)。deploy ジョブは、値があれば `wrangler secret put`、無ければ Worker のシークレット一覧を
見て `wrangler secret delete` する。put だけにすると、意図的に引き上げた資格情報が誰かが気づくまで
エッジで生き続ける。**削除の失敗を握り潰さない**のも同じ理由で、消えていないなら deploy を落とす。

> `production` Environment の **Deployment branches** 制限は、チェックアウト先ではなく
> **ワークフロー実行の ref** で判定される。スケジュール実行の ref はデフォルトブランチ
> (`develop`) になるため、ここでのトレードオフは二択になる (2026-09-08):
>
> - **`main` + `develop` を許可**: 週次リビルドが動く。ただし `develop` に push できる者は
>   `web.yml` の deploy ステップを書き換えられ、月曜のスケジュール実行がそれを production の
>   Cloudflare トークンで実行してしまう。job の `if:` は push / dispatch にしか効かないため、
>   ここは `develop` のブランチ保護が唯一の防壁になる
> - **`main` のみ**: 上の経路を塞げるが、週次リビルドは黙って止まる。星の数・コミット一覧・
>   release 一覧はビルド時に読むので、以後 deploy するまで古いまま固定される
>
> どちらを取るかは `develop` のブランチ保護の強さ次第。保護が弱いなら `main` のみにして、
> 週次リビルドは諦める (必要なときに `main` へ push するか、手動実行する)。

### Worker (`nohrs.app`)

`workers/site.ts`。全ページが prerender 済みなので、この Worker が自分で答えるのは 2 つだけで、
それ以外は assets binding にそのまま渡す。

1. **正規ホストとスキーム**。`www.nohrs.app` も同じ Worker に付けているため、何もしないと apex と
   同一のサイトをもう 1 部配信することになる。`*.nohrs.app` は apex へ **301**。あわせて `http://` も
   `https://` へ 301 する (カスタムドメインは HTTP を自動リダイレクトしない)。両方の補正を 1 回の
   301 にまとめるので、`http://www.nohrs.app/x` でもホップは 1 回。`localhost` と `*.workers.dev` は
   対象外 (`wrangler dev` とプレビューが本番に飛ばないように)
2. **`/` の言語振り分け**。Cookie → `Accept-Language` の順に決めて **302** で `/en` か `/ja` に送り、
   Cookie に記憶する。`Vary: Accept-Language, Cookie` を付け、別の言語設定の訪問者が同じリダイレクトを
   キャッシュから受け取らないようにする

`run_worker_first` は `["/"]` ではなく **`true`**。このオプションはパスで判定するため、`/` だけに
絞ると `www.nohrs.app/en/docs` を Worker が見られず、ホストの正規化ができない。**リクエストごとに
Worker が 1 回起きる代わりに、正規ホストの規則がダッシュボードの Redirect Rule ではなくリポジトリ内に
残り、レビューもテストもできる**。この規模のトラフィックでは無料枠に対して誤差。

### Worker (`noh.rs`)

`workers/noh-rs-redirect.ts`。`nohrs.app` へパスを保ってリダイレクトする。

- `noh.rs/en/...` のように**すでに言語が付いているパス**は恒久的な対応なので **301**
- `noh.rs/docs/installation` のように**言語が付いていないパス**は `Accept-Language` で解決する必要が
  あるので **302 + `Vary`**。ここで 301 を返すと、最初の訪問者の言語が全員に焼き付いてしまう
- 短縮スキーム `noh.rs/p/<plugin-id>` → `/<lang>/plugins/<plugin-id>` (302)。
  `noh.rs/r/<tag>` → **GitHub の release ページに 301** (個別 release のページが無いため。§1)

---

## 8. 後続フェーズの拡張

> スコープ変更により blog 本格化 (RSS / コメント / OG 自動生成) は **P1 に前倒し済**。以下は P1 以降に *データ・コンテンツが充実する* ものを中心に記載。

| Phase | 追加内容 |
|-------|---------|
| P1 (前倒し済) | blog 本格化 (MDX components / RSS / コメント / OG 自動生成)。器・機能はローンチ時に完成、記事は順次追加 |
| P3 | コマンド一覧ページ (`/docs/commands`) を本体の inventory レジストリからビルド時生成 |
| P4 | plugin authoring docs / WIT API reference 自動生成 |
| P5 | Plugin Store の実データ投入 (器は P1 で Preview 済、P4–P5 で本物の plugin metadata を enrich)、release frontmatter リッチ化 |
| P6 | docs 完成度向上、screenshot/動画整備 |
| Future | menubar 常駐モードページ、CLI/HTTP API doc、AI agent 統合 |
