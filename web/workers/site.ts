import { LANG_COOKIE, langFromCookie, negotiateLang } from '../app/lib/negotiate'

/**
 * Structurally typed rather than pulling in `@cloudflare/workers-types`: this
 * is the only Workers binding the project uses, and the full type package
 * conflicts with the DOM lib the app is checked against.
 */
type Env = { ASSETS: { fetch: (request: Request) => Promise<Response> } }

/**
 * The site itself is fully prerendered and served from the assets binding.
 * This Worker exists for the one decision that cannot be baked into a file:
 * which language a bare `/` should land on (docs/web.md §4).
 *
 * A remembered choice wins over `Accept-Language`, so someone who switched to
 * English on a Japanese machine is not sent back every time they type the
 * bare domain.
 */
export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url)

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
