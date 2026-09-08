# Web — nohrs.app & noh.rs

> Status: Active (P1 で **フルスコープ・production-grade** で立ち上げ、後続 Phase でデータを充実)
> Related: [`ROADMAP.md`](./ROADMAP.md), [ADR 0006 (monorepo-web)](./adr/0006-monorepo-web.md), [ADR 0007 (cloudflare-hosting)](./adr/0007-cloudflare-hosting.md), [ADR 0008 (web-design-system)](./adr/0008-web-design-system.md)

本書は nohrs の web 公開面 (`nohrs.app` + `noh.rs`) の構成・ホスティング・コンテンツ運用を定めます。

## 0. スコープ方針 (重要)

当初 P1 は「web MVP (landing + redirect + blog/docs skeleton)」だったが、**P1 から本格的・production-grade で立ち上げる**方針に変更した (issue #55 を re-scope)。

- **見た目・構造はフル完成**: デザイン DNA は **zed.dev** を土台に、Vercel (タイポグラフィ規律) / Cursor (製品デモの見せ方) をアクセントとして借りる。詳細は [ADR 0008](./adr/0008-web-design-system.md) と §2.5。
- **機能スコープもフル**: blog 本格機能 (giscus / RSS / OG 自動生成) を P2 から **P1 に前倒し**。Plugin Store / コマンド一覧など本体未実装に依存するページは、**シードデータ + "Coming soon / Preview" 状態**で器を作り込み、バックエンド (P3–P5) が揃い次第データを差し込む。
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
- 短縮スキーム (将来):
  - `noh.rs/p/<plugin-id>` → `nohrs.app/plugins/<plugin-id>`
  - `noh.rs/r/<release>` → `nohrs.app/releases/<release>` (もしくは GitHub release)
- HTTPS 強制
- HSTS preload は P5 以降に検討

---

## 2. 技術スタック

| 項目 | 採用 |
|------|------|
| Framework | TanStack Start + Vite+ |
| ホスティング | **Cloudflare Pages + Workers** |
| デプロイ | `main` ブランチ → 本番、PR → preview (`<branch>.nohrs-web.pages.dev`) |
| スタイリング | **Tailwind v4 + CSS 変数デザイントークン** (ライト/ダーク・warm neutral・tan アクセントを変数で一元管理) |
| コンポーネント | **shadcn / Radix headless プリミティブを取り込み、自前トークンで再スキン** (a11y を担保しつつ汎用 LP 感を回避) |
| フォント | self-host (Google Fonts 直リンクは禁止)。ラテン=グロテスク + モノ、和文=Zen Kaku Gothic New。詳細 §2.5 |
| モーション | 控えめ・意味のある動きのみ (CSS 主体、一部 Motion)。`prefers-reduced-motion` 必須対応 |
| ドキュメント検索 | **Pagefind** (ビルド時に静的インデックス生成、CJK セグメンテーション内蔵で和文 docs も対応) |
| 分析 | **Cloudflare Web Analytics** (cookie 不要、cookie banner 不要) |
| OG 画像 | Satori で自動生成 (Cloudflare Workers から)。**P1 から有効** |
| コメント (blog) | giscus (GitHub Discussions backed)。**P1 から有効** |
| RSS / Atom | 両方提供 (`/blog/rss.xml`, `/blog/atom.xml`)、言語別。**P1 から有効** |

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
- グラデ/3D/パララックスは封印 (マーケ LP 化を避け職人トーンを維持)
- **要素そのものを動かす演出も封印** (改訂 2026-09-08)。ホバーで吸い付くボタン、回転するグリフの類は、罫線と余白で組む他セクションと語彙が食い違い、ページ全体の品位を下げる。動かしてよいのは「状態が変わったこと」を伝える場合に限る
- 実装は CSS 主体、オーケストレーションが要る所のみ軽量に Motion。`prefers-reduced-motion` 対応必須
- 基準値: `220ms` / `cubic-bezier(.16,1,.3,1)` / `translateY(6px)` / stagger `50ms`
- **セクション単位のスクロールスナップを入れる** (改訂 2026-09-08)。`scroll-snap-type: y proximity` + `section { scroll-snap-align: start }` + `scroll-padding-top` で sticky ヘッダ分を逃がす。`mandatory` は使わない — メーカーズノートやロードマップはビューポートより高く、途中で止まりたい読者と衝突するため。`prefers-reduced-motion` では `scroll-snap-type: none` に落とす

### 品質基準 (ローンチ条件)

- **a11y**: WCAG AA・完全キーボード操作・コントラスト・reduced-motion
- **パフォーマンス**: Lighthouse 95+・edge SSR・画像最適化・font self-host で CLS 抑制
- **SEO**: sitemap・`hreflang` 自動生成・OG/Twitter meta・JSON-LD 構造化データ・canonical

---

## 3. ディレクトリ構成

```text
web/
├── package.json
├── vite.config.ts
├── app/                       # TanStack Start app
│   ├── routes/
│   │   ├── __root.tsx
│   │   ├── $lang.tsx          # /en/... or /ja/...
│   │   ├── $lang/index.tsx    # landing
│   │   ├── $lang/about.tsx    # プロジェクトの物語 + values + メーカーズノート
│   │   ├── $lang/download.tsx # ダウンロード専用導線
│   │   ├── $lang/roadmap.tsx  # ROADMAP.md の web 化 (P1–P6 進捗)
│   │   ├── $lang/blog/...
│   │   ├── $lang/docs/...
│   │   ├── $lang/releases/...
│   │   └── $lang/plugins/...
│   ├── components/
│   ├── lib/
│   │   ├── content.ts         # mdx loader
│   │   ├── github.ts          # GitHub API client (build-time)
│   │   └── i18n.ts
│   └── styles/
├── content/
│   ├── en/
│   │   ├── blog/
│   │   ├── docs/
│   │   └── pages/
│   ├── ja/
│   └── plugins/               # Plugin Store エントリ (PR ベース登録)
│       └── <plugin-id>.toml
├── public/
└── workers/
    └── noh-rs-redirect.ts     # noh.rs リダイレクト Worker
```

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

- **トップナビ**: Features (landing 内アンカー) · Docs · Blog · Plugins · Releases · **主 CTA** · 言語切替 · テーマ切替
  - 主 CTA は **リリースの有無で切り替える** (改訂 2026-09-08)。公開 release が 0 件の間は `Star on GitHub`、初回 release 以降は `Download`。GitHub API から取得する release 件数で分岐させ、pre-alpha 中に「押しても何も無い」導線を作らない
- **フッタ (zed 風 4 列)**:
  - Product: Download · Releases · Plugins · Roadmap · Docs · GitHub
  - Resources: FAQ (将来) · Community (Discord) · Discussions · Privacy
  - Project: Blog · About · Brand (将来) · License
  - Social: X · Discord · GitHub

### 6.1 `/` (ランディング)

ページ全体像 (上から下のスクロール、zed.dev のホーム構成を nohrs 流に):

1. **Hero**: tagline (大タイポ) + サブコピー + 主 CTA。**製品スクショは Hero に置かない** (改訂 2026-09-08)
   - tagline は **`Launcher × Explorer`** で確定。`×` のみ mono + Rust tan で組み、他は Inter。リポジトリ description の冒頭と一致させる
   - サブコピーは事実のみ 1〜2 行 (何であるか・何で書かれているか・ライセンス)。バッジや煽り文句を足さない
   - **背景は無地**。Hero に置くのは見出し・サブコピー・CTA の 3 つだけで、ステータスバッジの類は置かない。pre-alpha であることはサブコピーの文中で述べる
   - **バッジ・タグ・中黒区切りを禁止する** (改訂 2026-09-08)。`Pre-alpha · macOS · MIT` のような属性の羅列、枠線付きの小ラベル、見出し上のカテゴリタグは使わない。伝えるべき属性は本文の文として書くか、罫線で区切った行に落とす
   - CTA の下に罫線を挟んで **build from source のコマンド** を置く (改訂 2026-09-08)。公開 release が 0 件の間、「では今どう試すのか」に答える導線がページ上に存在しないため。release が出たら、このブロックは `/download` へのリンクに差し替える
   - **正直主義は維持**: 実在する Explorer のスクショは Hero ではなく直後の Preview セクションに置く。**当面は静止スクショで代替**し（**en ロケールで撮り直し**）、操作 GIF は後日差し替える（README 約束分）。Launcher/Plugins/Search は **偽装せず** mock も作らず、テキスト行のみで見せる
   - **スクショに額装をしない** (改訂 2026-09-08)。スクショには実物の macOS ウィンドウ (信号ボタン・角丸・影) が既に写っているため、外側にウィンドウクロームを模した枠・タイトルバー・影を重ねると二重になる。画像をそのまま置き、キャプションを罫線で受ける。撮影時に背景を含めて整えるのが正しい対処であり、web 側で飾って補うのは誤り
2. **"Why nohrs?"** — 3-4 ポイントで差別化 (Launcher first-class / Explorer first-class / WASM plugins / Spotlight 非依存の検索。README の柱を流用)
3. **主要機能ハイライト** (Explorer=実在 / Launcher・Plugin・Search=Coming カードで mock 提示)
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
- Cloudflare Pages の cron で **週次再ビルド** (latest release を追従)

### 6.3 `/blog`

- MDX (`web/content/<lang>/blog/<slug>.mdx`)
- frontmatter: title / date / author / tags / canonical / og_image
- カスタムコンポーネント: `<Callout>`, `<Screenshot>`, `<CodeTabs>`, `<YouTube>`
- タグページ (`/blog/tags/<tag>`)、年別アーカイブ (`/blog/2026/`)
- giscus コメント (GitHub Discussions)
- RSS / Atom feed (言語別)
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
- **本体未実装 (P4–P5) のため P1 では Preview**: シードの `<id>.toml` 数件 + "Coming soon" 状態でカード/グリッド/カテゴリの器を作り込む

### 6.6 `/about`

- プロジェクトの物語・哲学・values (zed.dev の信頼感の源。実装が安く効果が高い)
- **メーカーズノート** (landing と共有可。なぜ nohrs を作るのか)
- 注: Hero タグライン + メーカーズノートは翻訳が硬くなりやすいため、AI 訳でも最終的に軽い人力推敲を推奨

### 6.7 `/download`

- OSS 最重要 CTA。macOS バイナリ (release asset) · build from source 手順 · システム要件を集約
- pre-alpha の現状を誠実に提示 (まだ正式 release が無い旨)。release が出たら `/releases` と連動

### 6.8 `/roadmap`

- 既存 [`docs/ROADMAP.md`](./ROADMAP.md) を web 化。P1–P6 のフェーズと進捗を可視化
- zed.dev も Roadmap を持つ。透明なロードマップは §6.1 の社会的証明置換の一部

---

## 7. ビルド・デプロイ

### CI (GitHub Actions)

- PR open → Cloudflare Pages の preview デプロイが自動で立つ
- `main` への merge → 本番デプロイ
- `paths` filter で `web/**` と `docs/**` 変更時のみ web ビルドを走らせる

### 環境変数 (Cloudflare Pages secrets)

| 変数 | 用途 |
|------|------|
| `GITHUB_TOKEN` | ビルド時の GitHub API rate limit 回避 |
| `GISCUS_REPO_ID` | giscus コメント |
| `CF_ANALYTICS_TOKEN` | Cloudflare Web Analytics |

### Worker (`noh.rs`)

```ts
// workers/noh-rs-redirect.ts (擬似コード)
export default {
  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);

    // 短縮スキームの展開
    if (url.pathname.startsWith("/p/")) {
      const id = url.pathname.slice(3);
      return Response.redirect(`https://nohrs.app/plugins/${id}${url.search}`, 301);
    }
    if (url.pathname.startsWith("/r/")) {
      const tag = url.pathname.slice(3);
      return Response.redirect(`https://nohrs.app/releases/${tag}${url.search}`, 301);
    }

    // path 保持リダイレクト
    return Response.redirect(`https://nohrs.app${url.pathname}${url.search}`, 301);
  },
};
```

---

## 8. 後続フェーズの拡張

> スコープ変更により blog 本格化 (RSS / giscus / OG 自動生成) は **P1 に前倒し済**。以下は P1 以降に *データ・コンテンツが充実する* ものを中心に記載。

| Phase | 追加内容 |
|-------|---------|
| P1 (前倒し済) | blog 本格化 (MDX components / RSS / giscus / OG 自動生成)。器・機能はローンチ時に完成、記事は順次追加 |
| P3 | コマンド一覧ページ (`/docs/commands`) を本体の inventory レジストリからビルド時生成 |
| P4 | plugin authoring docs / WIT API reference 自動生成 |
| P5 | Plugin Store の実データ投入 (器は P1 で Preview 済、P4–P5 で本物の plugin metadata を enrich)、release frontmatter リッチ化 |
| P6 | docs 完成度向上、screenshot/動画整備 |
| Future | menubar 常駐モードページ、CLI/HTTP API doc、AI agent 統合 |
