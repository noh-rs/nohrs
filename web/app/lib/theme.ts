import { useCallback, useEffect, useState } from 'react'

export type Theme = 'light' | 'dark'

const STORAGE_KEY = 'nohrs-theme'

function systemTheme(): Theme {
  return typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches
    ? 'dark'
    : 'light'
}

/**
 * The theme has three states, but the toggle shows two: an explicit choice
 * stamped on <html>, and — until someone makes one — whatever the OS says.
 * The value returned is what the page is *showing*, which is what the toggle
 * has to reflect.
 *
 * SSR starts at 'light' rather than reading anything: the server has no OS
 * preference to read, and the pre-paint script in `__root.tsx` has already put
 * the right attribute on <html> by the time this hydrates.
 */
export function useTheme(): [Theme, (theme: Theme) => void] {
  const [theme, setTheme] = useState<Theme>('light')

  useEffect(() => {
    const explicit = document.documentElement.getAttribute('data-theme')
    setTheme(explicit === 'dark' || explicit === 'light' ? explicit : systemTheme())

    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => {
      if (!document.documentElement.hasAttribute('data-theme')) setTheme(systemTheme())
    }
    media.addEventListener('change', onChange)
    return () => media.removeEventListener('change', onChange)
  }, [])

  const choose = useCallback((next: Theme) => {
    document.documentElement.setAttribute('data-theme', next)
    setTheme(next)
    try {
      localStorage.setItem(STORAGE_KEY, next)
    } catch {
      // Private mode or blocked site data: the choice holds for this page only.
    }
  }, [])

  return [theme, choose]
}
