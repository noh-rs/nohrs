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
  /**
   * False when a page of this thread was left behind — more comments than
   * `COMMENT_PAGE`, or more replies than `REPLY_PAGE` under one of them. The
   * section says so and points at GitHub rather than quietly ending early.
   */
  complete: boolean
  reactions: Reaction[]
  comments: Comment[]
}

/**
 * Which terms name a thread. This is a validator before it is a convenience:
 * the term is interpolated into a GitHub search string below, and the pattern
 * is what makes a quote or an operator impossible to smuggle into it.
 */
export const THREAD_TERM = /^blog\/[a-z0-9][a-z0-9-]{0,80}$/

/** The Discussions category giscus was pointed at, and articles still use. */
const CATEGORY = 'blog'

const COMMENT_PAGE = 50
const REPLY_PAGE = 20

/**
 * Enough room that the exact title is on the page even when other discussions
 * mention it. Only titles are searched, so the field is narrow to begin with;
 * paginating a search whose realistic result count is one would be machinery
 * with nothing to do.
 */
const SEARCH_PAGE = 25

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
    search(type: DISCUSSION, query: $search, first: ${SEARCH_PAGE}) {
      nodes {
        ... on Discussion {
          title
          url
          createdAt
          category { slug }
          reactionGroups { content reactors { totalCount } }
          comments(first: ${COMMENT_PAGE}) {
            totalCount
            nodes {
              ${COMMENT_FIELDS}
              replies(first: ${REPLY_PAGE}) { totalCount nodes { ${COMMENT_FIELDS} } }
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
  replies?: { totalCount?: number; nodes?: (RawComment | null)[] }
}

type RawDiscussion = {
  title?: string
  url?: string
  createdAt?: string
  category?: { slug?: string } | null
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
 * Which of the search results is this article's thread.
 *
 * Search ranks rather than matches, so `blog/nohrs` also returns
 * `blog/nohrs-and-gpui`: the title is the identity and is compared exactly.
 * Two discussions can still carry the same title — anyone able to open one can
 * make a second — so the choice is pinned rather than left to whatever GitHub
 * ranked highest: the article's own category wins, and the oldest wins after
 * that. The original thread is the one people replied to, and a page that
 * changes which one it shows between two builds is worse than either.
 *
 * The category is a preference rather than a filter. If it is ever renamed,
 * this should keep finding threads instead of offering to start a second one.
 */
function pick(nodes: (RawDiscussion | null)[], term: string): RawDiscussion | undefined {
  const titled = nodes.filter((node): node is RawDiscussion => node?.title === term)
  const inCategory = titled.filter((node) => node.category?.slug?.toLowerCase() === CATEGORY)
  const candidates = inCategory.length > 0 ? inCategory : titled

  // The URL breaks a tie on creation time, which two discussions really can
  // share. Without it the last step would fall back to GitHub's order, which
  // is the thing this function exists to stop depending on.
  return candidates.reduce<RawDiscussion | undefined>((oldest, node) => {
    if (!oldest) return node
    const age = (node.createdAt ?? '').localeCompare(oldest.createdAt ?? '')
    if (age !== 0) return age < 0 ? node : oldest
    return (node.url ?? '') < (oldest.url ?? '') ? node : oldest
  }, undefined)
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

  const found = pick(payload.data?.search?.nodes ?? [], term)
  if (!found) return null

  const raw = (found.comments?.nodes ?? []).flatMap((node) => (node ? [node] : []))
  const total = found.comments?.totalCount ?? 0

  return {
    url: found.url ?? '',
    total,
    // A dropped page is reported rather than hidden. Paginating instead would
    // buy a thread nobody would read to the end of, at the cost of a second
    // and third API call on every article.
    complete:
      total <= raw.length &&
      raw.every((node) => (node.replies?.totalCount ?? 0) <= (node.replies?.nodes?.length ?? 0)),
    reactions: reactions(found.reactionGroups),
    comments: raw
      .flatMap((node) => [comment(node, 1)])
      .filter((node): node is Comment => node !== null),
  }
}
