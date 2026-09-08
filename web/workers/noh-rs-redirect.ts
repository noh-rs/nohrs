import { isLang, negotiateLang } from '../app/lib/negotiate.ts'

/**
 * `noh.rs` is a short domain, not a second site: everything it receives is
 * redirected to `nohrs.app` with the path preserved (docs/web.md §1).
 *
 * A path that already carries a language is a permanent mapping, so it gets a
 * 301 that caches forever. A path without one has to be resolved against
 * `Accept-Language` — `noh.rs/docs/installation` should reach the reader's own
 * language — so it gets a 302 that varies, because the answer is per-visitor
 * and a cached 301 would pin the first visitor's language onto everyone.
 */
const TARGET = 'https://nohrs.app'

const SHORTCUTS: Record<string, string> = {
  '/p/': '/plugins/',
  '/r/': '/releases/',
}

function permanent(location: string): Response {
  return new Response(null, { status: 301, headers: { Location: location } })
}

function negotiated(location: string): Response {
  return new Response(null, {
    status: 302,
    headers: { Location: location, Vary: 'Accept-Language', 'Cache-Control': 'no-store' },
  })
}

export default {
  fetch(request: Request): Response {
    const url = new URL(request.url)
    const lang = negotiateLang(request.headers.get('accept-language'))

    for (const [prefix, expansion] of Object.entries(SHORTCUTS)) {
      if (url.pathname.startsWith(prefix)) {
        const rest = url.pathname.slice(prefix.length)
        return negotiated(`${TARGET}/${lang}${expansion}${rest}${url.search}`)
      }
    }

    const first = url.pathname.split('/')[1]
    if (isLang(first)) return permanent(`${TARGET}${url.pathname}${url.search}`)
    if (url.pathname === '/') return permanent(`${TARGET}/${url.search}`)

    return negotiated(`${TARGET}/${lang}${url.pathname}${url.search}`)
  },
}
