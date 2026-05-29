/* noh.rs redirect Worker (web.md §1). Deployed separately from the main
   nohrs.app site, bound to the noh.rs zone. Keeps the path and query, and
   expands the short schemes /p/<id> and /r/<tag>. */

const TARGET = 'https://nohrs.app'

export default {
  fetch(request: Request): Response {
    const url = new URL(request.url)

    // Only expand the short scheme when a non-empty id/tag follows; otherwise
    // fall through to the plain path-preserving redirect.
    const id = url.pathname.startsWith('/p/') ? url.pathname.slice(3) : ''
    if (id) {
      return Response.redirect(`${TARGET}/plugins/${id}${url.search}`, 301)
    }
    const tag = url.pathname.startsWith('/r/') ? url.pathname.slice(3) : ''
    if (tag) {
      return Response.redirect(`${TARGET}/releases/${tag}${url.search}`, 301)
    }

    return Response.redirect(`${TARGET}${url.pathname}${url.search}`, 301)
  },
}
