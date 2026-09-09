import { LANG_COOKIE, langFromCookie, negotiateLang } from '../app/lib/negotiate.ts'

/**
 * Structurally typed rather than pulling in `@cloudflare/workers-types`: this
 * is the only Workers binding the project uses, and the full type package
 * conflicts with the DOM lib the app is checked against.
 */
type Env = { ASSETS: { fetch: (request: Request) => Promise<Response> } }

/** Every other hostname the Worker answers on redirects here. */
const CANONICAL_HOST = 'nohrs.app'

/**
 * The site itself is fully prerendered and served from the assets binding.
 * This Worker owns the two decisions that cannot be baked into a file:
 *
 * 1. Which language a bare `/` should land on (docs/web.md §4).
 * 2. Which hostname is canonical. `www.nohrs.app` is attached to this Worker
 *    as well, and without this it would serve a second, identical copy of the
 *    site rather than redirecting to the apex.
 *
 * The host rule is why `run_worker_first` is `true` rather than just `["/"]`:
 * that option matches on path, so a Worker scoped to `/` would never see
 * `www.nohrs.app/en/docs` and could not redirect it. Keeping the rule in code
 * costs one Worker invocation per request and buys a canonical-host policy
 * that is reviewed with the rest of the repository, rather than a dashboard
 * rule nothing here knows about.
 */
export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url)

    // `localhost` and `*.workers.dev` are left alone so preview deploys and
    // `wrangler dev` do not bounce to production.
    const local = url.hostname === 'localhost' || url.hostname === '127.0.0.1'
    const insecure = url.protocol === 'http:' && !local
    const offCanonicalHost = url.hostname.endsWith(`.${CANONICAL_HOST}`)

    // A custom domain does not redirect HTTP by default, so the scheme is
    // enforced here rather than left to a dashboard setting. Both corrections
    // go out as one 301, so `http://www.nohrs.app/x` costs one hop, not two.
    if (insecure || offCanonicalHost) {
      const target = new URL(url)
      if (insecure) target.protocol = 'https:'
      if (offCanonicalHost) target.hostname = CANONICAL_HOST
      return new Response(null, {
        status: 301,
        headers: { Location: target.toString() },
      })
    }

    if (url.pathname === '/') {
      const remembered = langFromCookie(request.headers.get('cookie'))
      const lang = remembered ?? negotiateLang(request.headers.get('accept-language'))
      const target = new URL(`/${lang}${url.search}`, url.origin)

      return new Response(null, {
        status: 302,
        headers: {
          Location: target.toString(),
          // Two visitors with different Accept-Language must not share a
          // cached redirect.
          Vary: 'Accept-Language, Cookie',
          'Cache-Control': 'no-store',
          ...(remembered
            ? {}
            : {
                'Set-Cookie': `${LANG_COOKIE}=${lang}; Path=/; Max-Age=31536000; SameSite=Lax; Secure`,
              }),
        },
      })
    }

    return env.ASSETS.fetch(request)
  },
}
