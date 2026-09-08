import { join } from 'node:path'
import { plugins, CONTENT_ROOT } from './content-index.mjs'

const VIRTUAL_ID = 'virtual:nohrs-plugins'
const RESOLVED_ID = `\0${VIRTUAL_ID}`

/**
 * The plugin registry is a directory of TOML files that contributors add to by
 * pull request (docs/web.md §6.5). Vite cannot import TOML, and neither the
 * browser nor the edge runtime should be parsing it, so it is read and frozen
 * into a module here, at build time.
 */
export default function pluginRegistry() {
  return {
    name: 'nohrs-plugin-registry',
    resolveId(id) {
      if (id === VIRTUAL_ID) return RESOLVED_ID
      return null
    },
    load(id) {
      if (id !== RESOLVED_ID) return null
      return `export default ${JSON.stringify(plugins())}`
    },
    configureServer(server) {
      const dir = join(CONTENT_ROOT, 'plugins')
      server.watcher.add(dir)
      const invalidate = (path) => {
        if (!path.startsWith(dir)) return
        const module = server.moduleGraph.getModuleById(RESOLVED_ID)
        if (module) server.moduleGraph.invalidateModule(module)
        server.ws.send({ type: 'full-reload' })
      }
      server.watcher.on('add', invalidate)
      server.watcher.on('change', invalidate)
      server.watcher.on('unlink', invalidate)
    },
  }
}
