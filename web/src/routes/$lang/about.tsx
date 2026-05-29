import { createFileRoute } from '@tanstack/react-router'
import { useLocale } from '#/lib/locale-context'
import { Reveal } from '#/components/reveal'
import { SpotlightCard } from '#/components/ui/card'
import { defaultLocale, getMessages, isLocale } from '#/lib/i18n'
import { seoHead } from '#/lib/seo'

export const Route = createFileRoute('/$lang/about')({
  head: ({ params }) => {
    const locale = isLocale(params.lang) ? params.lang : defaultLocale
    const t = getMessages(locale)
    return seoHead({
      locale,
      path: '/about',
      title: `${t.about.title} — nohrs`,
      description: t.about.lead,
    })
  },
  component: About,
})

function About() {
  const { t } = useLocale()
  return (
    <div className="relative mx-auto max-w-3xl px-4 py-16 sm:px-6 sm:py-24">
      <div
        aria-hidden
        className="bg-grid absolute inset-x-0 top-0 -z-10 h-72"
      />
      <Reveal>
        <h1 className="text-4xl font-semibold tracking-[-0.02em] text-ink sm:text-5xl">
          {t.about.title}
        </h1>
        <p className="mt-4 text-lg text-muted-foreground">{t.about.lead}</p>
      </Reveal>

      <Reveal className="mt-16">
        <h2 className="text-2xl font-semibold tracking-tight text-ink">
          {t.about.storyHeading}
        </h2>
        <div className="mt-4 flex flex-col gap-4">
          {t.about.story.map((paragraph, i) => (
            <p
              key={i}
              className="text-pretty leading-relaxed text-muted-foreground"
            >
              {paragraph}
            </p>
          ))}
        </div>
      </Reveal>

      <Reveal className="mt-16">
        <h2 className="text-2xl font-semibold tracking-tight text-ink">
          {t.about.valuesHeading}
        </h2>
        <div className="mt-6 grid gap-4 sm:grid-cols-2">
          {t.about.values.map((value) => (
            <SpotlightCard key={value.title} className="p-6">
              <h3 className="font-semibold text-ink">{value.title}</h3>
              <p className="mt-2 text-sm text-muted-foreground">{value.body}</p>
            </SpotlightCard>
          ))}
        </div>
      </Reveal>

      <Reveal className="mt-16">
        <div className="border-gradient rounded-2xl bg-card p-8">
          <h2 className="text-xl font-semibold tracking-tight text-ink">
            {t.about.makersNoteHeading}
          </h2>
          <p className="mt-4 text-pretty leading-relaxed text-muted-foreground">
            {t.social.makersNote}
          </p>
        </div>
      </Reveal>
    </div>
  )
}
