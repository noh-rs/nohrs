import { createFileRoute } from '@tanstack/react-router'
import { useEffect } from 'react'
import { LANGS, negotiateLang } from '~/lib/i18n'
import { SITE } from '~/lib/site'

/**
 * `/` carries no content of its own — it only decides a language.
 *
 * In production the redirect Worker answers this before the asset is reached
 * (docs/web.md §4), so the page below is the fallback for anything that gets
 * past it: a direct hit on the prerendered file, or a crawler. It redirects on
 * the client and, failing that, offers both languages as links.
 */
export const Route = createFileRoute('/')({
  head: () => ({
    meta: [
      { title: `${SITE.name} — Launcher × Explorer` },
      { name: 'robots', content: 'noindex' },
    ],
    links: [
      { rel: 'canonical', href: `${SITE.host}/en` },
      { rel: 'alternate', hrefLang: 'en', href: `${SITE.host}/en` },
      { rel: 'alternate', hrefLang: 'ja', href: `${SITE.host}/ja` },
      { rel: 'alternate', hrefLang: 'x-default', href: `${SITE.host}/en` },
    ],
  }),
  component: LanguagePicker,
})

function LanguagePicker() {
  useEffect(() => {
    const lang = negotiateLang(navigator.languages?.join(',') ?? navigator.language)
    window.location.replace(`/${lang}`)
  }, [])

  return (
    <main className="frame flex min-h-[70vh] flex-col justify-center gap-6">
      <h1 className="text-2xl">{SITE.name}</h1>
      <nav className="flex gap-3" aria-label="Language">
        {LANGS.map((lang) => (
          <a key={lang} href={`/${lang}`} className="btn btn-ghost font-mono">
            {lang === 'en' ? 'English' : '日本語'}
          </a>
        ))}
      </nav>
    </main>
  )
}
