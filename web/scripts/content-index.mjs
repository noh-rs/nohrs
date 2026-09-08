import { readdirSync, readFileSync, existsSync } from 'node:fs'
import { join, basename } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parse as parseYaml } from 'yaml'
import { parse as parseToml } from 'smol-toml'

export const WEB_ROOT = fileURLToPath(new URL('..', import.meta.url))
export const CONTENT_ROOT = join(WEB_ROOT, 'content')
export const LANGS = /** @type {const} */ (['en', 'ja'])

const FRONTMATTER = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/

/** Splits a `---`-delimited YAML header off the top of an MDX file. */
export function splitFrontmatter(source) {
  const match = FRONTMATTER.exec(source)
  if (!match) return { data: {}, body: source }
  return { data: parseYaml(match[1]) ?? {}, body: source.slice(match[0].length) }
}

function readCollection(lang, collection) {
  const dir = join(CONTENT_ROOT, lang, collection)
  if (!existsSync(dir)) return []
  return readdirSync(dir)
    .filter((name) => name.endsWith('.mdx'))
    .map((name) => {
      const slug = basename(name, '.mdx')
      const source = readFileSync(join(dir, name), 'utf8')
      const { data, body } = splitFrontmatter(source)
      return { slug, lang, collection, body, ...data }
    })
}

export function blogPosts(lang) {
  return readCollection(lang, 'blog').sort((a, b) => String(b.date).localeCompare(String(a.date)))
}

export function docPages(lang) {
  return readCollection(lang, 'docs').sort(
    (a, b) => (a.order ?? 999) - (b.order ?? 999) || a.slug.localeCompare(b.slug),
  )
}

export function plugins() {
  const dir = join(CONTENT_ROOT, 'plugins')
  if (!existsSync(dir)) return []
  return readdirSync(dir)
    .filter((name) => name.endsWith('.toml'))
    .map((name) => ({ id: basename(name, '.toml'), ...parseToml(readFileSync(join(dir, name), 'utf8')) }))
    .sort((a, b) => a.id.localeCompare(b.id))
}

/**
 * Every URL the site serves, as `{ path, changefreq, priority }`. Drives both
 * the prerender list and the sitemap, so the two can never drift apart.
 */
export function sitePaths() {
  /** @type {{path: string, changefreq: string, priority: number}[]} */
  const paths = [{ path: '/', changefreq: 'monthly', priority: 1 }]
  for (const lang of LANGS) {
    paths.push(
      { path: `/${lang}`, changefreq: 'weekly', priority: 1 },
      { path: `/${lang}/about`, changefreq: 'monthly', priority: 0.7 },
      { path: `/${lang}/download`, changefreq: 'weekly', priority: 0.9 },
      { path: `/${lang}/roadmap`, changefreq: 'weekly', priority: 0.8 },
      { path: `/${lang}/releases`, changefreq: 'weekly', priority: 0.8 },
      { path: `/${lang}/blog`, changefreq: 'weekly', priority: 0.8 },
      { path: `/${lang}/docs`, changefreq: 'weekly', priority: 0.8 },
      { path: `/${lang}/plugins`, changefreq: 'weekly', priority: 0.6 },
    )
    for (const post of blogPosts(lang)) {
      paths.push({ path: `/${lang}/blog/${post.slug}`, changefreq: 'monthly', priority: 0.6 })
    }
    for (const page of docPages(lang)) {
      paths.push({ path: `/${lang}/docs/${page.slug}`, changefreq: 'monthly', priority: 0.7 })
    }
    for (const plugin of plugins()) {
      paths.push({ path: `/${lang}/plugins/${plugin.id}`, changefreq: 'monthly', priority: 0.4 })
    }
  }
  return paths
}

/**
 * The hreflang set for a path. `/ja/blog/x` and `/en/blog/x` are alternates of
 * each other; `x-default` always points at the canonical language (en).
 */
export function alternateRefs(path, host) {
  if (path === '/') return []
  const rest = path.replace(/^\/(en|ja)/, '')
  return [
    ...LANGS.map((lang) => ({ hreflang: lang, href: `${host}/${lang}${rest}` })),
    { hreflang: 'x-default', href: `${host}/en${rest}` },
  ]
}
