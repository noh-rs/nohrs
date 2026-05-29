/* noh.rs redirect Worker (web.md §1). Deployed separately from the main
   nohrs.app site, bound to the noh.rs zone. Keeps the path and query, and
   expands the short schemes /p/<id> and /r/<tag>. */

const TARGET = 'https://nohrs.app'

export default {
  fetch(request: Request): Response {
    const url = new URL(request.url)

    if (url.pathname.startsWith('/p/')) {
      const id = url.pathname.slice(3)
      return Response.redirect(`${TARGET}/plugins/${id}${url.search}`, 301)
    }
    if (url.pathname.startsWith('/r/')) {
      const tag = url.pathname.slice(3)
      return Response.redirect(`${TARGET}/releases/${tag}${url.search}`, 301)
    }

    return Response.redirect(`${TARGET}${url.pathname}${url.search}`, 301)
  },
}
