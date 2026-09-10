import { useEffect, useRef, useState } from 'react'
import type { Comment, Reaction, Thread } from '~/lib/discussion'
import { formatDate, t, type Lang } from '~/lib/i18n'
import { SITE } from '~/lib/site'

/**
 * The comment thread, set in this site's own type rather than embedded.
 *
 * What is drawn comes from `/api/discussion`, which the Worker answers from
 * GitHub Discussions — see `app/lib/discussion.ts` for why the data stays
 * there and only the rendering moved here.
 */

/**
 * GitHub's eight reaction types. The emoji is left readable rather than hidden:
 * assistive technology announces the character's own name, which is a better
 * label than any we would write, and it stays right if GitHub adds a ninth.
 */
const REACTION_EMOJI: Record<string, string> = {
  THUMBS_UP: '👍',
  THUMBS_DOWN: '👎',
  LAUGH: '😄',
  HOORAY: '🎉',
  CONFUSED: '😕',
  HEART: '❤️',
  ROCKET: '🚀',
  EYES: '👀',
}

type Status = 'idle' | 'loading' | 'ready' | 'error'

function Reactions({ items, label }: { items: Reaction[]; label?: string }) {
  if (items.length === 0) return null

  return (
    <ul aria-label={label} className="flex list-none flex-wrap gap-2 p-0">
      {items.map((reaction) => (
        <li
          key={reaction.content}
          className="inline-flex items-center gap-1.5 rounded-full border border-line px-2.5 py-1 font-mono text-xs text-muted"
        >
          <span>{REACTION_EMOJI[reaction.content] ?? '•'}</span>
          <span className="tabular-nums">{reaction.count}</span>
        </li>
      ))}
    </ul>
  )
}

function Entry({ comment, lang, reply }: { comment: Comment; lang: Lang; reply: boolean }) {
  const strings = t(lang).blog

  return (
    <li className={reply ? 'pt-6' : 'border-t border-line-soft pt-7 first:border-t-0 first:pt-0'}>
      <p className="flex flex-wrap items-baseline gap-x-4 gap-y-1 font-mono text-xs text-muted">
        {comment.author ? (
          <a
            href={comment.authorUrl ?? undefined}
            className="text-ink no-underline hover:text-tan-ink"
          >
            {comment.author}
          </a>
        ) : (
          <span>{strings.commentsDeleted}</span>
        )}
        {comment.createdAt ? (
          <a href={comment.url || undefined} className="no-underline hover:text-tan-ink">
            <time dateTime={comment.createdAt}>{formatDate(comment.createdAt, lang)}</time>
          </a>
        ) : null}
      </p>

      {/* GitHub returns the body already rendered and already sanitised — the
          same HTML it serves on its own pages, and the same the giscus iframe
          used to show. Rendering the raw Markdown here instead would mean
          shipping a parser and owning that sanitiser ourselves. */}
      <div
        className="prose mt-4 text-[0.9375rem]"
        dangerouslySetInnerHTML={{ __html: comment.bodyHTML }}
      />

      {comment.reactions.length > 0 ? (
        <div className="mt-4">
          <Reactions items={comment.reactions} />
        </div>
      ) : null}

      {comment.replies.length > 0 ? (
        <ul className="mt-2 list-none border-l border-line pl-6">
          {comment.replies.map((child) => (
            <Entry key={child.id} comment={child} lang={lang} reply />
          ))}
        </ul>
      ) : null}
    </li>
  )
}

export function Comments({ lang, term }: { lang: Lang; term: string }) {
  const anchor = useRef<HTMLElement>(null)
  const [status, setStatus] = useState<Status>('idle')
  const [thread, setThread] = useState<Thread | null>(null)
  const strings = t(lang).blog

  // Nothing is fetched until the section is close to being read. An article is
  // usually left before its comments are, and this is the whole of what the
  // page costs a reader who never scrolls that far.
  useEffect(() => {
    const element = anchor.current
    if (!element || typeof IntersectionObserver === 'undefined') {
      setStatus('loading')
      return
    }

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          observer.disconnect()
          setStatus('loading')
        }
      },
      { rootMargin: '400px' },
    )
    observer.observe(element)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    if (status !== 'loading') return

    const aborter = new AbortController()
    fetch(`/api/discussion?term=${encodeURIComponent(term)}`, { signal: aborter.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(String(response.status))
        return (await response.json()) as { thread: Thread | null }
      })
      .then((payload) => {
        setThread(payload.thread)
        setStatus('ready')
      })
      .catch((error: unknown) => {
        // An aborted request is this component unmounting, not a failure.
        if (aborter.signal.aborted) return
        console.error('comments', error)
        setStatus('error')
      })

    return () => aborter.abort()
  }, [status, term])

  // Three destinations, because three different things are known. A thread that
  // was read has a URL. A thread that does not exist yet is opened with its
  // title already filled in, so the one-thread-per-article convention survives.
  // Anything else — not read yet, or not readable — is somewhere in Discussions,
  // and a search for the term is the most this side can honestly claim.
  //
  // The link is drawn in every state, including the prerendered one, so a
  // reader without JavaScript still leaves this section with somewhere to go.
  const outbound =
    status !== 'ready'
      ? {
          href: `${SITE.repoUrl}/discussions?discussions_q=${encodeURIComponent(term)}`,
          label: strings.commentsReply,
        }
      : thread
        ? { href: thread.url, label: strings.commentsReply }
        : {
            href: `${SITE.repoUrl}/discussions/new?category=blog&title=${encodeURIComponent(term)}`,
            label: strings.commentsStart,
          }

  return (
    <section ref={anchor} className="frame border-t border-line-soft py-14">
      <h2 className="eyebrow">
        <span>{strings.commentsTitle}</span>
      </h2>
      <p className="mb-7 max-w-[58ch] text-sm text-muted">{strings.commentsBody}</p>

      {/* Not drawn while idle: that is the state the page is prerendered in,
          and a reader without JavaScript would be left waiting on a fetch that
          is never going to happen. */}
      {status === 'loading' ? (
        <p className="font-mono text-xs text-muted">{strings.commentsLoading}</p>
      ) : null}

      {status === 'error' ? (
        <p className="max-w-[58ch] text-sm text-muted">{strings.commentsError}</p>
      ) : null}

      {status === 'ready' && thread && thread.reactions.length > 0 ? (
        <div className="mb-9">
          <Reactions items={thread.reactions} label={strings.commentsReactions} />
        </div>
      ) : null}

      {status === 'ready' ? (
        thread && thread.comments.length > 0 ? (
          <ul className="list-none p-0">
            {thread.comments.map((comment) => (
              <Entry key={comment.id} comment={comment} lang={lang} reply={false} />
            ))}
          </ul>
        ) : (
          <p className="max-w-[58ch] text-sm text-muted">{strings.commentsEmpty}</p>
        )
      ) : null}

      <a
        href={outbound.href}
        className="mt-9 inline-flex items-center gap-2 font-mono text-xs text-muted no-underline hover:text-tan-ink"
      >
        {outbound.label}
        <span aria-hidden="true">↗</span>
      </a>
    </section>
  )
}
