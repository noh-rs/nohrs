import { Link, createFileRoute } from '@tanstack/react-router'
import {
  ArrowRight,
  Command,
  FolderTree,
  Github,
  MessagesSquare,
  Puzzle,
  Search,
  Star,
} from 'lucide-react'
import { useLocale } from '#/lib/locale-context'
import { Reveal } from '#/components/reveal'
import { buttonClasses } from '#/components/ui/button'
import { SpotlightCard } from '#/components/ui/card'
import { CodeBlock, Comment, Prompt } from '#/components/ui/code-block'
import { Kbd } from '#/components/ui/kbd'
import { defaultLocale, getMessages, isLocale } from '#/lib/i18n'
import { seoHead } from '#/lib/seo'
import { GITHUB_DISCUSSIONS, GITHUB_REPO, SITE_URL } from '#/lib/links'
import { cn } from '#/lib/utils'

export const Route = createFileRoute('/$lang/')({
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
      <p className="eyebrow">
        <span className="text-muted-foreground">{n}</span>
        <span aria-hidden className="h-px w-6 bg-brand/60" />
        {kicker}
      </p>
      <h2 className="mt-4 text-3xl font-semibold tracking-tight text-ink sm:text-4xl">
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

/** Abstract CSS sketch of the Explorer (sidebar / list / preview). */
function ExplorerSketch() {
  return (
    <div
      aria-hidden
      className="mt-6 grid grid-cols-[64px_1fr_72px] gap-2 rounded-lg border border-border bg-background/70 p-2.5"
    >
      <div className="space-y-1.5">
        {[12, 9, 11, 8].map((w, i) => (
          <div
            key={i}
            className="h-2 rounded-full bg-foreground/10"
            style={{ width: `${w * 4}px` }}
          />
        ))}
      </div>
      <div className="space-y-1.5">
        {[0, 1, 2, 3, 4].map((i) => (
          <div
            key={i}
            className={cn(
              'h-2.5 rounded',
              i === 1 ? 'bg-brand/30' : 'bg-foreground/8',
            )}
          />
        ))}
      </div>
      <div className="rounded bg-foreground/5" />
    </div>
  )
}

function Landing() {
  const { locale, t } = useLocale()
  const explorer = t.features.items.find((i) => i.name === 'Explorer')
  const rest = t.features.items.filter((i) => i.name !== 'Explorer')

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
          <span className="inline-flex items-center rounded-full border border-border bg-card/70 px-3 py-1 font-mono text-xs uppercase tracking-wide text-muted-foreground backdrop-blur">
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
              className={buttonClasses('outline', 'lg')}
            >
              <Github className="size-5" />
              {t.hero.viewGithub}
            </a>
          </div>
        </Reveal>

        {/* Frameless product shot: the real Explorer, placed simply. */}
        <Reveal delay={0.1} className="mt-16">
          <figure className="mx-auto max-w-4xl">
            <img
              src="/screenshot-explorer.jpeg"
              alt={t.hero.screenshotAlt}
              width={1280}
              height={800}
              loading="eager"
              className="mx-auto w-full rounded-xl ring-1 ring-border shadow-2xl shadow-brand/10"
            />
            <figcaption className="mt-4 text-sm text-muted-foreground">
              {t.hero.screenshotCaption}
            </figcaption>
          </figure>
        </Reveal>
      </Section>

      {/* 2. Why nohrs */}
      <Section id="features" className="scroll-mt-20 py-16 sm:py-24">
        <SectionHead
          n="01"
          kicker={t.kickers.why}
          title={t.why.heading}
          subtitle={t.why.subheading}
        />
        <div className="mt-12 grid gap-4 sm:grid-cols-2">
          {t.why.points.map((point, i) => (
            <Reveal key={point.title} delay={i * 0.05}>
              <SpotlightCard className="h-full p-8">
                <span className="font-mono text-sm text-brand-emphasis">
                  {String(i + 1).padStart(2, '0')}
                </span>
                <h3 className="mt-3 text-lg font-semibold text-ink">
                  {point.title}
                </h3>
                <p className="mt-2 text-muted-foreground">{point.body}</p>
              </SpotlightCard>
            </Reveal>
          ))}
        </div>
      </Section>

      {/* 3. Feature highlights — bento */}
      <Section className="py-16 sm:py-24">
        <SectionHead
          n="02"
          kicker={t.kickers.features}
          title={t.features.heading}
          subtitle={t.features.subheading}
        />
        <div className="mt-12 grid gap-4 lg:grid-cols-3">
          {/* Explorer — large tile with a mini UI sketch */}
          {explorer ? (
            <Reveal className="lg:col-span-2 lg:row-span-2">
              <SpotlightCard className="flex h-full flex-col p-8">
                <div className="flex items-center gap-2">
                  <FolderTree className="size-6 text-brand-emphasis" />
                  <h3 className="font-semibold text-ink">{explorer.name}</h3>
                  <span className="rounded-full bg-brand px-2 py-0.5 font-mono text-[0.65rem] uppercase tracking-wide text-brand-foreground">
                    {t.features.available}
                  </span>
                </div>
                <p className="mt-2 max-w-md text-sm text-muted-foreground">
                  {explorer.body}
                </p>
                <ExplorerSketch />
              </SpotlightCard>
            </Reveal>
          ) : null}

          {rest.map((item, i) => {
            const Icon = featureIcons[item.name] ?? Puzzle
            return (
              <Reveal key={item.name} delay={i * 0.05}>
                <SpotlightCard className="flex h-full flex-col p-6">
                  <div className="flex items-center gap-2">
                    <Icon className="size-5 text-brand-emphasis" />
                    <h3 className="font-semibold text-ink">{item.name}</h3>
                    <span className="rounded-full border border-border px-2 py-0.5 font-mono text-[0.65rem] uppercase tracking-wide text-muted-foreground">
                      {t.features.coming}
                    </span>
                  </div>
                  <p className="mt-2 text-sm text-muted-foreground">
                    {item.body}
                  </p>
                  {item.name === 'Launcher' ? (
                    <div className="mt-4 rounded-lg border border-border bg-background/70 p-2.5">
                      <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Search className="size-4" />
                        <span className="truncate">
                          {t.features.launcherPlaceholder}
                        </span>
                        <Kbd className="ml-auto">⌘K</Kbd>
                      </div>
                    </div>
                  ) : null}
                </SpotlightCard>
              </Reveal>
            )
          })}
        </div>
      </Section>

      {/* 4. Built in Rust */}
      <Section className="py-16 sm:py-24">
        <SectionHead n="03" kicker={t.kickers.craft} title={t.craft.heading} />
        <div className="mt-10 grid gap-8 lg:grid-cols-2 lg:items-center">
          <Reveal>
            <p className="text-muted-foreground">{t.craft.body}</p>
            <ul className="mt-6 flex flex-col gap-3">
              {t.craft.points.map((point) => (
                <li key={point} className="flex items-start gap-3">
                  <span
                    aria-hidden
                    className="mt-2 size-1.5 shrink-0 rounded-full bg-brand"
                  />
                  <span className="text-sm">{point}</span>
                </li>
              ))}
            </ul>
          </Reveal>
          <Reveal delay={0.1}>
            <CodeBlock label="~/nohrs">
              <Prompt />
              git clone {'https://github.com/noh-rs/nohrs'}
              {'\n'}
              <Prompt />
              cd nohrs{'\n'}
              <Prompt />
              cargo run --features gui{'\n'}
              <Comment> Compiling nohrs v0.1.0</Comment>
              {'\n'}
              <Comment> Finished — launching nohrs</Comment>
            </CodeBlock>
          </Reveal>
        </div>
      </Section>

      {/* 5. OSS transparency */}
      <Section className="py-16 sm:py-24">
        <SectionHead
          n="04"
          kicker={t.kickers.social}
          title={t.social.heading}
          subtitle={t.social.subheading}
        />
        <div className="mt-12 grid gap-4 lg:grid-cols-3">
          <Reveal>
            <SpotlightCard className="flex h-full flex-col p-6">
              <Star className="size-6 text-brand-emphasis" />
              <p className="mt-4 font-mono text-sm text-muted-foreground">
                {t.social.stars}
              </p>
              <a
                href={GITHUB_REPO}
                target="_blank"
                rel="noreferrer noopener"
                className="mt-2 inline-flex items-center gap-1.5 text-sm font-medium text-brand-emphasis hover:underline"
              >
                {t.social.viewGithub}
                <ArrowRight className="size-4" />
              </a>
            </SpotlightCard>
          </Reveal>
          <Reveal delay={0.05} className="lg:col-span-2">
            <div className="border-gradient h-full rounded-xl bg-card p-6">
              <h3 className="font-semibold text-ink">
                {t.social.makersNoteHeading}
              </h3>
              <p className="mt-3 text-pretty text-muted-foreground">
                {t.social.makersNote}
              </p>
            </div>
          </Reveal>
        </div>
      </Section>

      {/* 6. Roadmap — timeline */}
      <Section className="py-16 sm:py-24">
        <SectionHead
          n="05"
          kicker={t.kickers.roadmap}
          title={t.roadmap.heading}
          subtitle={t.roadmap.subheading}
        />
        <ol className="mt-12 ml-1.5 border-l border-border">
          {t.roadmap.phases.map((phase, i) => {
            const current = i === 0
            return (
              <Reveal
                as="li"
                key={phase.id}
                delay={i * 0.04}
                className="relative pb-8 pl-8 last:pb-0"
              >
                <span
                  aria-hidden
                  className={cn(
                    'absolute -left-[7px] top-1 size-3.5 rounded-full border-2 border-background',
                    current ? 'bg-brand' : 'bg-muted-foreground/40',
                  )}
                />
                <span className="font-mono text-sm font-semibold text-brand-emphasis">
                  {phase.id}
                </span>
                <h3 className="mt-1 font-semibold text-ink">{phase.name}</h3>
                <p className="mt-1 text-sm text-muted-foreground">
                  {phase.body}
                </p>
              </Reveal>
            )
          })}
        </ol>
      </Section>

      {/* 7. Community + 8. Final CTA */}
      <Section className="pb-24 pt-4">
        <Reveal>
          <div className="relative overflow-hidden rounded-2xl border border-border bg-card p-8 text-center sm:p-14">
            <div
              aria-hidden
              className="glow-brand absolute left-1/2 top-0 h-64 w-[520px] max-w-full -translate-x-1/2"
            />
            <h2 className="relative text-3xl font-semibold tracking-tight text-ink sm:text-4xl">
              {t.finalCta.heading}
            </h2>
            <p className="relative mx-auto mt-3 max-w-xl text-muted-foreground">
              {t.finalCta.subheading}
            </p>
            <div className="relative mt-8 flex flex-wrap items-center justify-center gap-3">
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
                className={buttonClasses('outline', 'lg')}
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
          </div>
        </Reveal>
      </Section>
    </>
  )
}
