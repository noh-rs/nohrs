/**
 * The comment thread for an article, read from GitHub Discussions.
 *
 * The thread is stored on GitHub and rendered here, rather than embedded.
 * giscus put GitHub's entire stylesheet inside an iframe on the page, and no
 * theme option reaches into an iframe — so the comments could never be made to
 * look like the rest of the site. Reading the same data through the API and
 * setting it ourselves is the only way that changes.
 *
 * Leaving the data on GitHub is deliberate rather than a half-measure: the
 * moderation tools, the abuse reporting, the spam handling and the notification
 * mail stay with GitHub, and no visitor's name or address is ever stored here.
 *
 * The title convention is giscus's `specific` mapping — one discussion per
 * article, titled `blog/<slug>` — so a thread that was written through giscus
 * is the same thread this reads.
 */

export type Reaction = { content: string; count: number }

export type Comment = {
  id: string
  url: string
  author: string | null
  authorUrl: string | null
  createdAt: string
  bodyHTML: string
  reactions: Reaction[]
  replies: Comment[]
}

export type Thread = {
  url: string
  /** Top-level comments on the discussion, as GitHub counts them. */
  total: number
  reactions: Reaction[]
  comments: Comment[]
}

/**
 * Which terms name a thread. This is a validator before it is a convenience:
 * the term is interpolated into a GitHub search string below, and the pattern
 * is what makes a quote or an operator impossible to smuggle into it.
 */
export const THREAD_TERM = /^blog\/[a-z0-9][a-z0-9-]{0,80}$/

const COMMENT_FIELDS = `
  id
  url
  createdAt
  bodyHTML
  author { login url }
  reactionGroups { content reactors { totalCount } }
`

const THREAD_QUERY = `
  query Thread($search: String!) {
    search(type: DISCUSSION, query: $search, first: 10) {
      nodes {
        ... on Discussion {
          title
          url
          reactionGroups { content reactors { totalCount } }
          comments(first: 50) {
            totalCount
            nodes {
              ${COMMENT_FIELDS}
              replies(first: 20) { nodes { ${COMMENT_FIELDS} } }
            }
          }
        }
      }
    }
  }
`

type RawReactionGroup = { content?: string; reactors?: { totalCount?: number } }

type RawComment = {
  id?: string
  url?: string
  createdAt?: string
  bodyHTML?: string
  author?: { login?: string; url?: string } | null
  reactionGroups?: RawReactionGroup[]
  replies?: { nodes?: (RawComment | null)[] }
}

type RawDiscussion = {
  title?: string
  url?: string
  reactionGroups?: RawReactionGroup[]
  comments?: { totalCount?: number; nodes?: (RawComment | null)[] }
}

type RawPayload = {
  data?: { search?: { nodes?: (RawDiscussion | null)[] } }
  errors?: { message?: string }[]
}

function reactions(groups: RawReactionGroup[] | undefined): Reaction[] {
  return (groups ?? [])
    .map((group) => ({ content: group.content ?? '', count: group.reactors?.totalCount ?? 0 }))
    .filter((reaction) => reaction.content !== '' && reaction.count > 0)
}

function comment(raw: RawComment, depth: number): Comment | null {
  // An id and a body are what makes a comment renderable; anything without
  // them is a shape we did not ask for, and is dropped rather than drawn.
  if (!raw.id || typeof raw.bodyHTML !== 'string') return null

  return {
    id: raw.id,
    url: raw.url ?? '',
    // `author` is null for a deleted account, which GitHub still shows the
    // comments of.
    author: raw.author?.login ?? null,
    authorUrl: raw.author?.url ?? null,
    createdAt: raw.createdAt ?? '',
    bodyHTML: raw.bodyHTML,
    reactions: reactions(raw.reactionGroups),
    replies:
      depth > 0
        ? (raw.replies?.nodes ?? [])
            .flatMap((reply) => (reply ? [comment(reply, depth - 1)] : []))
            .filter((reply): reply is Comment => reply !== null)
        : [],
  }
}

/**
 * The thread for `term`, or `null` when nobody has commented yet — GitHub has
 * no discussion until the first comment creates one, and that is a normal
 * state rather than an error.
 *
 * Throws on anything else. The caller decides what an unreachable GitHub means
 * for the page; here it must not be confused with an empty thread.
 */
export async function loadThread(
  term: string,
  options: { token: string; repo: string; fetchImpl?: typeof fetch },
): Promise<Thread | null> {
  if (!THREAD_TERM.test(term)) throw new Error(`not a thread term: ${term}`)

  const request = options.fetchImpl ?? fetch
  const response = await request('https://api.github.com/graphql', {
    method: 'POST',
    headers: {
      authorization: `Bearer ${options.token}`,
      'content-type': 'application/json',
      // The API rejects a request that does not identify itself.
      'user-agent': 'nohrs.app',
    },
    body: JSON.stringify({
      query: THREAD_QUERY,
      variables: { search: `repo:${options.repo} in:title "${term}"` },
    }),
  })

  if (!response.ok) throw new Error(`github graphql: ${response.status}`)

  const payload = (await response.json()) as RawPayload
  // GraphQL reports its own failures inside a 200, so the status above is not
  // enough on its own.
  const failure = payload.errors?.[0]?.message
  if (failure) throw new Error(`github graphql: ${failure}`)

  // Search ranks rather than matches: `blog/nohrs` also returns
  // `blog/nohrs-and-gpui`. The title is the identity, so it is compared here
  // instead of trusting the order.
  const found = payload.data?.search?.nodes?.find((node) => node?.title === term)
  if (!found) return null

  return {
    url: found.url ?? '',
    total: found.comments?.totalCount ?? 0,
    reactions: reactions(found.reactionGroups),
    comments: (found.comments?.nodes ?? [])
      .flatMap((node) => (node ? [comment(node, 1)] : []))
      .filter((node): node is Comment => node !== null),
  }
}
