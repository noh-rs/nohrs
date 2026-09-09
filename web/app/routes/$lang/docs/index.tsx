import { createFileRoute, Link } from '@tanstack/react-router'
import { docSections } from '~/lib/content'
import { t, type Lang } from '~/lib/i18n'
import { seo } from '~/lib/seo'

export const Route = createFileRoute('/$lang/docs/')({
  head: ({ params }) => {
    const lang = params.lang as Lang
    const strings = t(lang).docs
    return seo({ lang, path: '/docs', title: strings.title, description: strings.lede })
  },
  component: DocsIndex,
})

function DocsIndex() {
  const { lang } = Route.useParams() as { lang: Lang }
  const strings = t(lang).docs
  const sections = docSections(lang)

  return (
    <>
      <h1 className="text-[clamp(1.75rem,3.4vw,2.4rem)] leading-tight tracking-[-0.035em]">
        {strings.title}
      </h1>
      <p className="mt-4 max-w-[58ch] text-[1.0625rem] text-ink-2">{strings.lede}</p>

      <div className="mt-12">
        {sections.map((section) => (
          <section key={section.category} className="mb-10 last:mb-0">
            <h2 className="eyebrow">
              <span>{strings.categories[section.category] ?? section.category}</span>
            </h2>
            <div className="rows">
              {section.pages.map((page) => (
                <Link
                  key={page.slug}
                  to="/$lang/docs/$slug"
                  params={{ lang, slug: page.slug }}
                  className="row"
                >
                  <span className="rd text-ink">{page.title}</span>
                  <span className="hidden flex-[2] text-[0.9375rem] text-muted sm:block">
                    {page.description}
                  </span>
                  <span className="ra" aria-hidden="true">
                    →
                  </span>
                </Link>
              ))}
            </div>
          </section>
        ))}
      </div>
    </>
  )
}
