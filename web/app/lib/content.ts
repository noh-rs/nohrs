import type { ComponentType } from 'react'
import type { Lang } from './i18n'
import { CANONICAL_LANG } from './i18n'

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

/**
 * The one place the translation fallback is decided, for both collections.
 *
 * docs/web.md §4 requires full parity at launch, so this only covers the
 * window between publishing something and translating it. Within that window
 * the reader must still be able to reach the page: dropping it would turn a
 * missing translation into a 404 and, for a doc, into a hole in the sidebar.
 *
 * The source language is whichever the author wrote in — `canonical: ja` in a
 * post's frontmatter makes Japanese the original, so an untranslated English
 * route falls back to it rather than the other way round.
 */
function withFallback<T extends { lang: Lang; slug: string; canonical?: Lang }>(
  entries: T[],
  lang: Lang,
  slug: string,
): (T & { fallbackFrom?: Lang }) | undefined {
  const exact = entries.find((entry) => entry.lang === lang && entry.slug === slug)
  if (exact) return exact

  const sameSlug = entries.filter((entry) => entry.slug === slug)
  if (sameSlug.length === 0) return undefined

  const declared = sameSlug.find((entry) => entry.canonical && entry.lang === entry.canonical)
  const source = declared ?? sameSlug.find((entry) => entry.lang === CANONICAL_LANG) ?? sameSlug[0]
  return { ...source, fallbackFrom: source.lang }
}

/** Every slug in either language, so an untranslated entry still gets listed. */
function slugsFor<T extends { lang: Lang; slug: string }>(entries: T[]): string[] {
  return [...new Set(entries.map((entry) => entry.slug))]
}

export function blogPosts(lang: Lang): BlogPost[] {
  return slugsFor(allPosts)
    .map((slug) => blogPost(lang, slug))
    .filter((post): post is BlogPost => post !== undefined)
    .sort((a, b) => b.date.localeCompare(a.date))
}

export function blogPost(lang: Lang, slug: string): BlogPost | undefined {
  return withFallback(allPosts, lang, slug)
}

export function docPages(lang: Lang): DocPage[] {
  return slugsFor(allDocs)
    .map((slug) => docPage(lang, slug))
    .filter((page): page is DocPage => page !== undefined)
    .sort((a, b) => (a.order ?? 999) - (b.order ?? 999) || a.slug.localeCompare(b.slug))
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
