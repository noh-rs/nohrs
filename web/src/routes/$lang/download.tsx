import { createFileRoute } from '@tanstack/react-router'
import { AlertTriangle, Apple, Terminal } from 'lucide-react'
import { useLocale } from '#/lib/locale-context'
import { Reveal } from '#/components/reveal'
import { buttonClasses } from '#/components/ui/button'
import { CodeBlock, Prompt } from '#/components/ui/code-block'
import { defaultLocale, getMessages, isLocale } from '#/lib/i18n'
import { seoHead } from '#/lib/seo'
import { GITHUB_RELEASES, GITHUB_REPO } from '#/lib/links'

export const Route = createFileRoute('/$lang/download')({
  head: ({ params }) => {
    const locale = isLocale(params.lang) ? params.lang : defaultLocale
    const t = getMessages(locale)
    return seoHead({
      locale,
      path: '/download',
      title: `${t.download.title} — nohrs`,
      description: t.download.lead,
    })
  },
  component: Download,
})

function Download() {
  const { t } = useLocale()
  return (
    <div className="relative mx-auto max-w-3xl px-4 py-16 sm:px-6 sm:py-24">
      <div
        aria-hidden
        className="bg-grid absolute inset-x-0 top-0 -z-10 h-72"
      />
      <Reveal>
        <h1 className="text-4xl font-semibold tracking-[-0.02em] text-ink sm:text-5xl">
          {t.download.title}
        </h1>
        <p className="mt-4 text-lg text-muted-foreground">{t.download.lead}</p>
      </Reveal>

      {/* Pre-alpha honesty banner */}
      <Reveal className="mt-10">
        <div className="flex gap-4 rounded-xl border border-brand/40 bg-brand/10 p-5">
          <AlertTriangle className="mt-0.5 size-5 shrink-0 text-brand-emphasis" />
          <div>
            <h2 className="font-semibold text-ink">
              {t.download.prealphaHeading}
            </h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t.download.prealpha}
            </p>
          </div>
        </div>
      </Reveal>

      <Reveal className="mt-12">
        <div className="border-gradient rounded-xl bg-card p-6">
          <div className="flex items-center gap-3">
            <Apple className="size-6" />
            <h2 className="text-xl font-semibold text-ink">
              {t.download.macHeading}
            </h2>
          </div>
          <p className="mt-3 text-sm text-muted-foreground">
            {t.download.macBody}
          </p>
          <a
            href={GITHUB_RELEASES}
            target="_blank"
            rel="noreferrer noopener"
            className={buttonClasses('outline', 'md', 'mt-5')}
          >
            {t.download.macButton}
          </a>
        </div>
      </Reveal>

      <Reveal className="mt-8">
        <div className="border-gradient rounded-xl bg-card p-6">
          <div className="flex items-center gap-3">
            <Terminal className="size-6" />
            <h2 className="text-xl font-semibold text-ink">
              {t.download.sourceHeading}
            </h2>
          </div>
          <p className="mt-3 text-sm text-muted-foreground">
            {t.download.sourceIntro}
          </p>
          <CodeBlock label="~/" className="mt-4">
            <Prompt />
            git clone {'https://github.com/noh-rs/nohrs'}
            {'\n'}
            <Prompt />
            cd nohrs{'\n'}
            <Prompt />
            cargo run --features gui --bin nohrs
          </CodeBlock>
          <a
            href={GITHUB_REPO}
            target="_blank"
            rel="noreferrer noopener"
            className={buttonClasses('outline', 'md', 'mt-5')}
          >
            {t.download.sourceButton}
          </a>
        </div>
      </Reveal>

      <Reveal className="mt-12">
        <h2 className="text-xl font-semibold tracking-tight text-ink">
          {t.download.reqHeading}
        </h2>
        <ul className="mt-4 flex flex-col gap-2">
          {t.download.requirements.map((req) => (
            <li
              key={req}
              className="flex items-start gap-3 text-sm text-muted-foreground"
            >
              <span
                aria-hidden
                className="mt-2 size-1.5 shrink-0 rounded-full bg-brand"
              />
              {req}
            </li>
          ))}
        </ul>
      </Reveal>
    </div>
  )
}
