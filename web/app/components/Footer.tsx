import { Link } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { Mark } from './Mark'
import { t, type Lang } from '~/lib/i18n'
import { SITE } from '~/lib/site'

function Column({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div>
      <h2 className="mb-3.5 font-mono text-[0.6875rem] leading-none font-medium tracking-[0.12em] text-muted uppercase">
        {title}
      </h2>
      {children}
    </div>
  )
}

const linkClass =
  'block py-[5px] text-sm text-ink-2 no-underline transition-colors duration-200 hover:text-tan-ink'

export function Footer({ lang }: { lang: Lang }) {
  const strings = t(lang)

  return (
    <footer className="border-t border-line bg-sunken">
      <div className="frame">
        <div className="grid grid-cols-2 gap-8 pt-14 pb-10 md:grid-cols-3 lg:grid-cols-[1.4fr_repeat(4,1fr)]">
          <div className="col-span-2 md:col-span-3 lg:col-span-1">
            <Link
              to="/$lang"
              params={{ lang }}
              className="mb-3 flex items-center gap-[9px] font-semibold tracking-[-0.015em] no-underline"
            >
              <Mark size={20} />
              <span>{SITE.name}</span>
            </Link>
            <p className="max-w-[28ch] text-[0.8125rem] text-muted">{strings.footer.tagline}</p>
          </div>

          <Column title={strings.footer.product}>
            <Link to="/$lang/download" params={{ lang }} className={linkClass}>
              {strings.nav.download}
            </Link>
            <Link to="/$lang/releases" params={{ lang }} className={linkClass}>
              {strings.nav.releases}
            </Link>
            <Link to="/$lang/plugins" params={{ lang }} className={linkClass}>
              {strings.nav.plugins}
            </Link>
            <Link to="/$lang/roadmap" params={{ lang }} className={linkClass}>
              {strings.nav.roadmap}
            </Link>
          </Column>

          <Column title={strings.footer.resources}>
            <Link to="/$lang/docs" params={{ lang }} className={linkClass}>
              {strings.nav.docs}
            </Link>
            <Link to="/$lang/blog" params={{ lang }} className={linkClass}>
              {strings.nav.blog}
            </Link>
            <a href={SITE.discussionsUrl} className={linkClass}>
              {strings.footer.discussions}
            </a>
            <a href={SITE.discordUrl} className={linkClass}>
              {strings.footer.community}
            </a>
          </Column>

          <Column title={strings.footer.project}>
            <Link to="/$lang/about" params={{ lang }} className={linkClass}>
              {strings.nav.about}
            </Link>
            <a href={SITE.contributingUrl} className={linkClass}>
              {strings.footer.contributing}
            </a>
            <a href={SITE.licenseUrl} className={linkClass}>
              {strings.footer.license}
            </a>
          </Column>

          <Column title={strings.footer.social}>
            <a href={SITE.repoUrl} className={linkClass}>
              GitHub
            </a>
            <a href={SITE.discordUrl} className={linkClass}>
              Discord
            </a>
            <a href={SITE.xUrl} className={linkClass}>
              X
            </a>
          </Column>
        </div>

        <div className="flex flex-wrap justify-between gap-x-6 gap-y-2.5 border-t border-line pt-5 pb-10 font-mono text-xs text-muted">
          <span>{strings.footer.rights}</span>
          <span>{strings.footer.builtWith}</span>
        </div>
      </div>
    </footer>
  )
}
