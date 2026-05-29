import * as React from 'react'
import { Link } from '@tanstack/react-router'
import { Github, Menu, X } from 'lucide-react'
import { useLocale } from '#/lib/locale-context'
import { buttonClasses } from '#/components/ui/button'
import { ThemeToggle } from '#/components/theme-toggle'
import { LangSwitcher } from '#/components/lang-switcher'
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
  DialogTrigger,
} from '#/components/ui/dialog'
import { GITHUB_REPO } from '#/lib/links'
import { cn } from '#/lib/utils'

function Wordmark({ lang }: { lang: string }) {
  return (
    <Link
      to="/$lang"
      params={{ lang }}
      className="inline-flex items-center font-mono text-lg font-semibold tracking-tight text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background rounded-sm"
    >
      noh<span className="text-brand-emphasis">rs</span>
    </Link>
  )
}

export function SiteHeader() {
  const { locale, t } = useLocale()
  const [open, setOpen] = React.useState(false)

  const desktopLink =
    'relative px-3 py-2 text-sm text-muted-foreground transition-colors hover:text-foreground rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background after:absolute after:inset-x-3 after:-bottom-px after:h-px after:origin-left after:scale-x-0 after:bg-brand after:transition-transform after:duration-200 hover:after:scale-x-100 data-[status=active]:text-foreground data-[status=active]:after:scale-x-100'

  return (
    <header className="sticky top-0 z-40 border-b border-border bg-background/80 backdrop-blur-md">
      <div className="mx-auto flex h-16 max-w-6xl items-center justify-between gap-4 px-4 sm:px-6">
        <div className="flex items-center gap-1">
          <Wordmark lang={locale} />
        </div>

        <nav aria-label="Primary" className="hidden items-center md:flex">
          <Link
            to="/$lang"
            params={{ lang: locale }}
            hash="features"
            className={desktopLink}
          >
            {t.nav.features}
          </Link>
          <Link
            to="/$lang/about"
            params={{ lang: locale }}
            className={desktopLink}
          >
            {t.footer.about}
          </Link>
          <Link
            to="/$lang/download"
            params={{ lang: locale }}
            className={desktopLink}
          >
            {t.nav.download}
          </Link>
        </nav>

        <div className="flex items-center gap-1">
          <a
            href={GITHUB_REPO}
            target="_blank"
            rel="noreferrer noopener"
            aria-label="GitHub"
            className="hidden size-10 items-center justify-center rounded-md text-foreground transition-colors hover:bg-muted sm:inline-flex focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
          >
            <Github className="size-5" />
          </a>
          <div className="hidden sm:block">
            <LangSwitcher />
          </div>
          <ThemeToggle label={t.nav.toggleTheme} />
          <Link
            to="/$lang/download"
            params={{ lang: locale }}
            className={cn(
              buttonClasses('brand', 'sm'),
              'hidden sm:inline-flex',
            )}
          >
            {t.nav.download}
          </Link>

          <Dialog open={open} onOpenChange={setOpen}>
            <DialogTrigger
              aria-label={t.nav.toggleMenu}
              className="inline-flex size-10 items-center justify-center rounded-md text-foreground transition-colors hover:bg-muted md:hidden focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
            >
              <Menu className="size-5" />
            </DialogTrigger>
            <DialogContent className="md:hidden">
              <div className="mx-auto flex max-w-6xl flex-col gap-1">
                <div className="flex items-center justify-between">
                  <DialogTitle asChild>
                    <span className="font-mono text-lg font-semibold">
                      noh<span className="text-brand-emphasis">rs</span>
                    </span>
                  </DialogTitle>
                  <DialogClose
                    aria-label={t.nav.toggleMenu}
                    className="inline-flex size-10 items-center justify-center rounded-md hover:bg-muted"
                  >
                    <X className="size-5" />
                  </DialogClose>
                </div>
                <nav aria-label="Mobile" className="mt-4 flex flex-col gap-1">
                  <Link
                    to="/$lang"
                    params={{ lang: locale }}
                    hash="features"
                    onClick={() => setOpen(false)}
                    className="rounded-md px-3 py-3 text-base hover:bg-muted"
                  >
                    {t.nav.features}
                  </Link>
                  <Link
                    to="/$lang/about"
                    params={{ lang: locale }}
                    onClick={() => setOpen(false)}
                    className="rounded-md px-3 py-3 text-base hover:bg-muted"
                  >
                    {t.footer.about}
                  </Link>
                  <Link
                    to="/$lang/download"
                    params={{ lang: locale }}
                    onClick={() => setOpen(false)}
                    className="rounded-md px-3 py-3 text-base hover:bg-muted"
                  >
                    {t.nav.download}
                  </Link>
                  {[t.nav.docs, t.nav.blog, t.nav.plugins, t.nav.releases].map(
                    (item) => (
                      <span
                        key={item}
                        className="flex items-center justify-between rounded-md px-3 py-3 text-base text-muted-foreground"
                      >
                        {item}
                        <span className="font-mono text-xs uppercase">
                          {t.footer.comingSoon}
                        </span>
                      </span>
                    ),
                  )}
                </nav>
                <div className="mt-4 flex items-center gap-2 px-3">
                  <LangSwitcher />
                  <a
                    href={GITHUB_REPO}
                    target="_blank"
                    rel="noreferrer noopener"
                    className={buttonClasses('outline', 'sm')}
                  >
                    <Github className="size-4" />
                    GitHub
                  </a>
                </div>
              </div>
            </DialogContent>
          </Dialog>
        </div>
      </div>
    </header>
  )
}
