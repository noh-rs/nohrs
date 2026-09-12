import { createFileRoute } from '@tanstack/react-router'
import { DefinitionRow, DefinitionRows, PageHeader, Section } from '~/components/Page'
import { t, type Lang } from '~/lib/i18n'
import { seo } from '~/lib/seo'

export const Route = createFileRoute('/$lang/about')({
  head: ({ params }) => {
    const lang = params.lang as Lang
    const strings = t(lang).about
    return seo({ lang, path: '/about', title: strings.title, description: strings.lede })
  },
  component: About,
})

function About() {
  const { lang } = Route.useParams() as { lang: Lang }
  const strings = t(lang)

  return (
    <>
      <PageHeader eyebrow={strings.nav.about} title={strings.about.title} lede={strings.about.lede} />

      <Section eyebrow={strings.note.eyebrow} title={strings.note.title}>
        <div className="max-w-[65ch] text-[1.0625rem] text-ink-2">
          {strings.note.body.map((paragraph) => (
            <p key={paragraph} className="mt-0 mb-[1.1em] last:mb-0">
              {paragraph}
            </p>
          ))}
        </div>
      </Section>

      <Section title={strings.about.valuesTitle}>
        <div className="grid gap-px border border-line-soft bg-line-soft sm:grid-cols-2">
          {strings.about.values.map((value) => (
            <div key={value.title} className="bg-paper px-7 py-[30px]">
              <h3 className="mb-2 text-[1.0625rem]">{value.title}</h3>
              <p className="text-[0.9375rem] text-muted">{value.body}</p>
            </div>
          ))}
        </div>
      </Section>

      <Section title={strings.about.stackTitle}>
        <DefinitionRows>
          {strings.about.stack.map((entry) => (
            <DefinitionRow key={entry.label} label={entry.label}>
              {entry.body}
            </DefinitionRow>
          ))}
        </DefinitionRows>
      </Section>
    </>
  )
}
