import { useEffect, useRef } from 'react'
import { GISCUS } from '~/lib/site'
import { useTheme } from '~/lib/theme'
import { t, type Lang } from '~/lib/i18n'

/**
 * Comments are GitHub Discussions through giscus. It is injected rather than
 * imported so nothing ships when the environment has not been configured — a
 * fork building this site should not post into our Discussions.
 */
export function Giscus({ lang, term }: { lang: Lang; term: string }) {
  const host = useRef<HTMLDivElement>(null)
  const [theme] = useTheme()
  const strings = t(lang).blog

  useEffect(() => {
    const element = host.current
    if (!element || !GISCUS.repoId || !GISCUS.categoryId) return

    element.replaceChildren()
    const script = document.createElement('script')
    script.src = 'https://giscus.app/client.js'
    script.async = true
    script.crossOrigin = 'anonymous'
    Object.assign(script.dataset, {
      repo: GISCUS.repo,
      repoId: GISCUS.repoId,
      category: GISCUS.category,
      categoryId: GISCUS.categoryId,
      mapping: 'specific',
      term,
      reactionsEnabled: '1',
      emitMetadata: '0',
      inputPosition: 'top',
      theme: theme === 'dark' ? 'transparent_dark' : 'light',
      lang: lang === 'ja' ? 'ja' : 'en',
      loading: 'lazy',
    })
    element.appendChild(script)
  }, [lang, term, theme])

  if (!GISCUS.repoId || !GISCUS.categoryId) return null

  return (
    <section className="frame border-t border-line-soft py-14">
      <h2 className="eyebrow">
        <span>{strings.commentsTitle}</span>
      </h2>
      <p className="mb-7 max-w-[58ch] text-sm text-muted">{strings.commentsBody}</p>
      <div ref={host} />
    </section>
  )
}
