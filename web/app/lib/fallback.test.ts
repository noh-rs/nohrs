import { test } from 'node:test'
import assert from 'node:assert/strict'
import { CANONICAL_LANG, localize, withFallback } from './fallback.mjs'
import { CANONICAL_LANG as ROUTER_CANONICAL_LANG, LANGS } from './negotiate.ts'

/**
 * The selection rule is the one piece of logic the app and the build scripts
 * both have to reach the same answer on — the app decides what it will render,
 * the scripts decide what gets prerendered and listed. A disagreement is a 404
 * on a URL the site links to, which no page-level test would catch.
 */

type Entry = { lang: 'en' | 'ja'; slug: string; canonical?: 'en' | 'ja'; title: string }

const entries: Entry[] = [
  { lang: 'en', slug: 'both', title: 'Both (en)' },
  { lang: 'ja', slug: 'both', title: 'Both (ja)' },
  { lang: 'en', slug: 'english-only', title: 'English only' },
  { lang: 'ja', slug: 'japanese-only', canonical: 'ja', title: 'Japanese only' },
  { lang: 'ja', slug: 'undeclared-ja-only', title: 'Undeclared, ja only' },
]

test('a translated slug is served in the language asked for', () => {
  assert.equal(withFallback(entries, 'ja', 'both')?.title, 'Both (ja)')
  assert.equal(withFallback(entries, 'ja', 'both')?.fallbackFrom, undefined)
})

test('an untranslated slug falls back and says so', () => {
  const page = withFallback(entries, 'ja', 'english-only')
  assert.equal(page?.title, 'English only')
  assert.equal(page?.fallbackFrom, 'en')
})

test('the fallback source is the language the author wrote in', () => {
  // `canonical: ja` makes Japanese the original, so English falls back to it
  // rather than the slug disappearing from the English side.
  const page = withFallback(entries, 'en', 'japanese-only')
  assert.equal(page?.title, 'Japanese only')
  assert.equal(page?.fallbackFrom, 'ja')
})

test('without a declared canonical, the only file that exists is used', () => {
  assert.equal(withFallback(entries, 'en', 'undeclared-ja-only')?.fallbackFrom, 'ja')
})

test('a slug that exists in no language is undefined, not a fallback', () => {
  assert.equal(withFallback(entries, 'en', 'missing'), undefined)
})

test('every language lists every slug, so the two builds cover the same URLs', () => {
  const slugs = (lang: 'en' | 'ja') =>
    localize(entries, lang)
      .map((entry) => entry.slug)
      .sort()
  assert.deepEqual(slugs('en'), slugs('ja'))
  assert.deepEqual(slugs('en'), [
    'both',
    'english-only',
    'japanese-only',
    'undeclared-ja-only',
  ])
})

test('the canonical language matches the one the router and Workers use', () => {
  // `fallback.mjs` restates it because the build scripts run under plain node
  // and cannot import TypeScript.
  assert.equal(CANONICAL_LANG, ROUTER_CANONICAL_LANG)
  assert.ok(LANGS.includes(CANONICAL_LANG))
})
