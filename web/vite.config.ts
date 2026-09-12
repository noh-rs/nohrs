import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import { tanstackStart } from '@tanstack/react-start/plugin/vite'
import viteReact from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import mdx from '@mdx-js/rollup'
import remarkFrontmatter from 'remark-frontmatter'
import remarkGfm from 'remark-gfm'
import rehypeSlug from 'rehype-slug'
import rehypeAutolinkHeadings from 'rehype-autolink-headings'
import rehypeShiki from '@shikijs/rehype'
// @ts-expect-error -- plain ESM helpers, typed through their JSDoc only
import remarkFrontmatterExport from './scripts/remark-frontmatter-export.mjs'
// @ts-expect-error -- see above
import { sitePaths, alternateRefs } from './scripts/content-index.mjs'
// @ts-expect-error -- see above
import pluginRegistry from './scripts/vite-plugin-registry.mjs'
// @ts-expect-error -- see above
import discussionDev from './scripts/vite-plugin-discussion.mjs'

const HOST = process.env.SITE_HOST ?? 'https://nohrs.app'

export default defineConfig({
  resolve: {
    alias: { '~': fileURLToPath(new URL('./app', import.meta.url)) },
  },
  plugins: [
    {
      // MDX has to compile before React sees the file, and Vite only honours
      // that ordering when the plugin declares it.
      enforce: 'pre',
      ...mdx({
        remarkPlugins: [remarkFrontmatter, remarkFrontmatterExport, remarkGfm],
        rehypePlugins: [
          rehypeSlug,
          [rehypeAutolinkHeadings, { behavior: 'wrap', properties: { className: 'heading-anchor' } }],
          // Highlighting runs at build time, so no highlighter ships to the browser.
          [rehypeShiki, { themes: { light: 'github-light', dark: 'github-dark' }, defaultColor: false }],
        ],
        providerImportSource: '@mdx-js/react',
      }),
    },
    pluginRegistry(),
    discussionDev(),
    tailwindcss(),
    tanstackStart({
      srcDirectory: 'app',
      // Resolved relative to `srcDirectory`.
      router: { routesDirectory: 'routes' },
      // Every page is content that changes when the repository changes, not
      // per-request — so the whole site is rendered once at build time and
      // served from the edge as static HTML. Pagefind needs that HTML too.
      prerender: { enabled: true, concurrency: 4, failOnError: true },
      sitemap: { enabled: true, host: HOST },
      pages: sitePaths().map(
        (page: { path: string; changefreq: string; priority: number; sitemap?: boolean }) => ({
          path: page.path,
          sitemap: {
            // `/` is prerendered but excluded: it is `noindex` and only picks
            // a language.
            exclude: page.sitemap === false,
            changefreq: page.changefreq,
            priority: page.priority,
            alternateRefs: alternateRefs(page.path, HOST),
          },
        }),
      ),
    }),
    viteReact(),
  ],
})
