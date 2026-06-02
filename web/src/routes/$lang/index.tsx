import { Link, createFileRoute } from '@tanstack/react-router'
import {
  ArrowRight,
  Command,
  FolderTree,
  Github,
  MessagesSquare,
  Puzzle,
  Search,
} from 'lucide-react'
import { useLocale } from '#/lib/locale-context'
import { Reveal } from '#/components/reveal'
import { buttonClasses } from '#/components/ui/button'
import { Kbd } from '#/components/ui/kbd'
import {
  AvatarMarquee,
  EngineTabs,
  ExplorerSketch,
  IsoFigure,
  LauncherMock,
  PluginsMock,
  SearchMock,
} from '#/components/landing-visuals'
import type { Contributor } from '#/components/landing-visuals'
import { defaultLocale, getMessages, isLocale } from '#/lib/i18n'
import { seoHead } from '#/lib/seo'
import { GITHUB_DISCUSSIONS, GITHUB_REPO, SITE_URL } from '#/lib/links'
import { cn } from '#/lib/utils'

type OssStats = {
  stars: number
  forks: number
  contributorCount: number
  issues: number
  contributors: Array<Contributor>
} | null

/* Live GitHub stats for the open-source band (SSR loader). Unauthenticated, so
   it can be rate-limited in production — every field degrades gracefully and
   the UI falls back to em-dashes / placeholder avatars when this returns null
   rather than showing anything fabricated. */
async function loadOssStats(): Promise<OssStats> {
  const headers = {
    Accept: 'application/vnd.github+json',
    'User-Agent': 'nohrs-web',
  }
  try {
    const [repoResponse, contributorsResponse] = await Promise.all([
      fetch('https://api.github.com/repos/noh-rs/nohrs', { headers }),
      fetch(
        'https://api.github.com/repos/noh-rs/nohrs/contributors?per_page=100',
        { headers },
      ),
    ])
    if (!repoResponse.ok) return null
    const repo = await repoResponse.json()
    const rawContributors = contributorsResponse.ok
      ? await contributorsResponse.json()
      : []
    const contributors: Array<Contributor> = Array.isArray(rawContributors)
      ? rawContributors.slice(0, 40).map((person) => ({
          login: String(person.login),
          avatarUrl: String(person.avatar_url),
          htmlUrl: String(person.html_url),
        }))
      : []
    return {
      stars: repo.stargazers_count ?? 0,
      forks: repo.forks_count ?? 0,
      issues: repo.open_issues_count ?? 0,
      contributorCount: Array.isArray(rawContributors)
        ? rawContributors.length
        : 0,
      contributors,
    }
  } catch {
    return null
  }
}

const compact = new Intl.NumberFormat('en', {
  notation: 'compact',
  maximumFractionDigits: 1,
})

export const Route = createFileRoute('/$lang/')({
  loader: () => loadOssStats(),
  staleTime: 5 * 60 * 1000,
  head: ({ params }) => {
    const locale = isLocale(params.lang) ? params.lang : defaultLocale
    const t = getMessages(locale)
    const base = seoHead({
      locale,
      path: '',
      title: `nohrs — ${t.hero.title}`,
      description: t.hero.subtitle,
    })
    return {
      ...base,
      scripts: [
        {
          type: 'application/ld+json',
          children: JSON.stringify({
            '@context': 'https://schema.org',
            '@type': 'SoftwareApplication',
            name: 'nohrs',
            applicationCategory: 'DeveloperApplication',
            operatingSystem: 'macOS',
            description: t.hero.subtitle,
            url: SITE_URL,
            offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
          }),
        },
      ],
    }
  },
  component: Landing,
})

function Section({
  id,
  className,
  children,
}: {
  id?: string
  className?: string
  children: React.ReactNode
}) {
  return (
    <section
      id={id}
      className={cn('mx-auto max-w-6xl px-4 sm:px-6', className)}
    >
      {children}
    </section>
  )
}

