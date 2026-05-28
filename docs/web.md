# Web — nohrs.app & noh.rs

> Status: Draft (P1 で MVP、後続 Phase で機能追加)
> Related: [`ROADMAP.md`](./ROADMAP.md), [ADR 0006 (monorepo-web)](./adr/0006-monorepo-web.md), [ADR 0007 (cloudflare-hosting)](./adr/0007-cloudflare-hosting.md)

本書は nohrs の web 公開面 (`nohrs.app` + `noh.rs`) の構成・ホスティング・コンテンツ運用を定めます。

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
| ドキュメント検索 | **Pagefind** (ビルド時に静的インデックス生成) |
| 分析 | **Cloudflare Web Analytics** (cookie 不要、cookie banner 不要) |
| OG 画像 | Satori で自動生成 (Cloudflare Workers から) |
| コメント (blog) | giscus (GitHub Discussions backed) |
| RSS / Atom | 両方提供 (`/blog/rss.xml`, `/blog/atom.xml`)、言語別 |

R2 は他用途でも使用:
- `coverage.nohrs.app` (PR ごとの HTML カバレッジレポートを保管。詳細は [`docs/testing.md`](./testing.md))
- (将来) blog 画像・plugin アイコン CDN

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
| 翻訳欠落時 | en にフォールバック、UI で "翻訳募集中" バナーを表示 |
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

### 6.1 `/` (ランディング)

- Hero: tagline + screenshot/GIF + download CTA
- "Why nohrs?" — 3-4 ポイントで差別化
- 主要機能ハイライト (Launcher × Explorer / Plugin / Search)
- 開発状況 / ROADMAP リンク
- Community (Discord / X / GitHub)

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

| Phase | 追加内容 |
|-------|---------|
| P2 | blog 本格化 (MDX components / RSS / giscus / OG 自動生成) |
| P3 | コマンド一覧ページ (`/docs/commands`) を本体の inventory レジストリからビルド時生成 |
| P4 | plugin authoring docs / WIT API reference 自動生成 |
| P5 | Plugin Store ページ、release frontmatter リッチ化 |
| P6 | docs 完成度向上、screenshot/動画整備 |
| Future | menubar 常駐モードページ、CLI/HTTP API doc、AI agent 統合 |
