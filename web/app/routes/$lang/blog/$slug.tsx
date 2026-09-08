import { createFileRoute, Link, notFound } from '@tanstack/react-router'
import { MDXProvider } from '@mdx-js/react'
import { Giscus } from '~/components/Giscus'
import { mdxComponents } from '~/components/MdxComponents'
import { blogPost } from '~/lib/content'
import { formatDate, t, type Lang } from '~/lib/i18n'
import { jsonLd, seo } from '~/lib/seo'
import { SITE } from '~/lib/site'

export const Route = createFileRoute('/$lang/blog/$slug')({
  beforeLoad: ({ params }) => {
    if (!blogPost(params.lang as Lang, params.slug)) throw notFound()
  },
  head: ({ params }) => {
    const lang = params.lang as Lang
    const post = blogPost(lang, params.slug)
    if (!post) return {}

    const { meta, links } = seo({
      lang,
      path: `/blog/${post.slug}`,
      title: post.title,
      description: post.description,
      // `post.lang` rather than the route's: an untranslated article is shown
      // in the canonical language, and its card was rendered for that text.
      image: post.og_image ?? `/og/blog-${post.lang}-${post.slug}.png`,
      type: 'article',
      publishedAt: post.date,
    })

    return {
      meta,
      links,
      scripts: [
        jsonLd({
          '@context': 'https://schema.org',
          '@type': 'BlogPosting',
          headline: post.title,
          description: post.description,
          datePublished: post.date,
          inLanguage: lang,
          author: { '@type': 'Person', name: post.author },
          mainEntityOfPage: `${SITE.host}/${lang}/blog/${post.slug}`,
        }),
      ],
    }
  },
  component: BlogPost,
})

function BlogPost() {
  const { lang, slug } = Route.useParams() as { lang: Lang; slug: string }
  const strings = t(lang).blog
  const post = blogPost(lang, slug)
  if (!post) return null

  const { Body } = post

  return (
    <>
      <article data-pagefind-body className="frame pt-[clamp(48px,6vw,80px)] pb-16">
        <Link
          to="/$lang/blog"
          params={{ lang }}
          className="inline-flex items-center gap-2 font-mono text-xs text-muted no-underline hover:text-tan-ink"
        >
          <span aria-hidden="true">←</span>
          {strings.backToIndex}
        </Link>

        <h1 className="mt-7 max-w-[24ch] text-[clamp(1.9rem,4vw,2.75rem)] leading-[1.1] tracking-[-0.035em]">
          {post.title}
        </h1>

        <p className="mt-5 flex flex-wrap gap-x-6 font-mono text-xs text-muted">
          <time dateTime={post.date}>{formatDate(post.date, lang)}</time>
          <span>{post.author}</span>
          {post.tags?.map((tag) => <span key={tag}>{tag}</span>)}
        </p>

        {post.fallbackFrom ? (
          <div className="mt-9 border-l-2 border-tan pl-5">
            <p className="text-[0.9375rem] text-ink">{strings.translationMissingTitle}</p>
            <p className="mt-1 text-sm text-muted">{strings.translationMissingBody}</p>
          </div>
        ) : null}

        <div className="prose mt-11" lang={post.fallbackFrom ?? lang}>
          <MDXProvider components={mdxComponents}>
            <Body />
          </MDXProvider>
        </div>
      </article>

      <Giscus lang={lang} term={`blog/${slug}`} />
    </>
  )
}