function Eyebrow({
  n,
  children,
  className,
}: {
  n: string
  children: React.ReactNode
  className?: string
}) {
  return (
    <p className={cn('eyebrow', className)}>
      {n ? <span className="text-muted-foreground">{n}</span> : null}
      <span aria-hidden className="h-px w-6 bg-brand/60" />
      {children}
    </p>
  )
}

function SectionHead({
  n,
  kicker,
  title,
  subtitle,
}: {
  n: string
  kicker: string
  title: string
  subtitle?: string
}) {
  return (
    <Reveal>
      <Eyebrow n={n}>{kicker}</Eyebrow>
      <h2 className="mt-4 max-w-3xl text-balance text-3xl font-semibold tracking-tight text-ink sm:text-4xl">
        {title}
      </h2>
      {subtitle ? (
        <p className="mt-3 max-w-2xl text-muted-foreground">{subtitle}</p>
      ) : null}
    </Reveal>
  )
}

const featureIcons: Record<string, typeof Puzzle | undefined> = {
  Explorer: FolderTree,
  Launcher: Command,
  Plugins: Puzzle,
  Search: Search,
}

const featureMocks: Record<string, (() => React.ReactNode) | undefined> = {
  Explorer: ExplorerSketch,
  Launcher: LauncherMock,
  Plugins: PluginsMock,
  Search: SearchMock,
}

const isoVariants = ['keys', 'sandbox', 'stack'] as const

