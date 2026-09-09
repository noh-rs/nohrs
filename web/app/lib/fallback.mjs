/**
 * Which file answers a `(language, slug)` request.
 *
 * Plain JavaScript, and deliberately in one place, because two consumers need
 * the identical answer from different data sources: the app reads MDX modules
 * through Vite, and the build scripts read the same files with `fs` to decide
 * what to prerender and what goes in the sitemap and feeds. When those two
 * disagreed, a page the app was willing to render was never built — so the
 * fallback URL 404'd on a static deploy and was missing from the sitemap.
 */

/**
 * English is canonical: it carries the wider reach, and `x-default` points at it.
 *
 * Restated here rather than imported from `negotiate.ts` because the build
 * scripts run under plain `node`, which cannot load TypeScript. A test pins the
 * two together.
 */
export const CANONICAL_LANG = 'en'

/**
 * The entry to show, or undefined if the slug does not exist in any language.
 *
 * docs/web.md §4 requires full parity at launch, so a fallback only covers the
 * window between publishing something and translating it. Within that window
 * the reader still has to be able to reach it.
 *
 * The source is whichever language the author wrote in — `canonical: ja` in the
 * frontmatter makes Japanese the original, so an untranslated English route
 * falls back to it rather than the other way round.
 */
export function withFallback(entries, lang, slug) {
  const exact = entries.find((entry) => entry.lang === lang && entry.slug === slug)
  if (exact) return exact

  const sameSlug = entries.filter((entry) => entry.slug === slug)
  if (sameSlug.length === 0) return undefined

  const declared = sameSlug.find((entry) => entry.canonical && entry.lang === entry.canonical)
  const source = declared ?? sameSlug.find((entry) => entry.lang === CANONICAL_LANG) ?? sameSlug[0]
  return { ...source, fallbackFrom: source.lang }
}

/** Every slug in either language, resolved for one of them. */
export function localize(entries, lang) {
  const slugs = [...new Set(entries.map((entry) => entry.slug))]
  return slugs.map((slug) => withFallback(entries, lang, slug)).filter((entry) => entry !== undefined)
}
