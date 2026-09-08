import { strings, type Dict } from './strings'
import { CANONICAL_LANG, LANGS, isLang, negotiateLang, type Lang } from './negotiate'

export { CANONICAL_LANG, LANGS, isLang, negotiateLang }
export type { Lang }

export function t(lang: Lang): Dict {
  return strings[lang]
}

/** The same path in the other language, for the header's language toggle. */
export function swapLang(pathname: string, to: Lang): string {
  const rest = pathname.replace(/^\/(en|ja)(?=\/|$)/, '')
  return `/${to}${rest}`
}

export function formatDate(value: string, lang: Lang): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return new Intl.DateTimeFormat(lang === 'ja' ? 'ja-JP' : 'en-GB', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  }).format(date)
}
