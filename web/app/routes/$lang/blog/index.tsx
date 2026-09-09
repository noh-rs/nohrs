import { createFileRoute, Link } from '@tanstack/react-router'
import { PageHeader, Section } from '~/components/Page'
import { blogPosts } from '~/lib/content'
import { formatDate, t, type Lang } from '~/lib/i18n'
import { seo } from '~/lib/seo'

export const Route = createFileRoute('/$lang/blog/')({
  head: ({ params }) => {
    const lang = params.lang as Lang
    const strings = t(lang).blog
    return seo({ lang, path: '/blog', title: strings.title, description: strings.lede })
  },
  component: BlogIndex,
})

function BlogIndex() {
  const { lang } = Route.useParams() as { lang: Lang }
  const strings = t(lang).blog
  const posts = blogPosts(lang)

  return (
    <>
      <PageHeader eyebrow={t(lang).nav.blog} title={strings.title} lede={strings.lede} />

      <Section>
        {posts.length === 0 ? (
          <p className="text-ink-2">{strings.empty}</p>
        ) : (
          <div className="border-t border-line">
            {posts.map((post) => (
              <Link
                key={post.slug}
                to="/$lang/blog/$slug"
                params={{ lang, slug: post.slug }}
                className="group grid gap-x-8 gap-y-2 border-b border-line-soft py-8 no-underline transition-[padding] duration-200 last:border-line hover:bg-surface hover:pl-3.5 md:grid-cols-[132px_minmax(0,1fr)]"
              >
                <time
                  dateTime={post.date}
                  className="font-mono text-[0.8125rem] text-muted tabular-nums"
                >
                  {formatDate(post.date, lang)}
                </time>
                <div>
                  <h2 className="text-[1.125rem] transition-colors duration-200 group-hover:text-tan-ink">
                    {post.title}
                  </h2>
                  <p className="mt-1.5 max-w-[62ch] text-[0.9375rem] text-muted">{post.description}</p>
                  {/* Words with space between them, not badges and not
                      interpunct-separated attributes (ADR 0008, 改訂). */}
                  {post.tags?.length ? (
                    <p className="mt-3 flex flex-wrap gap-x-5 font-mono text-xs text-muted">
                      {post.tags.map((tag) => (
                        <span key={tag}>{tag}</span>
                      ))}
                    </p>
                  ) : null}
                </div>
              </Link>
            ))}
          </div>
        )}
      </Section>
    </>
  )
}
