import { createFileRoute, redirect } from '@tanstack/react-router'
import { createServerFn } from '@tanstack/react-start'
import { getRequestHeader } from '@tanstack/react-start/server'
import { defaultLocale, isLocale } from '#/lib/i18n'
import type { Locale } from '#/lib/i18n'

/* Pick a locale from the request's Accept-Language header (server only),
   falling back to the canonical default. The noh.rs Worker (web.md §1) and
   a future cookie remember the choice; this is the first-hit default. */
const detectLocale = createServerFn().handler((): Locale => {
  const header = getRequestHeader('accept-language') ?? ''
  for (const part of header.split(',')) {
    const tag = part.split(';')[0]?.trim().slice(0, 2).toLowerCase()
    if (tag && isLocale(tag)) return tag
  }
  return defaultLocale
})

export const Route = createFileRoute('/')({
  beforeLoad: async () => {
    let lang: Locale = defaultLocale
    try {
      lang = await detectLocale()
    } catch {
      lang = defaultLocale
    }
    throw redirect({ to: '/$lang', params: { lang }, statusCode: 302 })
  },
})