function Landing() {
  const { locale, t } = useLocale()
  const stats = Route.useLoaderData()

  const statValues: Array<string> = stats
    ? [
        compact.format(stats.stars),
        compact.format(stats.forks),
        compact.format(stats.contributorCount),
        compact.format(stats.issues),
      ]
    : t.opensource.stats.map((s) => s.value)

  return (
    <>
      {/* 1. Hero */}
      <Section className="relative pt-16 pb-12 text-center sm:pt-24 sm:pb-16">
        <div aria-hidden className="bg-grid absolute inset-0 -z-10" />
        <div
          aria-hidden
          className="glow-brand absolute left-1/2 top-0 -z-10 h-[420px] w-[680px] max-w-full -translate-x-1/2"
        />
        <Reveal>
          <span className="inline-flex items-center gap-2 font-mono text-xs uppercase tracking-[0.12em] text-brand-emphasis">
            <span aria-hidden className="size-1.5 rounded-full bg-brand" />
            {t.hero.badge}
          </span>
          <h1 className="mx-auto mt-6 max-w-3xl text-balance text-5xl font-semibold tracking-[-0.02em] text-ink sm:text-7xl">
            {t.hero.title}
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-pretty text-lg text-muted-foreground">
            {t.hero.subtitle}
          </p>
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <Link
              to="/$lang/download"
              params={{ lang: locale }}
              className={buttonClasses('brand', 'lg')}
            >
              {t.hero.download}
            </Link>
            <a
              href={GITHUB_REPO}
              target="_blank"
              rel="noreferrer noopener"
              className={buttonClasses('ghost', 'lg')}
            >
              <Github className="size-5" />
              {t.hero.viewGithub}
            </a>
          </div>
        </Reveal>

        {/* Frameless product shot: the real Explorer, lifted off the page. */}
        <Reveal delay={0.08} className="relative mt-14 sm:mt-16">
          <div
            aria-hidden
            className="glow-brand absolute left-1/2 top-6 -z-10 h-64 w-[760px] max-w-full -translate-x-1/2"
          />
          <figure className="mx-auto max-w-4xl">
            <img
              src="/screenshot-explorer.jpeg"
              alt={t.hero.screenshotAlt}
              width={1280}
              height={800}
              loading="eager"
              className="mx-auto w-full rounded-xl ring-1 ring-border/70 shadow-[0_40px_90px_-32px_color-mix(in_oklab,#2a1b0f_38%,transparent)]"
            />
            <figcaption className="mt-5 text-sm text-muted-foreground">
              {t.hero.screenshotCaption}
            </figcaption>
          </figure>
        </Reveal>
      </Section>

      {/* 2. Why nohrs — editorial index rows */}
      <Section id="features" className="scroll-mt-20 py-16 sm:py-24">
        <SectionHead
          n="01"
          kicker={t.kickers.why}
          title={t.why.heading}
          subtitle={t.why.subheading}
        />
        <div className="mt-12">
          {t.why.points.map((point, i) => (
            <Reveal key={point.title} delay={i * 0.04}>
              <div className="grid items-baseline gap-x-8 gap-y-2 border-t border-border py-7 md:grid-cols-[6rem_minmax(0,18rem)_1fr]">
                <span className="font-mono text-sm text-brand-emphasis">
                  {String(i + 1).padStart(2, '0')}
                </span>
                <h3 className="text-lg font-semibold text-ink">
                  {point.title}
                </h3>
                <p className="text-pretty text-muted-foreground">
                  {point.body}
                </p>
              </div>
            </Reveal>
          ))}
        </div>
      </Section>

      {/* 3. Principles — three columns with isometric figures */}
      <Section className="py-16 sm:py-24">
        <Reveal>
          <Eyebrow n="02">{t.kickers.identity}</Eyebrow>
          <h2 className="mt-5 max-w-4xl text-balance text-3xl font-semibold leading-[1.18] tracking-tight sm:text-[2.6rem]">
            <span className="text-ink">{t.principles.lead}</span>{' '}
            <span className="text-muted-foreground">{t.principles.trail}</span>
          </h2>
        </Reveal>
        <div className="mt-14 grid gap-x-8 gap-y-12 sm:grid-cols-3">
          {t.principles.items.map((item, i) => (
            <Reveal key={item.title} delay={i * 0.07}>
              <div className="flex flex-col border-t border-border pt-5">
                <span className="font-mono text-xs tracking-widest text-muted-foreground">
                  {item.fig}
                </span>
                <IsoFigure variant={isoVariants[i] ?? 'stack'} />
                <h3 className="mt-2 font-semibold text-ink">{item.title}</h3>
                <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
                  {item.body}
                </p>
              </div>
            </Reveal>
          ))}
        </div>
      </Section>

      {/* 4. Feature highlights — alternating frameless showcase rows */}
      <Section className="py-16 sm:py-24">
        <SectionHead
          n="03"
          kicker={t.kickers.features}
          title={t.features.heading}
          subtitle={t.features.subheading}
        />
        <div className="mt-8">
          {t.features.items.map((item, i) => {
            const available = item.status === 'available'
            const mock = featureMocks[item.name]
            const flipped = i % 2 === 1
            return (
              <Reveal key={item.name} delay={0.04}>
                <div className="grid items-center gap-8 border-t border-border py-12 lg:grid-cols-2 lg:gap-14">
                  <div className={cn(flipped && 'lg:order-2')}>
                    <FeatureHeader
                      index={String(i + 1).padStart(2, '0')}
                      name={item.name}
                      badge={
                        available ? t.features.available : t.features.coming
                      }
                      available={available}
                    />
                    <p className="mt-3 max-w-md text-pretty text-muted-foreground">
                      {item.body}
                    </p>
                    {item.name === 'Launcher' ? (
                      <p className="mt-4 inline-flex items-center gap-2 text-sm text-muted-foreground">
                        {t.features.launcherPlaceholder}
                        <Kbd>⌘K</Kbd>
                      </p>
                    ) : null}
                  </div>
                  <div className={cn(flipped && 'lg:order-1')}>{mock?.()}</div>
                </div>
              </Reveal>
            )
          })}
        </div>

        {/* Two-tone closing line. */}
        <Reveal className="border-t border-border pt-10">
          <p className="max-w-3xl text-pretty text-xl font-semibold tracking-tight text-ink sm:text-2xl">
            {t.features.taglineStrong}{' '}
            <span className="font-normal text-muted-foreground">
              {t.features.tagline}
            </span>
          </p>
        </Reveal>
      </Section>

      {/* 5. Built in Rust — tabbed code + command list (2-col) */}
      <Section className="py-16 sm:py-24">
        <div className="grid gap-10 lg:grid-cols-[1fr_0.82fr] lg:items-start lg:gap-14">
          <div>
            <Reveal>
              <Eyebrow n="04">{t.kickers.craft}</Eyebrow>
              <h2 className="mt-4 max-w-xl text-balance text-3xl font-semibold tracking-tight text-ink sm:text-4xl">
                <span>{t.engine.headingStrong}</span>{' '}
                <span className="text-muted-foreground">
                  {t.engine.heading}
                </span>
              </h2>
            </Reveal>
            <Reveal delay={0.08} className="mt-7">
              <EngineTabs labels={t.engine.tabs} />
            </Reveal>
            <Reveal delay={0.12} className="mt-5">
              <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm text-muted-foreground">
                <span className="font-mono text-xs uppercase tracking-wide text-brand-emphasis">
                  {t.engine.worksWith}
                </span>
                {t.engine.targets.map((target) => (
                  <span key={target} className="text-foreground">
                    {target}
                  </span>
                ))}
              </div>
            </Reveal>
          </div>

          <Reveal delay={0.1} className="lg:pt-1">
            <Eyebrow n="" className="mb-4">
              <Command className="size-3.5" />
              {t.engine.paletteLabel}
            </Eyebrow>
            <ul className="divide-y divide-border border-t border-border">
              {t.engine.commands.map((commandItem, i) => (
                <li
                  key={commandItem.name}
                  className="flex items-center gap-3 py-3"
                >
                  <span className="w-4 font-mono text-xs text-muted-foreground">
                    {i + 1}
                  </span>
                  <span
                    aria-hidden
                    className="size-1.5 rounded-full bg-brand/70"
                  />
                  <span className="text-sm text-foreground">
                    {commandItem.name}
                  </span>
                  <Kbd className="ml-auto">{commandItem.keys}</Kbd>
                </li>
              ))}
            </ul>
          </Reveal>
        </div>
      </Section>

      {/* 6. Open source — centered, avatar marquees + big stats */}
      <div className="relative overflow-hidden border-y border-border">
        <div
          aria-hidden
          className="blueprint absolute inset-0 -z-10 opacity-60"
        />
        <Section className="py-20 text-center sm:py-28">
          <Reveal>
            <Eyebrow n="05" className="justify-center">
              {t.kickers.social}
            </Eyebrow>
            <h2 className="mx-auto mt-5 max-w-2xl text-3xl font-semibold tracking-tight text-ink sm:text-4xl">
              {t.opensource.heading}
            </h2>
            <p className="mx-auto mt-3 max-w-xl text-muted-foreground">
              {t.opensource.subheading}
            </p>
          </Reveal>

          <Reveal delay={0.08} className="mt-12">
            <AvatarMarquee contributors={stats?.contributors} />
          </Reveal>

          <Reveal delay={0.12}>
            <dl className="mt-12 grid grid-cols-2 gap-x-6 gap-y-10 sm:grid-cols-4">
              {t.opensource.stats.map((stat, i) => (
                <div key={stat.label}>
                  <dt className="sr-only">{stat.label}</dt>
                  <dd className="font-mono text-4xl font-semibold tracking-tight text-ink sm:text-5xl">
                    {statValues[i]}
                  </dd>
                  <p className="mt-2 text-sm text-muted-foreground">
                    {stat.label}
                  </p>
                </div>
              ))}
            </dl>
          </Reveal>

          {/* Maker's note — honest, human framing (frameless). */}
          <Reveal delay={0.16} className="mx-auto mt-16 max-w-3xl">
            <div className="border-t border-border pt-10 text-left">
              <h3 className="font-semibold text-ink">
                {t.social.makersNoteHeading}
              </h3>
              <p className="mt-3 text-pretty leading-relaxed text-muted-foreground">
                {t.social.makersNote}
              </p>
              <a
                href={GITHUB_REPO}
                target="_blank"
                rel="noreferrer noopener"
                className="mt-5 inline-flex items-center gap-1.5 text-sm font-medium text-brand-emphasis hover:underline"
              >
                {t.opensource.viewGithub}
                <ArrowRight className="size-4" />
              </a>
            </div>
          </Reveal>
        </Section>
      </div>

      {/* 7. Roadmap — timeline */}
      <Section className="py-16 sm:py-24">
        <SectionHead
          n="06"
          kicker={t.kickers.roadmap}
          title={t.roadmap.heading}
          subtitle={t.roadmap.subheading}
        />
        <ol className="mt-12">
          {t.roadmap.phases.map((phase, i) => {
            const current = i === 0
            const last = i === t.roadmap.phases.length - 1
            return (
              <Reveal
                as="li"
                key={phase.id}
                delay={i * 0.04}
                className="flex gap-5"
              >
                <div className="relative flex w-4 flex-col items-center">
                  <span
                    aria-hidden
                    className={cn(
                      'mt-1 size-4 shrink-0 rounded-full border-2',
                      current
                        ? 'border-brand bg-brand shadow-[0_0_0_4px_color-mix(in_oklab,var(--color-brand)_22%,transparent)]'
                        : 'border-border bg-background',
                    )}
                  />
                  {!last ? (
                    <span
                      aria-hidden
                      className="w-px flex-1 bg-gradient-to-b from-border to-border/30"
                    />
                  ) : null}
                </div>
                <div className={cn('pb-9', last && 'pb-0')}>
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-sm font-semibold text-brand-emphasis">
                      {phase.id}
                    </span>
                    {current ? (
                      <span className="font-mono text-[0.62rem] uppercase tracking-wide text-brand-emphasis">
                        · {t.features.available}
                      </span>
                    ) : null}
                  </div>
                  <h3 className="mt-1.5 font-semibold text-ink">
                    {phase.name}
                  </h3>
                  <p className="mt-1 max-w-xl text-sm text-muted-foreground">
                    {phase.body}
                  </p>
                </div>
              </Reveal>
            )
          })}
        </ol>
      </Section>

      {/* 8. Final CTA — frameless band */}
      <Section className="relative border-t border-border py-20 text-center sm:py-28">
        <div
          aria-hidden
          className="glow-brand absolute left-1/2 top-0 -z-10 h-64 w-[520px] max-w-full -translate-x-1/2"
        />
        <Reveal>
          <h2 className="text-3xl font-semibold tracking-tight text-ink sm:text-4xl">
            {t.finalCta.heading}
          </h2>
          <p className="mx-auto mt-3 max-w-xl text-muted-foreground">
            {t.finalCta.subheading}
          </p>
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <Link
              to="/$lang/download"
              params={{ lang: locale }}
              className={buttonClasses('brand', 'lg')}
            >
              {t.finalCta.download}
            </Link>
            <a
              href={GITHUB_REPO}
              target="_blank"
              rel="noreferrer noopener"
              className={buttonClasses('ghost', 'lg')}
            >
              <Github className="size-5" />
              {t.community.github}
            </a>
            <a
              href={GITHUB_DISCUSSIONS}
              target="_blank"
              rel="noreferrer noopener"
              className={buttonClasses('ghost', 'lg')}
            >
              <MessagesSquare className="size-5" />
              {t.community.discussions}
            </a>
          </div>
        </Reveal>
      </Section>
    </>
  )
}

function FeatureHeader({
  index,
  name,
  badge,
  available = false,
}: {
  index: string
  name: string
  badge: string
  available?: boolean
}) {
  const Icon = featureIcons[name] ?? Puzzle
  return (
    <div className="flex items-center gap-2.5">
      <span className="font-mono text-xs text-muted-foreground">{index}</span>
      <Icon className="size-5 text-brand-emphasis" />
      <h3 className="text-lg font-semibold text-ink">{name}</h3>
      <span
        className={cn(
          'font-mono text-[0.65rem] uppercase tracking-wide',
          available ? 'text-brand-emphasis' : 'text-muted-foreground',
        )}
      >
        · {badge}
      </span>
    </div>
  )
}
