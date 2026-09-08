import { createFileRoute, notFound, Outlet } from '@tanstack/react-router'
import { useEffect } from 'react'
import { Header } from '~/components/Header'
import { Footer } from '~/components/Footer'
import { isLang, t, type Lang } from '~/lib/i18n'

export const Route = createFileRoute('/$lang')({
  beforeLoad: ({ params }) => {
    // `/de/...` is a 404, not a page in a third language with English text.
    if (!isLang(params.lang)) throw notFound()
  },
  component: LanguageLayout,
})

function LanguageLayout() {
  const { lang } = Route.useParams() as { lang: Lang }
  useAnchorScroll()

  return (
    <>
      <a
        href="#main"
        className="sr-only rounded-md border border-line bg-paper px-4 py-2 focus:not-sr-only focus:absolute focus:top-3 focus:left-3 focus:z-50"
      >
        {t(lang).nav.skipToContent}
      </a>
      <Header lang={lang} />
      <main id="main">
        <Outlet />
      </main>
      <Footer lang={lang} />
    </>
  )
}

/**
 * Eases in-page anchor jumps without putting `scroll-behavior: smooth` on the
 * root, which would also re-time keyboard and scrollbar scrolling. The wheel
 * stays entirely the browser's (ADR 0008, 改訂 2026-09-08).
 */
function useAnchorScroll() {
  useEffect(() => {
    const onClick = (event: MouseEvent) => {
      if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey) return
      const link = (event.target as HTMLElement | null)?.closest('a[href^="#"]')
      if (!(link instanceof HTMLAnchorElement)) return

      const id = link.getAttribute('href')?.slice(1)
      if (!id) return
      const target = document.getElementById(id)
      if (!target) return

      event.preventDefault()
      const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches
      target.scrollIntoView({ behavior: reduce ? 'auto' : 'smooth', block: 'start' })
      history.replaceState(null, '', `#${id}`)
    }

    document.addEventListener('click', onClick)
    return () => document.removeEventListener('click', onClick)
  }, [])
}
