import type { ComponentType } from 'react'
import type { Lang } from './i18n'
import { localize, withFallback } from './fallback.mjs'

export type BlogFrontmatter = {
  title: string
  date: string
  description: string
  author: string
  tags?: string[]
  canonical?: Lang
  og_image?: string
}

export type DocFrontmatter = {
  title: string
  description: string
  category: string
  order?: number
  canonical?: Lang
}

type MdxModule<F> = { default: ComponentType<Record<string, unknown>>; frontmatter: F }

/**
 * Eager globs: the whole corpus is a handful of files, every page is rendered
 * once at build time, and a lazy import would have to be resolved inside a
 * loader whose return value must survive SSR serialisation — which a React
 * component does not.
 */
const blogModules = import.meta.glob<MdxModule<BlogFrontmatter>>('../../content/*/blog/*.mdx', {
  eager: true,
})
const docModules = import.meta.glob<MdxModule<DocFrontmatter>>('../../content/*/docs/*.mdx', {
  eager: true,
})

function parseKey(key: string): { lang: Lang; slug: string } {
  const match = /\/content\/(en|ja)\/(?:blog|docs)\/(.+)\.mdx$/.exec(key)
  if (!match) throw new Error(`content file outside the expected layout: ${key}`)
  return { lang: match[1] as Lang, slug: match[2] }
}

export type BlogPost = BlogFrontmatter & {
  lang: Lang
  slug: string
  Body: ComponentType<Record<string, unknown>>
  /** Set when a reader is being shown another language's text as a fallback. */
  fallbackFrom?: Lang
}

export type DocPage = DocFrontmatter & {
  lang: Lang
  slug: string
  Body: ComponentType<Record<string, unknown>>
  /** Set when a reader is being shown another language's text as a fallback. */
  fallbackFrom?: Lang
}

const allPosts: BlogPost[] = Object.entries(blogModules).map(([key, module]) => {
  const { lang, slug } = parseKey(key)
  return { ...module.frontmatter, lang, slug, Body: module.default }
})

const allDocs: DocPage[] = Object.entries(docModules).map(([key, module]) => {
  const { lang, slug } = parseKey(key)
  return { ...module.frontmatter, lang, slug, Body: module.default }
})

export function blogPosts(lang: Lang): BlogPost[] {
  return localize(allPosts, lang).sort((a, b) => b.date.localeCompare(a.date))
}

export function blogPost(lang: Lang, slug: string): BlogPost | undefined {
  return withFallback(allPosts, lang, slug)
}

export function docPages(lang: Lang): DocPage[] {
  return localize(allDocs, lang).sort(
    (a, b) => (a.order ?? 999) - (b.order ?? 999) || a.slug.localeCompare(b.slug),
  )
}

export function docPage(lang: Lang, slug: string): DocPage | undefined {
  return withFallback(allDocs, lang, slug)
}

/** Doc pages grouped into the sidebar's categories, in `order` order. */
export function docSections(lang: Lang): { category: string; pages: DocPage[] }[] {
  const sections: { category: string; pages: DocPage[] }[] = []
  for (const page of docPages(lang)) {
    const section = sections.find((entry) => entry.category === page.category)
    if (section) section.pages.push(page)
    else sections.push({ category: page.category, pages: [page] })
  }
  return sections
}

export type Plugin = {
  id: string
  name: string
  summary: Record<Lang, string>
  category: string
  repo: string
  author: string
  permissions: string[]
  status: 'planned' | 'preview' | 'available'
}

import registry from 'virtual:nohrs-plugins'

export const plugins: Plugin[] = (registry as Plugin[])
  .slice()
  .sort((a, b) => a.name.localeCompare(b.name))

export function plugin(id: string): Plugin | undefined {
  return plugins.find((entry) => entry.id === id)
}
