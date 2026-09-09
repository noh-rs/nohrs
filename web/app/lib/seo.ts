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
  contentLang = lang,
}: {
  lang: Lang
  path: string
  title: string
  description: string
  image?: string
  type?: 'website' | 'article'
  publishedAt?: string
  noindex?: boolean
  /**
   * The language the text on the page is actually in. Differs from `lang`
   * when a translation is missing and the reader is shown the source, and
   * the canonical URL has to point at that version — otherwise two URLs each
   * claim to be the definitive copy of the same English text.
   */
  contentLang?: Lang
}): { meta: Meta; links: LinkTag } {
  // One URL for both `canonical` and `og:url`: a share card that names the
  // route while the page disowns it as a duplicate splits the counts a
  // sharing service keeps, and the two tags would contradict each other.
  const canonical = `${SITE.host}/${contentLang}${path}`
  const fullTitle = path === '' ? `${SITE.name} — ${title}` : `${title} — ${SITE.name}`

  const meta: Meta = [
    { title: fullTitle },
    { name: 'description', content: description },
    { property: 'og:type', content: type },
    { property: 'og:title', content: fullTitle },
    { property: 'og:description', content: description },
    { property: 'og:url', content: canonical },
    { property: 'og:image', content: `${SITE.host}${image}` },
    { property: 'og:locale', content: contentLang === 'ja' ? 'ja_JP' : 'en_US' },
    { name: 'twitter:title', content: fullTitle },
    { name: 'twitter:description', content: description },
    { name: 'twitter:image', content: `${SITE.host}${image}` },
  ]

  if (publishedAt) meta.push({ property: 'article:published_time', content: publishedAt })
  if (noindex) meta.push({ name: 'robots', content: 'noindex' })

  const links: LinkTag = [
    { rel: 'canonical', href: canonical },
    ...LANGS.map((alternate) => ({
      rel: 'alternate',
      hrefLang: alternate,
      href: `${SITE.host}/${alternate}${path}`,
    })),
    { rel: 'alternate', hrefLang: 'x-default', href: `${SITE.host}/en${path}` },
  ]

  return { meta, links }
}

/**
 * JSON-LD, emitted as a script tag from a route's `head()`.
 *
 * `<` is escaped because the JSON goes inside a `<script>` element, where a
 * `</script>` sequence in any string value would close the block early and
 * put the remainder into the document as markup.
 */
export function jsonLd(data: Record<string, unknown>) {
  return {
    type: 'application/ld+json',
    children: JSON.stringify(data).replace(/</g, '\\u003c'),
  }
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
