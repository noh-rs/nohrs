import { test } from 'node:test'
import assert from 'node:assert/strict'
import site from './site.ts'
import redirect from './noh-rs-redirect.ts'

/**
 * The two Workers are the only request-time logic the site has, and both of
 * them decide redirects — the class of bug that is invisible in a screenshot
 * and expensive once a search engine has cached it. They are pure functions of
 * a Request, so they can be checked without deploying anything.
 */

const env = {
  ASSETS: {
    fetch: async (request: Request) =>
      new Response(`asset:${new URL(request.url).pathname}`, { status: 200 }),
  },
}

async function get(url: string, headers: Record<string, string> = {}) {
  const response = await site.fetch(new Request(url, { headers }), env)
  return {
    status: response.status,
    location: response.headers.get('location'),
    vary: response.headers.get('vary'),
  }
}

test('the apex serves assets', async () => {
  const response = await site.fetch(new Request('https://nohrs.app/en/docs/keyboard'), env)
  assert.equal(response.status, 200)
  assert.equal(await response.text(), 'asset:/en/docs/keyboard')
})

test('a subdomain redirects to the apex, keeping path and query', async () => {
  assert.deepEqual(await get('https://www.nohrs.app/en/docs/keyboard?x=1'), {
    status: 301,
    location: 'https://nohrs.app/en/docs/keyboard?x=1',
    vary: null,
  })
})

test('the host redirect runs before language negotiation', async () => {
  // Otherwise `www` would 302 to `www/ja` and only then 301 to the apex.
  assert.deepEqual(await get('https://www.nohrs.app/', { 'accept-language': 'ja' }), {
    status: 301,
    location: 'https://nohrs.app/',
    vary: null,
  })
})

test('workers.dev and localhost are left alone', async () => {
  const preview = await site.fetch(new Request('https://nohrs-web.example.workers.dev/en/'), env)
  assert.equal(preview.status, 200)
  const local = await site.fetch(new Request('http://localhost:3000/en/'), env)
  assert.equal(local.status, 200)
})

test('/ negotiates a language and varies on what it read', async () => {
  assert.deepEqual(await get('https://nohrs.app/', { 'accept-language': 'ja,en;q=0.8' }), {
    status: 302,
    location: 'https://nohrs.app/ja',
    vary: 'Accept-Language, Cookie',
  })
  assert.deepEqual(await get('https://nohrs.app/'), {
    status: 302,
    location: 'https://nohrs.app/en',
    vary: 'Accept-Language, Cookie',
  })
})

test('a remembered language beats Accept-Language', async () => {
  assert.deepEqual(
    await get('https://nohrs.app/', { 'accept-language': 'ja', cookie: 'nohrs-lang=en' }),
    { status: 302, location: 'https://nohrs.app/en', vary: 'Accept-Language, Cookie' },
  )
})

test('/ sets the cookie only when there was none to read', async () => {
  const fresh = await site.fetch(new Request('https://nohrs.app/'), env)
  assert.match(fresh.headers.get('set-cookie') ?? '', /^nohrs-lang=en;/)
  const returning = await site.fetch(
    new Request('https://nohrs.app/', { headers: { cookie: 'nohrs-lang=ja' } }),
    env,
  )
  assert.equal(returning.headers.get('set-cookie'), null)
})

function short(url: string, headers: Record<string, string> = {}) {
  const response = redirect.fetch(new Request(url, { headers }))
  return { status: response.status, location: response.headers.get('location') }
}

test('noh.rs keeps a path that already carries a language, permanently', () => {
  assert.deepEqual(short('https://noh.rs/ja/docs/installation'), {
    status: 301,
    location: 'https://nohrs.app/ja/docs/installation',
  })
})

test('noh.rs resolves a path without a language, temporarily', () => {
  // A 301 here would pin the first visitor's language onto everyone.
  assert.deepEqual(short('https://noh.rs/docs/installation', { 'accept-language': 'ja' }), {
    status: 302,
    location: 'https://nohrs.app/ja/docs/installation',
  })
})

test('noh.rs expands the short schemes', () => {
  assert.deepEqual(short('https://noh.rs/p/git-status', { 'accept-language': 'en' }), {
    status: 302,
    location: 'https://nohrs.app/en/plugins/git-status',
  })
  assert.deepEqual(short('https://noh.rs/r/v0.1.0', { 'accept-language': 'en' }), {
    status: 302,
    location: 'https://nohrs.app/en/releases/v0.1.0',
  })
})

test('noh.rs root goes to the apex root, which then negotiates', () => {
  assert.deepEqual(short('https://noh.rs/'), { status: 301, location: 'https://nohrs.app/' })
})
