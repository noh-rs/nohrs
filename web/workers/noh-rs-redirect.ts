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

// `/p/<id>` resolves to a page the site actually renders, so it is expanded
// against a language. `/r/<tag>` does not: the site has a releases index but
// no per-release page, so a tag goes to the GitHub release it names rather
// than to a 404. docs/web.md §1 allows either destination.
const LANG_SHORTCUTS: Record<string, string> = {
  '/p/': '/plugins/',
}

const RELEASE_PREFIX = '/r/'
const GITHUB_RELEASE_TAG = 'https://github.com/noh-rs/nohrs/releases/tag/'

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

    if (url.pathname.startsWith(RELEASE_PREFIX)) {
      const tag = url.pathname.slice(RELEASE_PREFIX.length)
      if (tag) return permanent(`${GITHUB_RELEASE_TAG}${encodeURIComponent(tag)}`)
    }

    for (const [prefix, expansion] of Object.entries(LANG_SHORTCUTS)) {
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
