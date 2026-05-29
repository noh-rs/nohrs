import { Outlet, createFileRoute, redirect } from '@tanstack/react-router'
import { LocaleProvider } from '#/lib/locale-context'
import { SiteHeader } from '#/components/site-header'
import { SiteFooter } from '#/components/site-footer'
import { defaultLocale, isLocale } from '#/lib/i18n'
import type { Locale } from '#/lib/i18n'

export const Route = createFileRoute('/$lang')({
  beforeLoad: ({ params }) => {
    if (!isLocale(params.lang)) {
      throw redirect({ to: '/$lang', params: { lang: defaultLocale } })
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
