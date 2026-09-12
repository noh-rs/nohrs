export type FallbackLang = 'en' | 'ja'

export declare const CANONICAL_LANG: FallbackLang

type Entry = { lang: FallbackLang; slug: string; canonical?: FallbackLang }

export declare function withFallback<T extends Entry>(
  entries: T[],
  lang: FallbackLang,
  slug: string,
): (T & { fallbackFrom?: FallbackLang }) | undefined

export declare function localize<T extends Entry>(
  entries: T[],
  lang: FallbackLang,
): (T & { fallbackFrom?: FallbackLang })[]
