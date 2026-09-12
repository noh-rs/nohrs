import { LANG_COOKIE, langFromCookie, negotiateLang } from '../app/lib/negotiate.ts'
import { loadThread, THREAD_TERM, type Thread } from '../app/lib/discussion.ts'

/**
 * Structurally typed rather than pulling in `@cloudflare/workers-types`: these
 * are the only Workers bindings the project uses, and the full type package
 * conflicts with the DOM lib the app is checked against.
 */
type Env = {
  ASSETS: { fetch: (request: Request) => Promise<Response> }
  /**
   * A read-only token for this repository's Discussions. Absent in a fork,
   * where the comment API reports itself unconfigured and the article falls
   * back to a link — the same graceful degradation the giscus wiring had.
   */
  GITHUB_TOKEN?: string
}

/** Only `waitUntil` is used, and only to finish a cache write after the reply. */
type Ctx = { waitUntil: (promise: Promise<unknown>) => void }

/** Every other hostname the Worker answers on redirects here. */
const CANONICAL_HOST = 'nohrs.app'

/**
 * Repeated from `app/lib/site.ts` rather than imported, for the reason the Env
 * type above is hand-written: that module reads `import.meta.env`, which only
 * exists once Vite has processed it, and this file is also run directly by the
 * tests and by Wrangler.
 */
const REPO = 'noh-rs/nohrs'

/** Where an article's comment thread is read from. */
const THREAD_PATH = '/api/discussion'

type EdgeCache = {
  match: (request: Request) => Promise<Response | undefined>
  put: (request: Request, response: Response) => Promise<void>
}

/**
 * `caches.default` is a Workers extension that the DOM `CacheStorage` type does
 * not describe, and it is missing entirely under `node --test`. Reached through
 * `globalThis` so that neither fact needs a type override or a test-only branch.
 */
function edgeCache(): EdgeCache | undefined {
  return (globalThis as { caches?: { default?: EdgeCache } }).caches?.default
}

function json(body: unknown, status: number, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json; charset=utf-8', ...headers },
  })
}

/**
 * Reads of the same thread that are already in flight, so that a cold cache
 * does not turn a burst of readers into a burst of GitHub calls. The cache
 * entry only exists once the first response comes back; until then every
 * request is a miss.
 *
 * Per isolate, which is all a Worker can do without a Durable Object — and
 * enough, since a colo runs few isolates and the two mechanisms cover the same
 * hole from either side.
 */
const inFlight = new Map<string, Promise<Thread | null>>()

/**
 * The comment thread for one article.
 *
 * The token stays on this side: it is a credential for our repository, and the
 * page only ever needs the comments it reads. Everything else here exists to
 * make sure one popular article costs GitHub one request rather than one per
 * reader — the edge cache is the rate limiter.
 */
async function thread(url: URL, env: Env, ctx?: Ctx): Promise<Response> {
  const term = url.searchParams.get('term') ?? ''
  if (!THREAD_TERM.test(term)) return json({ error: 'term' }, 400)
  if (!env.GITHUB_TOKEN) return json({ error: 'unconfigured' }, 503)

  // Keyed on the normalised term, so `?term=x&cb=1` cannot mint cache entries.
  const cache = edgeCache()
  const key = new Request(`${url.origin}${THREAD_PATH}?term=${encodeURIComponent(term)}`)
  const hit = await cache?.match(key)
  if (hit) return hit

  let found
  try {
    const running = inFlight.get(term)
    const read = running ?? loadThread(term, { token: env.GITHUB_TOKEN, repo: REPO })
    if (!running) inFlight.set(term, read)
    try {
      found = await read
    } finally {
      // Only the request that started it clears it, or a later arrival would
      // free the slot while the read it joined is still running.
      if (!running) inFlight.delete(term)
    }
  } catch {
    // An outage or an expired token must not take the article down with it.
    // Nothing is cached, so the next reader tries again, and the page falls
    // back to a link to the thread on GitHub.
    return json({ error: 'upstream' }, 502)
  }

  // A minute is long enough that a burst of readers costs one API call, and
  // short enough that a new comment is not missing for the rest of the day.
  const response = json({ thread: found }, 200, { 'Cache-Control': 'public, max-age=60' })
  if (cache) ctx?.waitUntil(cache.put(key, response.clone()))
  return response
}

/**
 * The site itself is fully prerendered and served from the assets binding.
 * This Worker owns the two decisions that cannot be baked into a file:
 *
 * 1. Which language a bare `/` should land on (docs/web.md §4).
 * 2. Which hostname is canonical. `www.nohrs.app` is attached to this Worker
 *    as well, and without this it would serve a second, identical copy of the
 *    site rather than redirecting to the apex.
 * 3. Reading an article's comment thread from GitHub, which needs a credential
 *    the prerendered site must not carry.
 *
 * The host rule is why `run_worker_first` is `true` rather than just `["/"]`:
 * that option matches on path, so a Worker scoped to `/` would never see
 * `www.nohrs.app/en/docs` and could not redirect it. Keeping the rule in code
 * costs one Worker invocation per request and buys a canonical-host policy
 * that is reviewed with the rest of the repository, rather than a dashboard
 * rule nothing here knows about.
 */
export default {
  // `ctx` is optional because the only thing taken from it is a best-effort
  // cache write; the tests call this without one.
  async fetch(request: Request, env: Env, ctx?: Ctx): Promise<Response> {
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

    if (url.pathname === THREAD_PATH) {
      if (request.method !== 'GET') {
        return json({ error: 'method' }, 405, { Allow: 'GET' })
      }
      return thread(url, env, ctx)
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
