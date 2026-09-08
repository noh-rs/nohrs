# web — nohrs.app & noh.rs

The public site. TanStack Start + Vite, Tailwind v4 on CSS-variable tokens, deployed to
Cloudflare Workers. Specification: [`docs/web.md`](../docs/web.md). Decisions:
[ADR 0006](../docs/adr/0006-monorepo-web.md) (monorepo),
[ADR 0007](../docs/adr/0007-cloudflare-hosting.md) (hosting),
[ADR 0008](../docs/adr/0008-web-design-system.md) (design system).

```sh
npm install
npm run dev          # http://localhost:3000
npm run build        # prerender, sitemap, feeds, search index → dist/client
npm run typecheck
```

`npm run dev` skips the `prebuild` scripts, so it runs with the committed GitHub snapshot
and without the self-hosted fonts. Both are build artifacts, not source; see below.

## How a page gets built

Every page is rendered once, at build time, and served as a static file. Nothing about the
site changes per request except which language a bare `/` lands on, so the deployment is a
directory of HTML plus one small Worker.

```
npm run build
├── prebuild
│   ├── fetch-fonts.mjs    → public/fonts/          self-hosted woff2 + @font-face CSS
│   ├── fetch-github.mjs   → app/data/…generated    stars, releases, recent commits
│   └── build-og.mjs       → public/og/             Open Graph cards, via Satori
├── vite build             → dist/client/           33 prerendered pages + sitemap.xml
├── build-feeds.mjs        → dist/client/<lang>/blog/{rss,atom}.xml
└── pagefind               → dist/client/_pagefind/ the docs search index
```

Each `prebuild` script degrades rather than failing the build: no network means system
fonts, the committed GitHub snapshot, and no OG cards — never a broken deploy.

## Layout

```
app/
├── routes/          file-based routes; `$lang` is the language segment
├── components/      the design system (Header, Section, Phases, FluidOrb, …)
├── lib/
│   ├── strings.ts   every UI string, in both languages
│   ├── negotiate.ts language negotiation — imported by the Workers too
│   ├── content.ts   MDX and the plugin registry
│   ├── github.ts    the build-time snapshot
│   └── seo.ts       canonical, hreflang, OG and JSON-LD for one page
content/
├── en|ja/blog/*.mdx
├── en|ja/docs/*.mdx
└── plugins/*.toml   the plugin registry: one file per plugin, added by pull request
workers/
├── site.ts          serves the assets; negotiates a language at `/`
└── noh-rs-redirect.ts
```

## Adding content

A blog post is `content/<lang>/blog/<slug>.mdx` with `title`, `date`, `description`,
`author` and optional `tags` in its frontmatter. A doc page is
`content/<lang>/docs/<slug>.mdx` with `title`, `description`, `category` and `order`.
Both are picked up by the build with no route to write: the prerender list, the sitemap,
the feeds and the search index all read the same directory.

`<Callout>`, `<Screenshot>`, `<CodeTabs>` and `<YouTube>` are available inside MDX.

A page added in one language and not the other still renders — the reader gets the English
text under a notice — but that is a stopgap for the window between publishing and
translating, not the normal state. Launch parity is required
([`docs/web.md`](../docs/web.md) §4), and `strings.ts` enforces it for UI copy by typing
`ja` as `typeof en`.

## Deploying

```sh
npx wrangler deploy                                   # nohrs.app
npx wrangler deploy --config workers/wrangler.noh-rs.jsonc   # noh.rs
```

CI does this on a push to `main`, and again every Monday so the star count, commit list
and release list on the published site follow the repository.

Secrets: `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`, and optionally `GISCUS_REPO_ID`,
`GISCUS_CATEGORY_ID`, `CF_ANALYTICS_TOKEN`. Comments and analytics are simply absent
without them, which is what a fork should get.

## Where this deviates from docs/web.md

| Spec | Built | Why |
|------|-------|-----|
| Cloudflare Pages | Cloudflare **Workers** with an assets binding | One deploy artifact instead of two: the assets binding lets a single Worker answer `/` with a negotiated language while every other path is served straight from static storage without waking it. ADR 0007's reasoning — Cloudflare, one ecosystem, wide free tier — is unchanged |
| Satori on a Worker | Satori at **build time** | The inputs are known when the site is built. Rasterising at the edge would mean shipping and caching a renderer to produce a file that never changes |
| RSS/Atom from a route | RSS/Atom written by a **build script** | The site is fully prerendered; a feed route would be the only reason to keep a server |
| Fonts vendored | Fonts **downloaded at build time**, git-ignored | Self-hosting is the requirement, vendoring is not. Noto Sans JP is ~120 subset files per weight, which would be ~400 binaries of churn in the repository |

Pagefind, the TOML plugin registry, path-prefixed i18n, hreflang, JSON-LD and the
`Star on GitHub` → `Download` CTA switch are all as specified.

## Not yet done

- The screenshot in the Preview section is the Japanese locale; it needs re-shooting in
  `en` (docs/web.md §6.1)
- Blog tag pages (`/blog/tags/<tag>`) and year archives (`/blog/2026/`)
- Plugin entries are enriched from their own repositories at build time in the spec; today
  the TOML file is the whole record, because none of those repositories exist yet
