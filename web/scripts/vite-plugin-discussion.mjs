/**
 * `/api/discussion` under `vite dev`.
 *
 * In production that route belongs to the Worker, and `vite dev` does not run
 * Workers — so without this the comments section sits in its error state for
 * the whole of local development. The Worker's own handler is loaded and
 * called here rather than reimplemented, so what runs locally is the code that
 * ships.
 *
 * `GITHUB_TOKEN` comes from the shell. Without one the handler answers 503,
 * which is the same thing a fork sees, and is also worth being able to look at.
 *
 * @returns {import('vite').Plugin}
 */
export default function discussionDev() {
  return {
    name: 'nohrs-discussion-dev',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use((request, reply, next) => {
        const url = new URL(request.url ?? '/', 'http://localhost')
        if (url.pathname !== '/api/discussion') return next()

        void (async () => {
          try {
            const worker = await server.ssrLoadModule('/workers/site.ts')
            const response = await worker.default.fetch(
              new Request(url, { method: request.method ?? 'GET' }),
              {
                ASSETS: { fetch: async () => new Response(null, { status: 404 }) },
                GITHUB_TOKEN: process.env.GITHUB_TOKEN,
              },
            )
            reply.statusCode = response.status
            for (const [name, value] of response.headers) reply.setHeader(name, value)
            reply.end(await response.text())
          } catch (error) {
            server.config.logger.error(`/api/discussion: ${error}`)
            reply.statusCode = 500
            reply.setHeader('content-type', 'application/json; charset=utf-8')
            reply.end(JSON.stringify({ error: 'dev' }))
          }
        })()
      })
    },
  }
}
