import { createRootRoute, HeadContent, Scripts, useRouterState } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { CANONICAL_LANG, isLang } from '~/lib/i18n'
import { CF_ANALYTICS_TOKEN, SITE } from '~/lib/site'
import appCss from '~/styles/app.css?url'

/**
 * Runs before first paint so an explicitly chosen theme is on <html> by the
 * time the first pixel lands. Without it a dark-theme visitor gets a white
 * flash on every navigation that reloads the document.
 */
const THEME_BOOTSTRAP = `
try {
  var stored = localStorage.getItem('nohrs-theme');
  if (stored === 'light' || stored === 'dark') {
    document.documentElement.setAttribute('data-theme', stored);
  }
} catch (error) {
  /* private mode, or site data blocked — the OS preference still applies */
}
`.trim()

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      { name: 'theme-color', content: '#fcfaf8', media: '(prefers-color-scheme: light)' },
      { name: 'theme-color', content: '#100e0c', media: '(prefers-color-scheme: dark)' },
      { name: 'twitter:card', content: 'summary_large_image' },
      { name: 'twitter:site', content: SITE.xHandle },
      { property: 'og:site_name', content: SITE.name },
    ],
    links: [
      { rel: 'stylesheet', href: appCss },
      // Written by scripts/fetch-fonts.mjs. A missing file costs the web faces,
      // not the page — the stacks in app.css fall back to system faces.
      { rel: 'stylesheet', href: '/fonts/fonts.css' },
      { rel: 'icon', href: '/favicon.svg', type: 'image/svg+xml' },
      { rel: 'apple-touch-icon', href: '/apple-touch-icon.png' },
      { rel: 'alternate', type: 'application/rss+xml', title: 'Nohrs blog (EN)', href: '/en/blog/rss.xml' },
      { rel: 'alternate', type: 'application/rss+xml', title: 'Nohrs blog (JA)', href: '/ja/blog/rss.xml' },
    ],
    scripts: [{ children: THEME_BOOTSTRAP }],
  }),
  shellComponent: RootDocument,
})

function RootDocument({ children }: { children: ReactNode }) {
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const segment = pathname.split('/')[1]
  const lang = isLang(segment) ? segment : CANONICAL_LANG

  return (
    <html lang={lang}>
      <head>
        <HeadContent />
      </head>
      <body>
        {children}
        {CF_ANALYTICS_TOKEN ? (
          <script
            defer
            src="https://static.cloudflareinsights.com/beacon.min.js"
            data-cf-beacon={`{"token":"${CF_ANALYTICS_TOKEN}"}`}
          />
        ) : null}
        <Scripts />
      </body>
    </html>
  )
}
