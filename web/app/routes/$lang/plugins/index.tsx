import { createFileRoute, Link } from '@tanstack/react-router'
import { PageHeader, Section } from '~/components/Page'
import { plugins } from '~/lib/content'
import { t, type Lang } from '~/lib/i18n'
import { seo } from '~/lib/seo'
import { SITE } from '~/lib/site'

export const Route = createFileRoute('/$lang/plugins/')({
  head: ({ params }) => {
    const lang = params.lang as Lang
    const strings = t(lang).plugins
    return seo({ lang, path: '/plugins', title: strings.title, description: strings.lede })
  },
  component: PluginsIndex,
})

function PluginsIndex() {
  const { lang } = Route.useParams() as { lang: Lang }
  const strings = t(lang).plugins

  return (
    <>
      <PageHeader eyebrow={t(lang).nav.plugins} title={strings.title} lede={strings.lede} />

      {/* The host does not exist yet. Saying so once, in prose, is the honest
          version of a "coming soon" badge on every card. */}
      <div className="frame">
        <p className="max-w-[62ch] border-l-2 border-tan pl-5 text-[0.9375rem] text-muted">
          {strings.previewNotice}
        </p>
      </div>

      <Section compact>
        {plugins.length === 0 ? (
          <p className="text-ink-2">{strings.empty}</p>
        ) : (
          // Cells carry their own borders rather than showing a grid-coloured
          // background through a 1px gap: an odd number of entries would leave
          // that background as a filled block where a card is missing.
          <div className="grid border-t border-l border-line-soft sm:grid-cols-2">
            {plugins.map((entry) => (
              <Link
                key={entry.id}
                to="/$lang/plugins/$id"
                params={{ lang, id: entry.id }}
                className="group border-r border-b border-line-soft px-7 py-[26px] no-underline"
              >
                <span className="font-mono text-[0.6875rem] tracking-[0.12em] text-muted uppercase">
                  {strings.categories[entry.category] ?? entry.category}
                </span>
                <h2 className="mt-2 text-[1.0625rem] transition-colors duration-200 group-hover:text-tan-ink">
                  {entry.name}
                </h2>
                <p className="mt-1.5 text-[0.9375rem] text-muted">{entry.summary[lang]}</p>
                <p className="mt-4 flex flex-wrap gap-x-4 font-mono text-[0.6875rem] text-muted">
                  {entry.permissions.map((permission) => (
                    <span key={permission}>{permission}</span>
                  ))}
                </p>
              </Link>
            ))}
          </div>
        )}
      </Section>

      <Section title={strings.submitTitle} lede={strings.submitBody}>
        <pre className="cmd max-w-[62ch]">
          <code>web/content/plugins/&lt;plugin-id&gt;.toml</code>
        </pre>
        <p className="mt-6 font-mono text-xs text-muted">
          <a href={`${SITE.repoUrl}/tree/develop/web/content/plugins`}>web/content/plugins ↗</a>
        </p>
      </Section>
    </>
  )
}
