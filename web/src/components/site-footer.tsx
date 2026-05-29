import { Link } from '@tanstack/react-router'
import { useLocale } from '#/lib/locale-context'
import {
  GITHUB_DISCUSSIONS,
  GITHUB_LICENSE,
  GITHUB_RELEASES,
  GITHUB_REPO,
} from '#/lib/links'

type Item =
  | { label: string; kind: 'internal'; to: '/$lang/about' | '/$lang/download' }
  | { label: string; kind: 'external'; href: string }
  | { label: string; kind: 'soon' }

export function SiteFooter() {
  const { locale, t } = useLocale()

  const columns: Array<{ title: string; items: Array<Item> }> = [
    {
      title: t.footer.product,
      items: [
        { label: t.nav.download, kind: 'internal', to: '/$lang/download' },
        { label: t.nav.releases, kind: 'external', href: GITHUB_RELEASES },
        { label: t.nav.plugins, kind: 'soon' },
        { label: t.footer.roadmap, kind: 'soon' },
        { label: t.nav.docs, kind: 'soon' },
        { label: 'GitHub', kind: 'external', href: GITHUB_REPO },
      ],
    },
    {
      title: t.footer.resources,
      items: [
        { label: t.footer.faq, kind: 'soon' },
        { label: t.footer.community, kind: 'soon' },
        {
          label: t.footer.discussions,
          kind: 'external',
          href: GITHUB_DISCUSSIONS,
        },
        { label: t.footer.privacy, kind: 'soon' },
      ],
    },
    {
      title: t.footer.project,
      items: [
        { label: t.nav.blog, kind: 'soon' },
        { label: t.footer.about, kind: 'internal', to: '/$lang/about' },
        { label: t.footer.license, kind: 'external', href: GITHUB_LICENSE },
      ],
    },
    {
      title: t.footer.social,
      items: [
        { label: 'GitHub', kind: 'external', href: GITHUB_REPO },
        {
          label: t.footer.discussions,
          kind: 'external',
          href: GITHUB_DISCUSSIONS,
        },
      ],
    },
  ]

  const linkCls =
    'text-sm text-muted-foreground transition-colors hover:text-foreground rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background'

  return (
    <footer className="border-t border-border">
      <div className="mx-auto max-w-6xl px-4 py-14 sm:px-6">
        <div className="grid grid-cols-2 gap-8 md:grid-cols-5">
          <div className="col-span-2 md:col-span-1">
            <span className="font-mono text-lg font-semibold">
              noh<span className="text-brand-emphasis">rs</span>
            </span>
            <p className="mt-3 max-w-[24ch] text-sm text-muted-foreground">
              {t.footer.builtWith}
            </p>
          </div>
          {columns.map((col) => (
            <div key={col.title}>
              <h2 className="text-sm font-semibold text-foreground">
                {col.title}
              </h2>
              <ul className="mt-4 flex flex-col gap-3">
                {col.items.map((item) => (
                  <li key={item.label}>
                    {item.kind === 'internal' ? (
                      <Link
                        to={item.to}
                        params={{ lang: locale }}
                        className={linkCls}
                      >
                        {item.label}
                      </Link>
                    ) : item.kind === 'external' ? (
                      <a
                        href={item.href}
                        target="_blank"
                        rel="noreferrer noopener"
                        className={linkCls}
                      >
                        {item.label}
                      </a>
                    ) : (
                      <span className="flex items-center gap-2 text-sm text-muted-foreground/70">
                        {item.label}
                        <span className="font-mono text-[0.65rem] uppercase tracking-wide text-muted-foreground/60">
                          {t.footer.comingSoon}
                        </span>
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
        <div className="mt-12 border-t border-border pt-6 text-sm text-muted-foreground">
          {t.footer.rights}
        </div>
      </div>
    </footer>
  )
}
