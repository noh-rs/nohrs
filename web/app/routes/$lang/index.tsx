import { createFileRoute, Link } from '@tanstack/react-router'
import { HeroOrbit } from '~/components/HeroOrbit'
import { GitHubMark } from '~/components/Mark'
import { Phases } from '~/components/Phases'
import { Reveal } from '~/components/Reveal'
import { Section } from '~/components/Page'
import { github, hasDownloads } from '~/lib/github'
import { formatDate, t, type Lang } from '~/lib/i18n'
import { jsonLd, seo, softwareApplicationLd } from '~/lib/seo'
import { SITE } from '~/lib/site'

export const Route = createFileRoute('/$lang/')({
  head: ({ params }) => {
    const lang = params.lang as Lang
    const strings = t(lang)
    const { meta, links } = seo({
      lang,
      path: '',
      title: 'Launcher × Explorer',
      description: strings.hero.sub,
    })
    return { meta, links, scripts: [jsonLd(softwareApplicationLd(lang, strings.hero.sub))] }
  },
  component: Landing,
})

function Landing() {
  const { lang } = Route.useParams() as { lang: Lang }
  const strings = t(lang)

  return (
    <>
      <Hero lang={lang} />

      <Section id="note" eyebrow={strings.note.eyebrow} title={strings.note.title}>
        <div className="max-w-[65ch] text-[1.0625rem] text-ink-2">
          {strings.note.body.map((paragraph) => (
            <p key={paragraph} className="mt-0 mb-[1.1em] last:mb-0">
              {paragraph}
            </p>
          ))}
        </div>
      </Section>

      <Section id="why" eyebrow={strings.why.eyebrow} title={strings.why.title}>
        <Reveal className="grid gap-px border border-line-soft bg-line-soft sm:grid-cols-2">
          {strings.why.pillars.map((pillar) => (
            <div key={pillar.title} className="bg-paper px-7 py-[30px]">
              <h3 className="mb-2 text-[1.0625rem]">{pillar.title}</h3>
              <p className="text-[0.9375rem] text-muted">{pillar.body}</p>
            </div>
          ))}
        </Reveal>
      </Section>

      <Preview lang={lang} />

      <Section
        id="roadmap"
        eyebrow={strings.roadmap.eyebrow}
        title={strings.roadmap.title}
        lede={strings.roadmap.lede}
      >
        <Reveal>
          <Phases lang={lang} />
        </Reveal>
        <Link
          to="/$lang/roadmap"
          params={{ lang }}
          className="mt-8 inline-flex items-center gap-2 font-mono text-[0.8125rem] text-tan-ink no-underline"
        >
          {strings.roadmap.pageTitle}
          <span aria-hidden="true">→</span>
        </Link>
      </Section>

      <OpenSource lang={lang} />
      <Community lang={lang} />
    </>
  )
}

function Hero({ lang }: { lang: Lang }) {
  const strings = t(lang)

  return (
    <section className="border-t-0 pb-[clamp(48px,6vw,84px)]">
      <HeroOrbit lang={lang}>
        {/* `Launcher ×` is held together on one line. The × is an
            inline-block, which is a break opportunity on its own — a
            non-breaking space does not close it — and a × alone at the head
            of the second line reads as a bullet, not as an operator. */}
        <h1 className="m-0 leading-[1.04] font-semibold tracking-[-0.045em]">
          <span className="whitespace-nowrap">
            Launcher{' '}
            <span className="inline-block px-[0.1em] align-[0.035em] font-mono text-[0.74em] font-normal tracking-normal text-tan-ink">
              ×
            </span>
          </span>{' '}
          Explorer
        </h1>

        <p className="text-ink-2">
          {strings.hero.sub}
        </p>

        <div className="orbit-cta flex flex-wrap justify-center gap-3">
          {hasDownloads ? (
            <Link to="/$lang/download" params={{ lang }} className="btn btn-primary">
              {strings.nav.download}
            </Link>
          ) : (
            <a href={SITE.repoUrl} className="btn btn-primary">
              <GitHubMark />
              <span>{strings.nav.star}</span>
              <span className="count">{github.repo.stars}</span>
            </a>
          )}
          <a href={SITE.discordUrl} className="btn btn-ghost">
            <span>{strings.nav.joinDiscord}</span>
            <span aria-hidden="true">↗</span>
          </a>
        </div>
      </HeroOrbit>

      {/* Without a published binary, nothing else on the page answers
          "so how do I try this today". */}
      <div className="frame">
        <div className="mx-auto mt-[clamp(30px,4vw,58px)] max-w-[62ch] border-t border-line pt-6.5">
          <p className="mb-3.5 text-sm text-muted">
            {hasDownloads ? strings.hero.tryTitleReleased : strings.hero.tryTitle}
          </p>
          <pre className="cmd">
            <code>
              <span className="prompt">$</span>git clone https://github.com/noh-rs/nohrs{'\n'}
              <span className="prompt">$</span>cd nohrs{'\n'}
              <span className="prompt">$</span>cargo run -p nohrs
            </code>
          </pre>
        </div>
      </div>
    </section>
  )
}

