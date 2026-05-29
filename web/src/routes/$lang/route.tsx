import { Outlet, createFileRoute, redirect } from '@tanstack/react-router'
import { LocaleProvider } from '#/lib/locale-context'
import { SiteHeader } from '#/components/site-header'
import { SiteFooter } from '#/components/site-footer'
import { defaultLocale, isLocale } from '#/lib/i18n'
import type { Locale } from '#/lib/i18n'

export const Route = createFileRoute('/$lang')({
  beforeLoad: ({ params, location }) => {
    if (!isLocale(params.lang)) {
      // Swap only the (invalid) locale segment, keeping the rest of the
      // path, search, and hash (e.g. /fr/download?x=1 -> /en/download?x=1).
      const href = location.href.replace(/^\/[^/?#]+/, `/${defaultLocale}`)
      throw redirect({ href })
    }
  },
  component: LangLayout,
})

function LangLayout() {
  const { lang } = Route.useParams()
  return (
    <LocaleProvider locale={lang as Locale}>
      <div className="flex min-h-screen flex-col">
        <SiteHeader />
        <main className="flex-1">
          <Outlet />
        </main>
        <SiteFooter />
      </div>
    </LocaleProvider>
  )
}
