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
npm test             # the Workers' redirect logic
```

`npm run dev` skips the `prebuild` scripts, so it runs with the committed GitHub snapshot
and without the self-hosted fonts. Both are build artifacts, not source; see below.

## How a page gets built

Every page is rendered once, at build time, and served as a static file. The only things
decided per request are the canonical host and which language a bare `/` lands on, so the
deployment is a directory of HTML plus one small Worker.

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
├── components/      the design system (Header, Section, Phases, HeroOrbit, FluidOrb, …)
├── lib/
│   ├── strings.ts   every UI string, in both languages
│   ├── negotiate.ts language negotiation — imported by the Workers too
│   ├── content.ts   MDX and the plugin registry
│   ├── orbit.ts     the hero ring's geometry
│   ├── github.ts    the build-time snapshot
│   └── seo.ts       canonical, hreflang, OG and JSON-LD for one page
content/
├── en|ja/blog/*.mdx
├── en|ja/docs/*.mdx
└── plugins/*.toml   the plugin registry: one file per plugin, added by pull request
workers/
├── site.ts          canonical host, language at `/`, then the assets
├── noh-rs-redirect.ts
└── workers.test.ts  both Workers' redirects, run by `npm test`
```

The Workers are the only request-time logic on the site, and both decide
redirects — the kind of bug that is invisible in a screenshot and expensive once
a crawler has cached it. They take a `Request` and return a `Response` with
nothing else in the way, so `npm test` checks them with `node:test` and no
deployment.

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

Deploys run from CI, so no Cloudflare credential ever has to sit on a laptop.

**One-time setup.** The five secrets are split by blast radius, not filed together.

Settings → Environments → new environment named `production`, then as **environment
secrets** there:

| Secret | Where it comes from |
|--------|--------------------|
| `CLOUDFLARE_API_TOKEN` | A Cloudflare API token from the **Edit Cloudflare Workers** template, with the `nohrs.app` and `noh.rs` zones included in its zone resources |
| `CLOUDFLARE_ACCOUNT_ID` | The account ID on any Cloudflare dashboard page |

These two can deploy to production, and an environment secret is only visible to a job
that declares `environment: production` — which is the deploy job and nothing else. In a
public repository that is a real boundary: a repository secret can be read by any workflow
run from any branch someone can push to.

The token needs the zones because the Workers claim `nohrs.app` and `noh.rs` as custom
domains; an account-only token uploads the script and then fails attaching the routes.

Settings → Secrets and variables → Actions, as **repository secrets**:

| Secret | Where it comes from | Without it |
|--------|--------------------|------------|
| `GISCUS_REPO_ID`, `GISCUS_CATEGORY_ID` | giscus.app, for this repository's Discussions | No comment section |
| `CF_ANALYTICS_TOKEN` | Cloudflare Web Analytics | No beacon |

The build job reads these and does not declare an environment, so environment secrets
would not reach it. They are also not really secrets — all three are served to every
visitor inside the built HTML. They live in `secrets` so that a fork building this site
does not post into our Discussions or count against our analytics.

`GITHUB_TOKEN` is provided automatically; there is nothing to add.

**If you put a Deployment branches rule on the `production` environment**, it has to allow
`develop` as well as `main`. The rule is evaluated against the workflow run's ref, and a
scheduled run's ref is the default branch — so a `main`-only rule silently stops the Monday
rebuild. Making these environment secrets already limits them to the deploy job, which is
the point; the job's own `if:` decides when it runs.

**Running it.** Actions → *Web* → Run workflow. A manual run builds and checks whichever ref
it was dispatched on, but **only `main` deploys** — production credentials must not be
reachable from an arbitrary branch that anyone with write access can push. So the way to put
something on the site is to merge it. After merging, a push to `main` deploys on its own, and
a Monday schedule redeploys `main` so the star count, commit list and release list follow the
repository.

**By hand**, if you would rather (`wrangler login` first):

```sh
npx wrangler deploy                                          # nohrs.app
npx wrangler deploy --config workers/wrangler.noh-rs.jsonc   # noh.rs
```

The optional secrets are optional on purpose: comments and analytics are simply absent
without them, which is what a fork building this site should get.

### While the site is gated

Before launch the deployed site sits behind **Cloudflare Access** (Zero Trust → Access →
Applications), which is dashboard state and not described by anything in this repository.
Two consequences to know about:

- **Nothing without a session can read the site.** The `curl` checks below return the
  Access login page rather than the page you asked for, and no crawler can reach the
  sitemap. That is the point, but it means the site cannot be verified from a script until
  the gate comes off or a service token is issued for it.
- **Deploys are unaffected.** They go through the Cloudflare API, not the hostname, so
  `wrangler deploy` and the workflow keep working with the gate in place.

`www.nohrs.app` needs no policy of its own: the Worker answers every `*.nohrs.app` request
with a 301 to the apex before it touches an asset, so it serves no content to gate.

Removing the gate is the launch step. `robots.txt` and the sitemap already assume a public
site, so nothing in the build has to change.

## Where this deviates from docs/web.md

| Spec | Built | Why |
|------|-------|-----|
| Cloudflare Pages | Cloudflare **Workers** with an assets binding | One deploy artifact instead of two, and the redirect rules — canonical host, language at `/` — live in the repository as reviewable, tested code rather than in a dashboard the repo knows nothing about. ADR 0007's reasoning — Cloudflare, one ecosystem, wide free tier — is unchanged |
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