function Preview({ lang }: { lang: Lang }) {
  const strings = t(lang).preview

  return (
    <Section id="preview" eyebrow={strings.eyebrow} title={strings.title} lede={strings.lede}>
      {/* The screenshot already contains a real macOS window, with its own
          traffic lights and shadow. Framing it adds a second window around the
          first, so it is placed bare and received by a hairline caption. */}
      <Reveal as="figure" className="m-0">
        <img
          src="/screenshot-explorer.jpg"
          alt={strings.caption}
          width={1106}
          height={709}
          loading="lazy"
          decoding="async"
          className="block h-auto w-full"
        />
      </Reveal>
      <p className="mt-3.5 font-mono text-xs leading-relaxed text-muted">{strings.caption}</p>

      <div className="mt-11 border-t border-line">
        {strings.upcoming.map((item) => (
          <div
            key={item.title}
            className="grid items-baseline gap-x-6 gap-y-1.5 border-b border-line-soft py-[19px] last:border-line md:grid-cols-[48px_minmax(0,1fr)_minmax(0,1.6fr)] max-md:grid-cols-[48px_minmax(0,1fr)]"
          >
            <span className="font-mono text-sm leading-relaxed font-medium text-muted">{item.phase}</span>
            <span className="text-[0.9375rem] text-ink max-md:col-start-2">{item.title}</span>
            <span className="text-[0.9375rem] text-muted max-md:col-start-2">{item.body}</span>
          </div>
        ))}
      </div>
    </Section>
  )
}

function OpenSource({ lang }: { lang: Lang }) {
  const strings = t(lang).openSource
  const figures = [
    { value: String(github.repo.stars), label: strings.stars },
    { value: String(github.repo.forks), label: strings.forks },
    { value: String(github.repo.openIssues), label: strings.issues },
    { value: github.repo.license, label: strings.license },
  ]

  return (
    <Section id="opensource" eyebrow={strings.eyebrow} title={strings.title} lede={strings.lede}>
      <Reveal className="mb-9 grid grid-cols-2 gap-7 sm:grid-cols-4">
        {figures.map((figure) => (
          <div key={figure.label}>
            <div className="font-mono text-[clamp(1.75rem,3.4vw,2.5rem)] leading-none font-medium tracking-[-0.03em] tabular-nums">
              {figure.value}
            </div>
            <div className="mt-2.5 font-mono text-xs leading-none tracking-[0.1em] text-muted uppercase">
              {figure.label}
            </div>
          </div>
        ))}
      </Reveal>

      <h3 className="mt-12 mb-0 font-mono text-[0.6875rem] tracking-[0.14em] text-muted uppercase">
        {strings.recent}
      </h3>
      <div className="mt-4 rows">
        {github.commits.map((commit) => (
          <a key={commit.sha} href={commit.url} className="row">
            <span className="rl tabular-nums">{commit.sha}</span>
            <span className="rd">{commit.title}</span>
            <span className="hidden font-mono text-xs text-muted sm:inline">
              {formatDate(commit.date, lang)}
            </span>
            <span className="ra" aria-hidden="true">
              ↗
            </span>
          </a>
        ))}
      </div>

      <p className="mt-6 font-mono text-xs text-muted">
        {strings.fetched} {github.fetchedAt}
      </p>
    </Section>
  )
}

function Community({ lang }: { lang: Lang }) {
  const strings = t(lang).community
  const rows = [
    { label: 'GitHub', body: strings.github, href: SITE.repoUrl },
    { label: 'Discord', body: strings.discord, href: SITE.discordUrl },
    { label: 'X', body: strings.x, href: SITE.xUrl },
  ]

  return (
    <Section id="community" eyebrow={strings.eyebrow} title={strings.title}>
      <Reveal className="mt-8 rows">
        {rows.map((row) => (
          <a key={row.label} href={row.href} className="row">
            <span className="rl">{row.label}</span>
            <span className="rd">{row.body}</span>
            <span className="ra" aria-hidden="true">
              ↗
            </span>
          </a>
        ))}
      </Reveal>
    </Section>
  )
}
