import { SITE_URL } from '#/lib/links'
import type { Locale } from '#/lib/i18n'

interface SeoArgs {
  locale: Locale
  /** Path without the locale prefix, e.g. "" for landing or "/about". */
  path: string
  title: string
  description: string
}

/* Build a TanStack `head()` payload with canonical (always the en URL),
   hreflang alternates, and OG/Twitter meta (web.md §2.5 SEO gate). */
export function seoHead({ locale, path, title, description }: SeoArgs) {
  const enUrl = `${SITE_URL}/en${path}`
  const jaUrl = `${SITE_URL}/ja${path}`
  const currentUrl = `${SITE_URL}/${locale}${path}`

  return {
    meta: [
      { title },
      { name: 'description', content: description },
      { property: 'og:title', content: title },
      { property: 'og:description', content: description },
      { property: 'og:url', content: currentUrl },
      { property: 'og:locale', content: locale === 'ja' ? 'ja_JP' : 'en_US' },
      { name: 'twitter:title', content: title },
      { name: 'twitter:description', content: description },
    ],
    links: [
      { rel: 'canonical', href: enUrl },
      { rel: 'alternate', hrefLang: 'en', href: enUrl },
      { rel: 'alternate', hrefLang: 'ja', href: jaUrl },
      { rel: 'alternate', hrefLang: 'x-default', href: enUrl },
    ],
  }
}
