import * as React from 'react'
import { Moon, Sun } from 'lucide-react'
import { cn } from '#/lib/utils'

const STORAGE_KEY = 'nohrs-theme'

/* Inline script injected in <head> before paint: applies the persisted (or
   system) theme synchronously so there's no flash of the wrong theme. */
export const themeInitScript = `(function(){try{var k="${STORAGE_KEY}";var s=localStorage.getItem(k);var d=s?s==="dark":matchMedia("(prefers-color-scheme: dark)").matches;document.documentElement.classList.toggle("dark",d);}catch(e){}})();`

export function ThemeToggle({ label }: { label: string }) {
  const [mounted, setMounted] = React.useState(false)
  const [isDark, setIsDark] = React.useState(false)

  React.useEffect(() => {
    setMounted(true)
    setIsDark(document.documentElement.classList.contains('dark'))
  }, [])

  function toggle() {
    const next = !document.documentElement.classList.contains('dark')
    document.documentElement.classList.toggle('dark', next)
    try {
      localStorage.setItem(STORAGE_KEY, next ? 'dark' : 'light')
    } catch {
      /* storage unavailable (private mode); theme still applies for the session */
    }
    setIsDark(next)
  }

  return (
    <button
      type="button"
      onClick={toggle}
      aria-label={label}
      aria-pressed={mounted ? isDark : undefined}
      className={cn(
        'inline-flex size-10 items-center justify-center rounded-md text-foreground transition-colors hover:bg-muted',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background',
      )}
    >
      {/* Render both; CSS shows the one matching the current theme to avoid hydration mismatch. */}
      <Sun className="size-5 dark:hidden" />
      <Moon className="hidden size-5 dark:block" />
    </button>
  )
}
