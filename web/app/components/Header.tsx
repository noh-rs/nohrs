import { Link, useNavigate, useRouterState } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { Mark, GitHubMark } from './Mark'
import { Segmented } from './Segmented'
import { DocsSearch } from './DocsSearch'
import { useTheme } from '~/lib/theme'
import { swapLang, t, type Lang } from '~/lib/i18n'
import { LANG_COOKIE } from '~/lib/negotiate'
import { github, hasDownloads } from '~/lib/github'
import { SITE } from '~/lib/site'

export function Header({ lang }: { lang: Lang }) {
  const strings = t(lang)
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const [theme, setTheme] = useTheme()
  const [stuck, setStuck] = useState(false)

  useEffect(() => {
    const onScroll = () => setStuck(window.scrollY > 8)
    onScroll()
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => window.removeEventListener('scroll', onScroll)
  }, [])

  const links = [
    { to: '/$lang/docs', label: strings.nav.docs },
    { to: '/$lang/blog', label: strings.nav.blog },
    { to: '/$lang/plugins', label: strings.nav.plugins },
    { to: '/$lang/releases', label: strings.nav.releases },
  ] as const

  return (
    <header
      className={`sticky top-0 z-40 border-b transition-[border-color] duration-200 ${
        stuck ? 'border-line-soft' : 'border-transparent'
      }`}
      style={{
        background: 'color-mix(in srgb, var(--paper) 86%, transparent)',
        backdropFilter: 'saturate(1.4) blur(12px)',
      }}
    >
      <div className="frame flex h-[60px] items-center gap-5">
        <Link
          to="/$lang"
          params={{ lang }}
          className="flex items-center gap-[9px] font-semibold tracking-[-0.015em] no-underline"
        >
          <Mark />
          <span>{SITE.name}</span>
        </Link>

        <nav className="ml-3 hidden flex-1 gap-1 lg:flex" aria-label={strings.nav.menu}>
          {links.map((link) => (
            <Link
              key={link.to}
              to={link.to}
              params={{ lang }}
              // Japanese labels are longer than the English ones and wrap out
              // of the 60px bar without this.
              className="group relative rounded px-2.5 py-1.5 text-sm whitespace-nowrap text-muted no-underline transition-colors duration-200 hover:text-ink [&.active]:text-ink"
              activeProps={{ className: 'active' }}
            >
              {link.label}
              <span className="pointer-events-none absolute inset-x-2.5 bottom-0.5 h-px origin-left scale-x-0 bg-tan-ink transition-transform duration-200 group-hover:scale-x-100 group-[.active]:scale-x-100" />
            </Link>
          ))}
        </nav>

        <div className="ml-auto flex items-center gap-2.5 lg:ml-0">
          <DocsSearch lang={lang} />

          <Segmented
            label={strings.nav.language}
            value={lang}
            onChange={(next) => {
              // The redirect Worker reads this when someone types the bare
              // domain, so a chosen language survives leaving the site.
              document.cookie = `${LANG_COOKIE}=${next}; Path=/; Max-Age=31536000; SameSite=Lax`
              void navigate({ to: swapLang(pathname, next) })
            }}
            options={[
              { value: 'en' as Lang, label: 'EN' },
              { value: 'ja' as Lang, label: 'JA' },
            ]}
          />

          <Segmented
            label={strings.nav.theme}
            value={theme}
            onChange={setTheme}
            options={[
              { value: 'light' as const, label: '☀', title: strings.nav.light },
              { value: 'dark' as const, label: '☾', title: strings.nav.dark },
            ]}
          />

          {hasDownloads ? (
            <Link to="/$lang/download" params={{ lang }} className="btn btn-primary hidden sm:inline-flex">
              {strings.nav.download}
            </Link>
          ) : (
            <a href={SITE.repoUrl} className="btn btn-primary hidden sm:inline-flex">
              <GitHubMark />
              <span>{strings.nav.star}</span>
              <span className="count">{github.repo.stars}</span>
            </a>
          )}
        </div>
      </div>

      {/* The link row moves below the bar rather than into a drawer: four items
          fit on one scrollable line, and a drawer would be a second navigation
          model to maintain for no gain. */}
      <nav
        className="frame flex gap-1 overflow-x-auto border-t border-line-soft py-1.5 lg:hidden"
        aria-label={strings.nav.menu}
      >
        {links.map((link) => (
          <Link
            key={link.to}
            to={link.to}
            params={{ lang }}
            className="shrink-0 rounded px-2.5 py-1 text-sm whitespace-nowrap text-muted no-underline [&.active]:text-ink"
            activeProps={{ className: 'active' }}
          >
            {link.label}
          </Link>
        ))}
      </nav>
    </header>
  )
}
