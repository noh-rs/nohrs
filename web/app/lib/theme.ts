import { useCallback, useEffect, useState } from 'react'

export type Theme = 'light' | 'dark'

const STORAGE_KEY = 'nohrs-theme'

function systemTheme(): Theme {
  return typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches
    ? 'dark'
    : 'light'
}

/** What the page is currently showing, as last resolved on the client. */
let current: Theme = 'light'
const listeners = new Set<(theme: Theme) => void>()

/**
 * One value shared by every `useTheme` caller.
 *
 * Per-hook state was a real defect rather than a tidiness point: the header's
 * toggle and the comments section each held their own copy, so switching the
 * theme left giscus rendering its previous one until the page reloaded.
 *
 * Deliberately not `useSyncExternalStore`: the server cannot know the
 * viewer's OS preference, so a snapshot that read it on the client would
 * disagree with the prerendered HTML and trip a hydration warning on every
 * dark-mode visit. The pre-paint script in `__root.tsx` has already put the
 * right colours on screen — this only has to agree with them by first effect,
 * which is well before anyone can act on the control.
 */
function publish(theme: Theme) {
  current = theme
  for (const listener of listeners) listener(theme)
}

export function useTheme(): [Theme, (theme: Theme) => void] {
  const [theme, setTheme] = useState<Theme>(current)

  useEffect(() => {
    listeners.add(setTheme)

    const explicit = document.documentElement.getAttribute('data-theme')
    publish(explicit === 'dark' || explicit === 'light' ? explicit : systemTheme())

    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => {
      // An explicit choice wins; without one the page follows the OS.
      if (!document.documentElement.hasAttribute('data-theme')) publish(systemTheme())
    }
    media.addEventListener('change', onChange)

    return () => {
      listeners.delete(setTheme)
      media.removeEventListener('change', onChange)
    }
  }, [])

  const choose = useCallback((next: Theme) => {
    document.documentElement.setAttribute('data-theme', next)
    publish(next)
    try {
      localStorage.setItem(STORAGE_KEY, next)
    } catch {
      // Private mode or blocked site data: the choice holds for this page only.
    }
  }, [])

  return [theme, choose]
}
