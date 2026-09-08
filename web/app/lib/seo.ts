import { LANGS, type Lang } from './i18n'
import { SITE } from './site'

type Meta = Record<string, string>[]
type LinkTag = Record<string, string>[]

/**
 * Head tags for one page, in one language.
 *
 * `path` is the language-less remainder ('' for the landing page, '/docs' for
 * the docs index), which is what lets canonical, `hreflang` and the OG URL all
 * be derived from a single argument instead of being spelled out per route and
 * drifting.
 */
export function seo({
  lang,
  path,
  title,
  description,
  image = '/og/default.png',
  type = 'website',
  publishedAt,
  noindex = false,
}: {
  lang: Lang
  path: string
  title: string
  description: string
  image?: string
  type?: 'website' | 'article'
  publishedAt?: string
  noindex?: boolean
}): { meta: Meta; links: LinkTag } {
  const url = `${SITE.host}/${lang}${path}`
  const fullTitle = path === '' ? `${SITE.name} — ${title}` : `${title} — ${SITE.name}`

  const meta: Meta = [
    { title: fullTitle },
    { name: 'description', content: description },
    { property: 'og:type', content: type },
    { property: 'og:title', content: fullTitle },
    { property: 'og:description', content: description },
    { property: 'og:url', content: url },
    { property: 'og:image', content: `${SITE.host}${image}` },
    { property: 'og:locale', content: lang === 'ja' ? 'ja_JP' : 'en_US' },
    { name: 'twitter:title', content: fullTitle },
    { name: 'twitter:description', content: description },
    { name: 'twitter:image', content: `${SITE.host}${image}` },
  ]

  if (publishedAt) meta.push({ property: 'article:published_time', content: publishedAt })
  if (noindex) meta.push({ name: 'robots', content: 'noindex' })

  const links: LinkTag = [
    { rel: 'canonical', href: url },
    ...LANGS.map((alternate) => ({
      rel: 'alternate',
      hrefLang: alternate,
      href: `${SITE.host}/${alternate}${path}`,
    })),
    { rel: 'alternate', hrefLang: 'x-default', href: `${SITE.host}/en${path}` },
  ]

  return { meta, links }
}

/** JSON-LD, emitted as a script tag from a route's `head()`. */
export function jsonLd(data: Record<string, unknown>) {
  return { type: 'application/ld+json', children: JSON.stringify(data) }
}

export function softwareApplicationLd(lang: Lang, description: string) {
  return {
    '@context': 'https://schema.org',
    '@type': 'SoftwareApplication',
    name: SITE.name,
    applicationCategory: 'DeveloperApplication',
    operatingSystem: 'macOS',
    description,
    url: `${SITE.host}/${lang}`,
    license: 'https://opensource.org/licenses/MIT',
    isAccessibleForFree: true,
    codeRepository: SITE.repoUrl,
    programmingLanguage: 'Rust',
  }
}
