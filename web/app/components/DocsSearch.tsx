import { useCallback, useEffect, useRef, useState } from 'react'
import { t, type Lang } from '~/lib/i18n'

type PagefindResult = {
  id: string
  data: () => Promise<{
    url: string
    meta: { title?: string }
    excerpt: string
  }>
}

type PagefindApi = {
  options: (options: Record<string, unknown>) => Promise<void>
  search: (term: string) => Promise<{ results: PagefindResult[] }>
}

type Hit = { id: string; url: string; title: string; excerpt: string }

/**
 * Pagefind indexes the prerendered HTML after the build and ships its own
 * runtime and index shards, so nothing here is in the app bundle and the
 * Japanese pages get CJK segmentation for free.
 *
 * The path is deliberately hidden from Vite: the file does not exist until
 * `postbuild` has run, and a static import would fail the build.
 */
const PAGEFIND_URL = '/_pagefind/pagefind.js'

export function DocsSearch({ lang }: { lang: Lang }) {
  const strings = t(lang).docs
  const [open, setOpen] = useState(false)
  const [term, setTerm] = useState('')
  const [hits, setHits] = useState<Hit[]>([])
  const [unavailable, setUnavailable] = useState(false)
  const input = useRef<HTMLInputElement>(null)
  const api = useRef<PagefindApi | null>(null)

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const typingElsewhere =
        event.target instanceof HTMLElement &&
        (event.target.tagName === 'INPUT' ||
          event.target.tagName === 'TEXTAREA' ||
          event.target.isContentEditable)

      if ((event.key === 'k' && (event.metaKey || event.ctrlKey)) || (event.key === '/' && !typingElsewhere)) {
        event.preventDefault()
        setOpen(true)
      }
      if (event.key === 'Escape') setOpen(false)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  useEffect(() => {
    if (!open) return
    input.current?.focus()
  }, [open])

  const load = useCallback(async () => {
    if (api.current) return api.current
    try {
      const module = (await import(/* @vite-ignore */ PAGEFIND_URL)) as unknown as PagefindApi
      await module.options({ excerptLength: 24 })
      api.current = module
      return module
    } catch {
      setUnavailable(true)
      return null
    }
  }, [])

  useEffect(() => {
    if (!open || term.trim().length < 2) {
      setHits([])
      return
    }
    let cancelled = false
    void (async () => {
      const pagefind = await load()
      if (!pagefind || cancelled) return
      // The index covers both language trees, and the locale is only known
      // once a result's data is fetched — so more than a page of results is
      // resolved and the filter runs before the limit. Slicing to the limit
      // first would show nothing whenever the top hits were all in the other
      // language.
      const response = await pagefind.search(term)
      const resolved = await Promise.all(response.results.slice(0, 40).map((result) => result.data()))
      if (cancelled) return
      setHits(
        resolved
          .filter((entry) => entry.url.startsWith(`/${lang}/`))
          .slice(0, 8)
          .map((entry, index) => ({
            id: `${index}-${entry.url}`,
            url: entry.url,
            title: entry.meta.title ?? entry.url,
            excerpt: entry.excerpt,
          })),
      )
    })()
    return () => {
      cancelled = true
    }
  }, [open, term, lang, load])

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        aria-label={strings.searchLabel}
        // Shown at every width: `/` is the only other way in, and a phone has
        // no key to press.
        className="inline-flex items-center gap-2 rounded-md border border-line px-2.5 py-[7px] font-mono text-xs whitespace-nowrap text-muted transition-colors duration-200 hover:border-tan hover:text-ink md:px-3"
      >
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.4" />
          <path d="m10.5 10.5 3 3" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </svg>
        <span className="hidden sm:inline">{strings.searchShort}</span>
        <span className="ml-2 hidden opacity-60 md:inline">/</span>
      </button>

      {open ? (
        <div
          className="fixed inset-0 z-50 flex items-start justify-center px-4 pt-[12vh]"
          style={{ background: 'color-mix(in srgb, var(--ink) 24%, transparent)' }}
          onClick={(event) => {
            if (event.target === event.currentTarget) setOpen(false)
          }}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-label={strings.searchLabel}
            className="w-full max-w-[560px] overflow-hidden rounded-lg border border-line bg-paper shadow-lg"
          >
            <input
              ref={input}
              value={term}
              onChange={(event) => setTerm(event.target.value)}
              placeholder={strings.searchPlaceholder}
              className="w-full border-b border-line-soft bg-transparent px-4 py-3.5 text-[15px] outline-none"
            />
            <div className="max-h-[52vh] overflow-y-auto">
              {unavailable ? (
                <p className="px-4 py-5 text-sm text-muted">
                  {/* The index only exists in a built site. */}
                  {strings.searchEmpty}
                </p>
              ) : hits.length === 0 ? (
                term.trim().length >= 2 ? (
                  <p className="px-4 py-5 text-sm text-muted">{strings.searchEmpty}</p>
                ) : (
                  <p className="px-4 py-5 font-mono text-xs text-muted">{strings.searchHint}</p>
                )
              ) : (
                hits.map((hit) => (
                  <a
                    key={hit.id}
                    href={hit.url}
                    className="block border-b border-line-soft px-4 py-3 no-underline last:border-b-0 hover:bg-surface"
                    onClick={() => setOpen(false)}
                  >
                    <span className="block text-sm font-medium text-ink">{hit.title}</span>
                    <span
                      className="mt-1 block text-[13px] text-muted [&_mark]:bg-tan-wash [&_mark]:text-ink"
                      dangerouslySetInnerHTML={{ __html: hit.excerpt }}
                    />
                  </a>
                ))
              )}
            </div>
          </div>
        </div>
      ) : null}
    </>
  )
}
