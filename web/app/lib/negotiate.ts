export const LANGS = ['en', 'ja'] as const
export type Lang = (typeof LANGS)[number]

/** English is canonical: it carries the wider reach, and `x-default` points at it. */
export const CANONICAL_LANG: Lang = 'en'

export function isLang(value: unknown): value is Lang {
  return typeof value === 'string' && (LANGS as readonly string[]).includes(value)
}

/**
 * Picks a language from an `Accept-Language` header.
 *
 * This module deliberately imports nothing: the redirect Worker answers `/`
 * with it, and pulling the string dictionary into a Worker bundle to decide
 * between two two-letter codes would be absurd.
 */
export function negotiateLang(header: string | null | undefined): Lang {
  if (!header) return CANONICAL_LANG

  const ranked = header
    .split(',')
    .map((part) => {
      const [tag, ...params] = part.trim().split(';')
      const quality = params.find((param) => param.trim().startsWith('q='))
      return { tag: tag.toLowerCase(), q: quality ? Number.parseFloat(quality.split('=')[1]) : 1 }
    })
    .filter((entry) => Number.isFinite(entry.q))
    .sort((a, b) => b.q - a.q)

  for (const { tag } of ranked) {
    if (tag.startsWith('ja')) return 'ja'
    if (tag.startsWith('en')) return 'en'
  }
  return CANONICAL_LANG
}

export const LANG_COOKIE = 'nohrs-lang'

/** Reads the remembered choice out of a raw `Cookie` header. */
export function langFromCookie(header: string | null | undefined): Lang | null {
  if (!header) return null
  for (const part of header.split(';')) {
    const [name, ...rest] = part.trim().split('=')
    if (name === LANG_COOKIE) {
      const value = rest.join('=')
      return isLang(value) ? value : null
    }
  }
  return null
}
