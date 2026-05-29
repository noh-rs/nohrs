import {
  HeadContent,
  Scripts,
  createRootRoute,
  useRouterState,
} from '@tanstack/react-router'
import { TanStackRouterDevtoolsPanel } from '@tanstack/react-router-devtools'
import { TanStackDevtools } from '@tanstack/react-devtools'

import appCss from '../styles.css?url'
import { themeInitScript } from '#/components/theme-toggle'
import { htmlLang, isLocale } from '#/lib/i18n'

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      { title: 'nohrs — keyboard-first launcher & file explorer' },
      {
        name: 'description',
        content:
          'nohrs is a keyboard-first launcher and file explorer for macOS, built in Rust and extensible with sandboxed WASM plugins.',
      },
      { name: 'theme-color', content: '#dea584' },
      { property: 'og:site_name', content: 'nohrs' },
      { property: 'og:type', content: 'website' },
      { name: 'twitter:card', content: 'summary_large_image' },
    ],
    links: [{ rel: 'stylesheet', href: appCss }],
  }),
  shellComponent: RootDocument,
})

function RootDocument({ children }: { children: React.ReactNode }) {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const segment = pathname.split('/')[1] ?? ''
  const lang = isLocale(segment) ? htmlLang[segment] : 'en'

  return (
    <html lang={lang}>
      <head>
        {/* Apply persisted/system theme before paint to avoid a flash. */}
        <script dangerouslySetInnerHTML={{ __html: themeInitScript }} />
        <HeadContent />
      </head>
      <body>
        {children}
        {/* The @tanstack/devtools-vite plugin strips this block from
            production builds, so no manual env guard is needed (and a guard
            would break that plugin's AST removal). */}
        <TanStackDevtools
          config={{ position: 'bottom-right' }}
          plugins={[
            {
              name: 'Tanstack Router',
              render: <TanStackRouterDevtoolsPanel />,
            },
          ]}
        />
        <Scripts />
      </body>
    </html>
  )
}
