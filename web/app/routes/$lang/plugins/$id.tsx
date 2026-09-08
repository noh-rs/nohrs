import { createFileRoute, Link, notFound } from '@tanstack/react-router'
import { DefinitionRow, DefinitionRows, PageHeader, Section } from '~/components/Page'
import { plugin } from '~/lib/content'
import { t, type Lang } from '~/lib/i18n'
import { seo } from '~/lib/seo'

export const Route = createFileRoute('/$lang/plugins/$id')({
  beforeLoad: ({ params }) => {
    if (!plugin(params.id)) throw notFound()
  },
  head: ({ params }) => {
    const lang = params.lang as Lang
    const entry = plugin(params.id)
    if (!entry) return {}
    return seo({
      lang,
      path: `/plugins/${entry.id}`,
      title: entry.name,
      description: entry.summary[lang],
    })
  },
  component: PluginDetail,
})

function PluginDetail() {
  const { lang, id } = Route.useParams() as { lang: Lang; id: string }
  const strings = t(lang).plugins
  const entry = plugin(id)
  if (!entry) return null

  return (
    <>
      <PageHeader
        eyebrow={strings.categories[entry.category] ?? entry.category}
        title={entry.name}
        lede={entry.summary[lang]}
      >
        <Link
          to="/$lang/plugins"
          params={{ lang }}
          className="mt-8 inline-flex items-center gap-2 font-mono text-xs text-muted no-underline hover:text-tan-ink"
        >
          <span aria-hidden="true">←</span>
          {strings.title}
        </Link>
      </PageHeader>

      <Section>
        <DefinitionRows>
          <DefinitionRow label={strings.source}>
            <a href={`https://github.com/${entry.repo}`}>{entry.repo} ↗</a>
          </DefinitionRow>
          <DefinitionRow label="Author">{entry.author}</DefinitionRow>
          <DefinitionRow label={strings.permissions}>
            <span className="flex flex-wrap gap-x-5 font-mono text-[0.8125rem]">
              {entry.permissions.map((permission) => (
                <span key={permission}>{permission}</span>
              ))}
            </span>
          </DefinitionRow>
          <DefinitionRow label={strings.install}>
            <span className="text-muted">{strings.installUnavailable}</span>
            <br />
            <code className="font-mono text-[0.8125rem]">nohrs://install?source={entry.repo}</code>
          </DefinitionRow>
        </DefinitionRows>
      </Section>
    </>
  )
}
