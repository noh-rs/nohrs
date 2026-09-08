import { createFileRoute, Link, notFound } from '@tanstack/react-router'
import { MDXProvider } from '@mdx-js/react'
import { mdxComponents } from '~/components/MdxComponents'
import { docPage, docPages } from '~/lib/content'
import { t, type Lang } from '~/lib/i18n'
import { seo } from '~/lib/seo'
import { SITE } from '~/lib/site'

export const Route = createFileRoute('/$lang/docs/$slug')({
  beforeLoad: ({ params }) => {
    if (!docPage(params.lang as Lang, params.slug)) throw notFound()
  },
  head: ({ params }) => {
    const lang = params.lang as Lang
    const page = docPage(lang, params.slug)
    if (!page) return {}
    return seo({
      lang,
      path: `/docs/${page.slug}`,
      title: page.title,
      description: page.description,
      contentLang: page.lang,
    })
  },
  component: DocDetail,
})

function DocDetail() {
  const { lang, slug } = Route.useParams() as { lang: Lang; slug: string }
  const strings = t(lang).docs
  const page = docPage(lang, slug)
  if (!page) return null

  const ordered = docPages(lang)
  const index = ordered.findIndex((entry) => entry.slug === slug)
  const previous = index > 0 ? ordered[index - 1] : undefined
  const next = index >= 0 && index < ordered.length - 1 ? ordered[index + 1] : undefined
  const { Body } = page

  // `data-pagefind-body` narrows the index to documentation and articles, so a
  // search does not match the nav and footer that every page repeats.
  return (
    <article data-pagefind-body>
      <p className="eyebrow">
        <span>{strings.categories[page.category] ?? page.category}</span>
      </p>
      <h1 className="text-[clamp(1.75rem,3.4vw,2.4rem)] leading-tight tracking-[-0.035em]">
        {page.title}
      </h1>
      <p className="mt-4 max-w-[58ch] text-[1.0625rem] text-ink-2">{page.description}</p>

      {page.fallbackFrom ? (
        <div className="mt-8 border-l-2 border-tan pl-5">
          <p className="text-[0.9375rem] text-ink">{strings.translationMissingTitle}</p>
          <p className="mt-1 text-sm text-muted">{strings.translationMissingBody}</p>
        </div>
      ) : null}

      <div className="prose mt-10" lang={page.fallbackFrom ?? lang}>
        <MDXProvider components={mdxComponents}>
          <Body />
        </MDXProvider>
      </div>

      <nav className="mt-16 grid gap-px border-t border-line pt-6 sm:grid-cols-2" aria-label={strings.title}>
        <div>
          {previous ? (
            <Link
              to="/$lang/docs/$slug"
              params={{ lang, slug: previous.slug }}
              className="block no-underline"
            >
              <span className="font-mono text-xs text-muted">← {strings.previous}</span>
              <span className="mt-1 block text-[0.9375rem] text-ink">{previous.title}</span>
            </Link>
          ) : null}
        </div>
        <div className="sm:text-right">
          {next ? (
            <Link to="/$lang/docs/$slug" params={{ lang, slug: next.slug }} className="block no-underline">
              <span className="font-mono text-xs text-muted">{strings.next} →</span>
              <span className="mt-1 block text-[0.9375rem] text-ink">{next.title}</span>
            </Link>
          ) : null}
        </div>
      </nav>

      <p className="mt-10 font-mono text-xs text-muted">
        <a href={`${SITE.repoUrl}/blob/develop/web/content/${page.lang}/docs/${slug}.mdx`}>
          {strings.editOnGitHub} ↗
        </a>
      </p>
    </article>
  )
}
