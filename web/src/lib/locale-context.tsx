import * as React from 'react'
import { getMessages } from '#/lib/i18n'
import type { Locale, Messages } from '#/lib/i18n'

interface LocaleContextValue {
  locale: Locale
  t: Messages
}

const LocaleContext = React.createContext<LocaleContextValue | null>(null)

export function LocaleProvider({
  locale,
  children,
}: {
  locale: Locale
  children: React.ReactNode
}) {
  const value = React.useMemo<LocaleContextValue>(
    () => ({ locale, t: getMessages(locale) }),
    [locale],
  )
  return (
    <LocaleContext.Provider value={value}>{children}</LocaleContext.Provider>
  )
}

export function useLocale(): LocaleContextValue {
  const ctx = React.useContext(LocaleContext)
  if (!ctx) {
    throw new Error('useLocale must be used within a LocaleProvider')
  }
  return ctx
}
