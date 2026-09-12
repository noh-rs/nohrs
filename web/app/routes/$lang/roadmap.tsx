import { createFileRoute } from '@tanstack/react-router'
import { PageHeader, Section } from '~/components/Page'
import { Phases } from '~/components/Phases'
import { t, type Lang } from '~/lib/i18n'
import { seo } from '~/lib/seo'
import { SITE } from '~/lib/site'

export const Route = createFileRoute('/$lang/roadmap')({
  head: ({ params }) => {
    const lang = params.lang as Lang
    const strings = t(lang).roadmap
    return seo({ lang, path: '/roadmap', title: strings.pageTitle, description: strings.pageLede })
  },
  component: Roadmap,
})

function Roadmap() {
  const { lang } = Route.useParams() as { lang: Lang }
  const strings = t(lang).roadmap

  return (
    <>
      <PageHeader eyebrow={strings.eyebrow} title={strings.pageTitle} lede={strings.pageLede} />

      <Section compact>
        <Phases lang={lang} />
        <p className="mt-8 font-mono text-xs text-muted">
          <a href={`${SITE.repoUrl}/blob/develop/docs/ROADMAP.md`}>docs/ROADMAP.md ↗</a>
        </p>
      </Section>

      <Section title={strings.versioningTitle}>
        <ol className="border-t border-line">
          {strings.versioning.map((item, index) => (
            <li
              key={item}
              className="grid gap-x-6 gap-y-1 border-b border-line-soft py-[19px] sm:grid-cols-[48px_minmax(0,1fr)]"
            >
              <span className="font-mono text-sm text-muted tabular-nums">
                {String(index + 1).padStart(2, '0')}
              </span>
              <span className="max-w-[65ch] text-[0.9375rem] text-ink-2">{item}</span>
            </li>
          ))}
        </ol>
      </Section>
    </>
  )
}
