import { createFileRoute, Link } from '@tanstack/react-router'
import { PageHeader, Section } from '~/components/Page'
import { github } from '~/lib/github'
import { formatDate, t, type Lang } from '~/lib/i18n'
import { seo } from '~/lib/seo'

export const Route = createFileRoute('/$lang/releases')({
  head: ({ params }) => {
    const lang = params.lang as Lang
    const strings = t(lang).releases
    return seo({
      lang,
      path: '/releases',
      title: strings.title,
      // Saying "no release exists" in the meta description of a page that
      // lists releases is worse than saying nothing specific.
      description: github.releases.length === 0 ? strings.empty : strings.lede,
    })
  },
  component: Releases,
})

function Releases() {
  const { lang } = Route.useParams() as { lang: Lang }
  const strings = t(lang)

  return (
    <>
      <PageHeader eyebrow={strings.nav.releases} title={strings.releases.title} />

      <Section compact>
        {github.releases.length === 0 ? (
          <>
            <p className="max-w-[58ch] text-[1.0625rem] text-ink-2">{strings.releases.empty}</p>
            <Link
              to="/$lang/roadmap"
              params={{ lang }}
              className="mt-8 inline-flex items-center gap-2 font-mono text-[0.8125rem] text-tan-ink no-underline"
            >
              {strings.releases.emptyCta}
              <span aria-hidden="true">→</span>
            </Link>
          </>
        ) : (
          <div className="border-t border-line">
            {github.releases.map((release) => (
              <a
                key={release.tag}
                href={release.url}
                className="grid items-baseline gap-x-6 gap-y-1.5 border-b border-line-soft py-6 no-underline transition-[padding] duration-200 last:border-line hover:bg-surface hover:pl-3.5 md:grid-cols-[110px_120px_minmax(0,1fr)_auto] max-md:grid-cols-1"
              >
                <span className="font-mono text-sm font-medium text-ink tabular-nums">{release.tag}</span>
                <span className="font-mono text-sm text-muted tabular-nums">
                  {release.date ? formatDate(release.date, lang) : ''}
                </span>
                <span className="text-[0.9375rem] text-ink-2">
                  {release.highlight ?? release.name}
                  {release.prerelease ? (
                    <span className="ml-3 font-mono text-xs text-tan-ink">{strings.releases.prerelease}</span>
                  ) : null}
                </span>
                <span className="font-mono text-xs text-muted">{strings.releases.viewOnGitHub} ↗</span>
              </a>
            ))}
          </div>
        )}
      </Section>
    </>
  )
}
