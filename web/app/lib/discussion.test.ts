import { test } from 'node:test'
import assert from 'node:assert/strict'
import { loadThread, THREAD_TERM } from './discussion.ts'

/**
 * The thread reader turns one GitHub search result into what the page draws.
 * Everything checked here is a way that result can disagree with the shape the
 * component assumes — a near-miss title, a deleted author, an empty reaction
 * group — none of which is reproducible on demand against the live API.
 */

function payload(body: unknown, status = 200) {
  return async () => new Response(JSON.stringify(body), { status })
}

function search(nodes: unknown[]) {
  return { data: { search: { nodes } } }
}

function discussion(overrides: Record<string, unknown> = {}) {
  return {
    title: 'blog/hello-nohrs',
    url: 'https://github.com/noh-rs/nohrs/discussions/12',
    reactionGroups: [
      { content: 'HEART', reactors: { totalCount: 2 } },
      { content: 'ROCKET', reactors: { totalCount: 0 } },
    ],
    comments: {
      totalCount: 1,
      nodes: [
        {
          id: 'DC_1',
          url: 'https://github.com/noh-rs/nohrs/discussions/12#discussioncomment-1',
          createdAt: '2026-09-01T10:00:00Z',
          bodyHTML: '<p>Nice.</p>',
          author: { login: 'someone', url: 'https://github.com/someone' },
          reactionGroups: [{ content: 'THUMBS_UP', reactors: { totalCount: 1 } }],
          replies: {
            nodes: [
              {
                id: 'DC_2',
                url: 'https://github.com/noh-rs/nohrs/discussions/12#discussioncomment-2',
                createdAt: '2026-09-01T11:00:00Z',
                bodyHTML: '<p>Thanks.</p>',
                author: null,
                reactionGroups: [],
              },
            ],
          },
        },
      ],
    },
    ...overrides,
  }
}

async function load(fetchImpl: typeof fetch, term = 'blog/hello-nohrs') {
  return loadThread(term, { token: 'x', repo: 'noh-rs/nohrs', fetchImpl })
}

test('a term that is not a slug is refused before it reaches the query', async () => {
  // The term is interpolated into a search string, so this is the injection
  // boundary and not merely a tidy input check.
  for (const term of ['blog/a" OR x', 'blog/../secret', 'docs/keyboard', '', 'blog/']) {
    assert.equal(THREAD_TERM.test(term), false, term)
    await assert.rejects(() => load(payload(search([])), term), /not a thread term/)
  }
})

test('the exactly titled discussion wins over a better-ranked prefix match', async () => {
  const near = discussion({ title: 'blog/hello-nohrs-and-gpui', url: 'https://example.invalid/no' })
  const thread = await load(payload(search([near, discussion()])))
  assert.equal(thread?.url, 'https://github.com/noh-rs/nohrs/discussions/12')
})

test('no discussion yet is null rather than an error', async () => {
  assert.equal(await load(payload(search([]))), null)
  assert.equal(await load(payload(search([discussion({ title: 'blog/other' })]))), null)
})

test('reaction groups with nobody in them are dropped', async () => {
  const thread = await load(payload(search([discussion()])))
  assert.deepEqual(thread?.reactions, [{ content: 'HEART', count: 2 }])
})

test('a reply is carried, with a deleted author left nameless', async () => {
  const thread = await load(payload(search([discussion()])))
  const [comment] = thread?.comments ?? []
  assert.equal(comment?.author, 'someone')
  assert.deepEqual(comment?.reactions, [{ content: 'THUMBS_UP', count: 1 }])
  assert.equal(comment?.replies.length, 1)
  assert.equal(comment?.replies[0]?.author, null)
  assert.equal(comment?.replies[0]?.bodyHTML, '<p>Thanks.</p>')
})

test('replies do not nest a second time', async () => {
  // GitHub only threads one level, and a component that recursed further would
  // be drawing something the data cannot contain.
  const nested = discussion({
    comments: {
      totalCount: 1,
      nodes: [
        {
          id: 'DC_1',
          bodyHTML: '<p>one</p>',
          replies: {
            nodes: [
              {
                id: 'DC_2',
                bodyHTML: '<p>two</p>',
                replies: { nodes: [{ id: 'DC_3', bodyHTML: '<p>three</p>' }] },
              },
            ],
          },
        },
      ],
    },
  })
  const thread = await load(payload(search([nested])))
  assert.deepEqual(thread?.comments[0]?.replies[0]?.replies, [])
})

test('a comment without a body is dropped rather than drawn empty', async () => {
  const broken = discussion()
  broken.comments.nodes.push({ id: 'DC_9' } as never)
  const thread = await load(payload(search([broken])))
  assert.equal(thread?.comments.length, 1)
})

test('a GraphQL error inside a 200 is still a failure', async () => {
  await assert.rejects(
    () => load(payload({ errors: [{ message: 'Bad credentials' }] })),
    /Bad credentials/,
  )
})

test('an HTTP failure carries its status', async () => {
  await assert.rejects(() => load(payload({}, 401)), /github graphql: 401/)
})

test('the request is authenticated and scoped to the repository', async () => {
  let seen: { url: string; init?: RequestInit } | undefined
  const spy = (async (url: string | URL | Request, init?: RequestInit) => {
    seen = { url: String(url), init }
    return new Response(JSON.stringify(search([])))
  }) as typeof fetch

  await load(spy)
  assert.equal(seen?.url, 'https://api.github.com/graphql')
  const headers = seen?.init?.headers as Record<string, string>
  assert.equal(headers.authorization, 'Bearer x')
  const body = JSON.parse(String(seen?.init?.body)) as { variables: { search: string } }
  assert.equal(body.variables.search, 'repo:noh-rs/nohrs in:title "blog/hello-nohrs"')
})
