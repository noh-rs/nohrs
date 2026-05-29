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

// Typed so an unmatched name yields `undefined` (keeps the `?? Puzzle`
// fallback honest and avoids rendering an undefined component).
const featureIcons: Record<string, typeof Puzzle | undefined> = {
  Explorer: FolderTree,
  Launcher: Command,
  Plugins: Puzzle,
  Search: Search,
}

function Landing() {
  const { locale, t } = useLocale()

  return (
    <>
      {/* 1. Hero */}
      <Section className="pt-16 pb-12 sm:pt-24 sm:pb-16 text-center">
        <Reveal>
          <span className="inline-flex items-center rounded-full border border-border bg-muted px-3 py-1 font-mono text-xs uppercase tracking-wide text-muted-foreground">
            {t.hero.badge}
          </span>
          <h1 className="mx-auto mt-6 max-w-3xl text-balance text-4xl font-semibold tracking-tight sm:text-6xl">
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

        {/* Framed product shot. Honest: the real Explorer, not a faked composite. */}
        <Reveal delay={0.1} className="mt-14">
          <figure className="mx-auto max-w-4xl">
            <div className="relative">
              <div
                aria-hidden
                className="absolute -inset-6 -z-10 rounded-[2rem] bg-brand/20 blur-3xl"
              />
              <div className="overflow-hidden rounded-xl border border-border bg-card shadow-2xl">
                <div className="flex items-center gap-1.5 border-b border-border bg-muted px-4 py-3">
                  <span className="size-3 rounded-full bg-foreground/15" />
                  <span className="size-3 rounded-full bg-foreground/15" />
                  <span className="size-3 rounded-full bg-foreground/15" />
                </div>
                <img
                  src="/screenshot-explorer.jpeg"
                  alt={t.hero.screenshotAlt}
                  width={1280}
                  height={800}
                  loading="eager"
                  className="block w-full"
                />
              </div>
            </div>
            <figcaption className="mt-4 text-sm text-muted-foreground">
              {t.hero.screenshotCaption}
            </figcaption>
          </figure>
        </Reveal>
      </Section>

      {/* 2. Why nohrs */}
      <Section id="features" className="scroll-mt-20 py-16 sm:py-24">
        <Reveal>
          <h2 className="text-3xl font-semibold tracking-tight sm:text-4xl">
            {t.why.heading}
          </h2>
          <p className="mt-3 max-w-2xl text-muted-foreground">
            {t.why.subheading}
          </p>
        </Reveal>
        <div className="mt-12 grid gap-px overflow-hidden rounded-xl border border-border bg-border sm:grid-cols-2">
          {t.why.points.map((point, i) => (
            <Reveal key={point.title} delay={i * 0.05} className="bg-card p-8">
              <h3 className="text-lg font-semibold">{point.title}</h3>
              <p className="mt-2 text-muted-foreground">{point.body}</p>
            </Reveal>
          ))}
        </div>
      </Section>

      {/* 3. Feature highlights (honest available/coming) */}
      <Section className="py-16 sm:py-24">
        <Reveal>
          <h2 className="text-3xl font-semibold tracking-tight sm:text-4xl">
            {t.features.heading}
          </h2>
          <p className="mt-3 max-w-2xl text-muted-foreground">
            {t.features.subheading}
          </p>
        </Reveal>
        <div className="mt-12 grid gap-6 sm:grid-cols-2 lg:grid-cols-4">
          {t.features.items.map((item, i) => {
            const Icon = featureIcons[item.name] ?? Puzzle
            const available = item.status === 'available'
            return (
              <Reveal
                key={item.name}
                delay={i * 0.05}
                className="flex flex-col rounded-xl border border-border bg-card p-6"
              >
                <Icon className="size-6 text-brand-emphasis" />
                <div className="mt-4 flex items-center gap-2">
                  <h3 className="font-semibold">{item.name}</h3>
                  <span
                    className={cn(
                      'rounded-full px-2 py-0.5 font-mono text-[0.65rem] uppercase tracking-wide',
                      available
                        ? 'bg-brand text-brand-foreground'
                        : 'border border-border text-muted-foreground',
                    )}
                  >
                    {available ? t.features.available : t.features.coming}
                  </span>
                </div>
                <p className="mt-2 text-sm text-muted-foreground">
                  {item.body}
                </p>
              </Reveal>
            )
          })}
        </div>
      </Section>

      {/* 4. Built in Rust */}
      <Section className="py-16 sm:py-24">
        <div className="overflow-hidden rounded-2xl border border-border bg-card">
          <div className="grid gap-8 p-8 sm:p-12 lg:grid-cols-2 lg:items-center">
            <Reveal>
              <h2 className="text-3xl font-semibold tracking-tight sm:text-4xl">
                {t.craft.heading}
              </h2>
              <p className="mt-4 text-muted-foreground">{t.craft.body}</p>
            </Reveal>
            <Reveal delay={0.1}>
              <ul className="flex flex-col gap-3">
                {t.craft.points.map((point) => (
                  <li
                    key={point}
                    className="flex items-start gap-3 rounded-lg border border-border bg-background p-4"
                  >
                    <span
                      aria-hidden
                      className="mt-2 size-2 shrink-0 rounded-full bg-brand"
                    />
                    <span className="text-sm">{point}</span>
                  </li>
                ))}
              </ul>
            </Reveal>
          </div>
        </div>
      </Section>

      {/* 5. OSS transparency (social-proof replacement) */}
      <Section className="py-16 sm:py-24">
        <Reveal>
          <h2 className="text-3xl font-semibold tracking-tight sm:text-4xl">
            {t.social.heading}
          </h2>
          <p className="mt-3 max-w-2xl text-muted-foreground">
            {t.social.subheading}
          </p>
        </Reveal>
        <div className="mt-12 grid gap-6 lg:grid-cols-3">
          <Reveal className="rounded-xl border border-border bg-card p-6">
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
          </Reveal>
          <Reveal
            delay={0.05}
            className="lg:col-span-2 rounded-xl border border-border bg-card p-6"
          >
            <h3 className="font-semibold">{t.social.makersNoteHeading}</h3>
            <p className="mt-3 text-pretty text-muted-foreground">
              {t.social.makersNote}
            </p>
          </Reveal>
        </div>
      </Section>

      {/* 6. Roadmap teaser */}
      <Section className="py-16 sm:py-24">
        <Reveal>
          <h2 className="text-3xl font-semibold tracking-tight sm:text-4xl">
            {t.roadmap.heading}
          </h2>
          <p className="mt-3 max-w-2xl text-muted-foreground">
            {t.roadmap.subheading}
          </p>
        </Reveal>
        <ol className="mt-12 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {t.roadmap.phases.map((phase, i) => (
            <Reveal
              key={phase.id}
              delay={i * 0.04}
              as="li"
              className="rounded-xl border border-border bg-card p-6"
            >
              <span className="font-mono text-sm font-semibold text-brand-emphasis">
                {phase.id}
              </span>
              <h3 className="mt-1 font-semibold">{phase.name}</h3>
              <p className="mt-2 text-sm text-muted-foreground">{phase.body}</p>
            </Reveal>
          ))}
        </ol>
      </Section>

      {/* 7. Community */}
      <Section className="py-16 sm:py-24">
        <Reveal className="rounded-2xl border border-border bg-card p-8 text-center sm:p-12">
          <h2 className="text-3xl font-semibold tracking-tight sm:text-4xl">
            {t.community.heading}
          </h2>
          <p className="mx-auto mt-3 max-w-xl text-muted-foreground">
            {t.community.subheading}
          </p>
          <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
            <a
              href={GITHUB_REPO}
              target="_blank"
              rel="noreferrer noopener"
              className={buttonClasses('brand', 'md')}
            >
              <Github className="size-5" />
              {t.community.github}
            </a>
            <a
              href={GITHUB_DISCUSSIONS}
              target="_blank"
              rel="noreferrer noopener"
              className={buttonClasses('outline', 'md')}
            >
              <MessagesSquare className="size-5" />
              {t.community.discussions}
            </a>
          </div>
        </Reveal>
      </Section>

      {/* 8. Final CTA */}
      <Section className="pb-24 pt-4 text-center">
        <Reveal>
          <h2 className="text-3xl font-semibold tracking-tight sm:text-4xl">
            {t.finalCta.heading}
          </h2>
          <p className="mx-auto mt-3 max-w-xl text-muted-foreground">
            {t.finalCta.subheading}
          </p>
          <div className="mt-8">
            <Link
              to="/$lang/download"
              params={{ lang: locale }}
              className={buttonClasses('brand', 'lg')}
            >
              {t.finalCta.download}
            </Link>
          </div>
        </Reveal>
      </Section>
    </>
  )
}
