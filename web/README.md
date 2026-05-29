# nohrs web (`nohrs.app` + `noh.rs`)

The public site for nohrs. Built with TanStack Start (React 19) + Vite, Tailwind
v4, and deployed on Cloudflare. See [`docs/web.md`](../docs/web.md) for the full
spec and [ADR 0006–0008](../docs/adr) for the decisions behind it.

This package is intentionally **outside** the Cargo workspace (ADR 0006); it has
its own toolchain and is never built by `cargo`.

## Develop

```bash
pnpm install
pnpm dev          # http://localhost:3000  (redirects to /en)
```

## Quality gate

```bash
pnpm lint             # eslint
pnpm exec tsc --noEmit  # type check
pnpm build            # production build (Cloudflare Pages runs this on deploy)
pnpm check            # prettier --check
```

## Layout

```text
src/
  routes/
    __root.tsx        # document shell, base SEO meta, theme init
    index.tsx         # /  → Accept-Language redirect to /{locale}
    $lang/
      route.tsx       # locale layout: validates lang, header + footer
      index.tsx       # landing (web.md §6.1)
      about.tsx       # web.md §6.6
      download.tsx     # web.md §6.7
  components/
    ui/               # Radix primitives reskinned with our tokens
    site-header.tsx, site-footer.tsx, theme-toggle.tsx,
    lang-switcher.tsx, reveal.tsx
  lib/
    i18n.ts           # locales + full en/ja message tree (single source)
    locale-context.tsx, seo.ts, links.ts, utils.ts
  styles.css          # Tailwind v4 + design tokens (light/dark, warm, tan)
workers/
  noh-rs-redirect.ts  # noh.rs → nohrs.app 301 Worker (deployed separately)
```

## i18n

Path-prefix routing (`/en`, `/ja`), canonical = `en`, full bilingual parity.
All UI strings live in `src/lib/i18n.ts`; the `en` tree defines the shape and
`ja` is type-checked to match it.

## Deploy

Cloudflare Pages via git integration: PRs get a preview, `main` is production
(web.md §7). Connecting the Cloudflare project and its secrets
(`GITHUB_TOKEN`, analytics token) is a one-time ops step done in the dashboard.
