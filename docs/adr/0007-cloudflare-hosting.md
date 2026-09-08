# 0007 — web ホスティングは Cloudflare Pages + Workers + R2

> Status: Accepted
> Date: 2026-05-28

## Context

P1 で web (nohrs.app + noh.rs) を立ち上げるにあたり、ホスティングプラットフォームを決める必要がある。

候補:

- Cloudflare Pages + Workers (+ R2)
- Vercel
- Netlify
- GitHub Pages (静的のみ)

考慮事項:

- カバレッジパイプライン ([`docs/testing.md`](../testing.md)) で **`cargo llvm-cov` の HTML レポートを Cloudflare R2** にアップロードする方針 → どのみち Cloudflare アカウントが必要
- **noh.rs → nohrs.app の path 保持リダイレクト** を Worker で実装する方針 (詳細は [`docs/web.md`](../web.md) §1)
- TanStack Start の Cloudflare Pages adapter は v1 リリース済 (2026)、production-ready
- 無料枠が広いほど個人 OSS プロジェクトに優しい

## Decision

**Cloudflare Pages + Workers + R2 を採用する**。

| 用途 | サービス |
|------|---------|
| **静的 / SSR サイト** (nohrs.app) | Cloudflare Pages |
| **noh.rs リダイレクト** | Cloudflare Workers |
| **カバレッジ HTML レポート** (`coverage.nohrs.app`) | R2 + Pages routing |
| **将来の画像 CDN** (blog screenshot / plugin icon) | R2 |
| **分析** | Cloudflare Web Analytics (cookie 不要) |
| **検索 (docs)** | Pagefind (静的、ホスティング非依存) |
| **PR preview** | Cloudflare Pages 標準機能 (`<branch>.nohrs-web.pages.dev`) |

ドメイン管理も Cloudflare DNS で一本化。

## Consequences

### Positive

- エコシステム一本化: DNS / hosting / R2 / Analytics / Workers が同コンソール
- 無料枠が広く、個人 OSS のスケールでコスト懸念がほぼゼロ
- noh.rs のリダイレクト Worker が 5 行で書ける
- カバレッジ HTML を R2 にアップ → `coverage.nohrs.app/pr/<n>/` で公開、追加サービス不要
- Cookie 不要の Web Analytics で GDPR / Cookie banner 不要

### Negative

- TanStack Start の Cloudflare adapter は Vercel adapter よりやや新しい (実用段階だが trailing edge)
- Node ライブラリの一部 (`fs`, `path` の一部 API) が edge runtime で動かない → ビルド時処理に寄せる必要
- Cloudflare 自体の outage 時に web 全体が落ちる (Cloudflare の SLA を信頼)

## Alternatives Considered

| 案 | 棄却理由 |
|----|---------|
| Vercel | tanstack start サポート安定、PR preview 標準。ただし Cloudflare R2 と別エコシステム、カバレッジ用に別 storage 必要 → 二重管理 |
| Netlify | Vercel と同様、エコシステム分散 |
| GitHub Pages | 静的のみ。TanStack Start の SSR / 動的機能を活用できない |
| 自前 VPS | OSS 個人プロジェクトに運用負担が見合わない |
| AWS Amplify + CloudFront | AWS は無料枠が小さく、個人 OSS 規模では月額が読みにくい |

## 改訂 (2026-09-08) — Pages ではなく Workers + assets binding

実装時に、静的サイト部分のホスティングを **Cloudflare Pages から Cloudflare Workers の
assets binding に変更**した。Cloudflare にエコシステムを一本化する・無料枠が広い・R2 と
同じコンソールで扱える、という本 ADR の判断根拠は変わらない。変わったのは、同じ
Cloudflare の中でどの入れ口を使うか。

理由は**成果物を 1 つにするため**。web の全ページはビルド時に prerender され、
リクエストごとに決まるのは正規ホストと `/` の言語振り分けだけ ([`docs/web.md`](../web.md) §4)。
assets binding なら、その 2 つを担う Worker 1 本と静的アセットを同じ `wrangler deploy` で
出せる。Pages プロジェクトと別 Worker の 2 つをデプロイして両者のルーティングを合わせる
必要がなくなり、**リダイレクトの規則がダッシュボードの設定ではなくリポジトリ内のコードとして
残る**。

### 影響

- PR preview が Pages 標準機能ではなくなる。`wrangler versions upload` のプレビュー URL で
  代替する
- デプロイは GitHub Actions の `wrangler deploy` に一本化 ([`docs/web.md`](../web.md) §7)。
  Pages の cron ビルドも使えなくなるため、**週次再ビルドは GitHub Actions の `schedule`** で行う
- `coverage.nohrs.app` (R2) と noh.rs リダイレクト Worker は本 ADR のまま変更なし
