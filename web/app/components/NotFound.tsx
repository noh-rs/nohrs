import { Link, useRouterState } from '@tanstack/react-router'
import { CANONICAL_LANG, isLang, t } from '~/lib/i18n'

export function NotFound() {
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const segment = pathname.split('/')[1]
  const lang = isLang(segment) ? segment : CANONICAL_LANG
  const strings = t(lang).notFound

  return (
    <div className="frame py-32">
      <p className="eyebrow">
        <span>404</span>
      </p>
      <h1 className="text-3xl leading-tight md:text-4xl">{strings.title}</h1>
      <p className="mt-5 max-w-[52ch] text-ink-2">{strings.body}</p>
      <Link to="/$lang" params={{ lang }} className="btn btn-primary mt-9">
        {strings.cta}
      </Link>
    </div>
  )
}
