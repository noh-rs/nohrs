import { createFileRoute, Link } from '@tanstack/react-router'
import { DefinitionRow, DefinitionRows, PageHeader, Section } from '~/components/Page'
import { GitHubMark } from '~/components/Mark'
import { github, hasDownloads } from '~/lib/github'
import { formatDate, t, type Lang } from '~/lib/i18n'
import { seo } from '~/lib/seo'
import { SITE } from '~/lib/site'

export const Route = createFileRoute('/$lang/download')({
  head: ({ params }) => {
    const lang = params.lang as Lang
    const strings = t(lang).download
    return seo({
      lang,
      path: '/download',
      title: strings.title,
      description: hasDownloads ? strings.ledeReleased : strings.ledePreRelease,
    })
  },
  component: Download,
})

function Download() {
  const { lang } = Route.useParams() as { lang: Lang }
  const strings = t(lang)
  const latest = github.releases[0]

  return (
    <>
      <PageHeader
        eyebrow={strings.nav.download}
        title={strings.download.title}
        lede={hasDownloads ? strings.download.ledeReleased : strings.download.ledePreRelease}
      />

      {/* Nothing here offers a download while `releases` is empty: a button
          that lands on an empty page costs more trust than it buys clicks. */}
      {/* A release with no macOS asset attached would otherwise render a
          heading over an empty list. */}
      {latest && latest.assets.length > 0 ? (
        <Section eyebrow={`${latest.tag} · ${latest.date ? formatDate(latest.date, lang) : ''}`}>
          <div className="rows">
            {latest.assets.map((asset) => (
              <a key={asset.url} href={asset.url} className="row">
                <span className="rl">{(asset.size / 1_000_000).toFixed(1)} MB</span>
                <span className="rd">{asset.name}</span>
                <span className="ra" aria-hidden="true">
                  ↓
                </span>
              </a>
            ))}
          </div>
          <Link
            to="/$lang/releases"
            params={{ lang }}
            className="mt-8 inline-flex items-center gap-2 font-mono text-[0.8125rem] text-tan-ink no-underline"
          >
            {strings.releases.title}
            <span aria-hidden="true">→</span>
          </Link>
        </Section>
      ) : null}

      <Section title={strings.download.buildTitle} lede={strings.download.buildIntro}>
        <pre className="cmd max-w-[62ch]">
          <code>
            <span className="prompt">$</span>git clone https://github.com/noh-rs/nohrs{'\n'}
            <span className="prompt">$</span>cd nohrs{'\n'}
            <span className="prompt">$</span>cargo run -p nohrs --release
          </code>
        </pre>

        <h3 className="mt-12 mb-4 font-mono text-[0.6875rem] tracking-[0.14em] text-muted uppercase">
          {strings.download.requirementsTitle}
        </h3>
        <DefinitionRows>
          {strings.download.requirements.map((requirement) => (
            <DefinitionRow key={requirement.label} label={requirement.label}>
              {requirement.body}
            </DefinitionRow>
          ))}
        </DefinitionRows>
      </Section>

      <Section title={strings.download.watchTitle} lede={strings.download.watchBody}>
        <a href={`${SITE.repoUrl}/releases`} className="btn btn-primary">
          <GitHubMark />
          <span>{strings.download.watchCta}</span>
        </a>
      </Section>
    </>
  )
}
