import { Link, useRouterState } from '@tanstack/react-router'
import { Check, Languages } from 'lucide-react'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '#/components/ui/dropdown-menu'
import { useLocale } from '#/lib/locale-context'
import { localizePath, locales } from '#/lib/i18n'
import type { Locale } from '#/lib/i18n'
import { cn } from '#/lib/utils'

const labels: Record<Locale, string> = { en: 'English', ja: '日本語' }

export function LangSwitcher() {
  const { locale, t } = useLocale()
  const pathname = useRouterState({
    select: (s) => s.location.pathname,
  })

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={t.nav.language}
        className={cn(
          'inline-flex h-10 items-center gap-1.5 rounded-md px-3 text-sm text-foreground transition-colors hover:bg-muted',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background',
        )}
      >
        <Languages className="size-4" />
        <span className="font-mono uppercase">{locale}</span>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {locales.map((code) => (
          <DropdownMenuItem key={code} asChild>
            <Link
              to={localizePath(pathname, code)}
              className="justify-between"
              aria-current={code === locale ? 'true' : undefined}
            >
              {labels[code]}
              {code === locale ? <Check className="size-4" /> : null}
            </Link>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
