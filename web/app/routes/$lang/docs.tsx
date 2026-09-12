import { createFileRoute, Link, Outlet } from '@tanstack/react-router'
import { docSections } from '~/lib/content'
import { t, type Lang } from '~/lib/i18n'

export const Route = createFileRoute('/$lang/docs')({
  component: DocsLayout,
})

function DocsLayout() {
  const { lang } = Route.useParams() as { lang: Lang }
  const strings = t(lang).docs
  const sections = docSections(lang)

  return (
    <div className="frame grid gap-x-12 gap-y-8 pt-[clamp(40px,5vw,64px)] pb-20 lg:grid-cols-[228px_minmax(0,1fr)]">
      {/* The sidebar is a list of hairline-separated links, not a tree with
          disclosure arrows: four categories do not need machinery. */}
      <nav className="lg:sticky lg:top-[84px] lg:self-start" aria-label={strings.title}>
        {sections.map((section) => (
          <div key={section.category} className="mb-7 last:mb-0">
            <h2 className="mb-2.5 font-mono text-[0.6875rem] tracking-[0.12em] text-muted uppercase">
              {strings.categories[section.category] ?? section.category}
            </h2>
            {section.pages.map((page) => (
              <Link
                key={page.slug}
                to="/$lang/docs/$slug"
                params={{ lang, slug: page.slug }}
                className="block border-l border-line py-[5px] pl-3.5 text-sm text-muted no-underline transition-colors duration-200 hover:text-ink [&.active]:border-tan-ink [&.active]:text-ink"
                activeProps={{ className: 'active' }}
              >
                {page.title}
              </Link>
            ))}
          </div>
        ))}
      </nav>

      <div className="min-w-0">
        <Outlet />
      </div>
    </div>
  )
}
