/**
 * Writes RSS 2.0 and Atom feeds, one pair per language, into the built site.
 *
 * They are generated after `vite build` rather than served from a route:
 * the site is fully prerendered, so a feed route would be a server the rest of
 * the deployment does not need.
 */
import { mkdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { WEB_ROOT, LANGS, blogPosts } from './content-index.mjs'

const HOST = process.env.SITE_HOST ?? 'https://nohrs.app'
const OUT_ROOT = join(WEB_ROOT, 'dist', 'client')

const TITLES = {
  en: { title: 'Nohrs blog', description: 'Release announcements and notes on how Nohrs is built.' },
  ja: { title: 'Nohrs ブログ', description: 'リリースの告知と、Nohrs をどう作っているかの記録です。' },
}

/** A post URL, path-encoded and then XML-escaped, safe in text or an attribute. */
function postUrl(lang, slug) {
  return escapeXml(`${HOST}/${lang}/blog/${encodeURIComponent(slug)}`)
}

function escapeXml(value) {
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

function rss(lang, posts) {
  const meta = TITLES[lang]
  const items = posts
    .map((post) => {
      const url = postUrl(lang, post.slug)
      // `dc:creator`, not `author`: RSS 2.0 defines `author` as an email
      // address, and these are display names.
      return `    <item>
      <title>${escapeXml(post.title)}</title>
      <link>${url}</link>
      <guid isPermaLink="true">${url}</guid>
      <pubDate>${new Date(post.date).toUTCString()}</pubDate>
      <description>${escapeXml(post.description)}</description>
      <dc:creator>${escapeXml(post.author)}</dc:creator>
    </item>`
    })
    .join('\n')

  return `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <channel>
    <title>${escapeXml(meta.title)}</title>
    <link>${HOST}/${lang}/blog</link>
    <description>${escapeXml(meta.description)}</description>
    <language>${lang}</language>
    <atom:link href="${HOST}/${lang}/blog/rss.xml" rel="self" type="application/rss+xml" />
${items}
  </channel>
</rss>
`
}

function atom(lang, posts) {
  const meta = TITLES[lang]
  const updated = posts[0] ? new Date(posts[0].date).toISOString() : new Date().toISOString()
  const entries = posts
    .map((post) => {
      const url = postUrl(lang, post.slug)
      return `  <entry>
    <title>${escapeXml(post.title)}</title>
    <link href="${url}" />
    <id>${url}</id>
    <updated>${new Date(post.date).toISOString()}</updated>
    <summary>${escapeXml(post.description)}</summary>
    <author><name>${escapeXml(post.author)}</name></author>
  </entry>`
    })
    .join('\n')

  return `<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xml:lang="${lang}">
  <title>${escapeXml(meta.title)}</title>
  <subtitle>${escapeXml(meta.description)}</subtitle>
  <link href="${HOST}/${lang}/blog/atom.xml" rel="self" />
  <link href="${HOST}/${lang}/blog" />
  <id>${HOST}/${lang}/blog</id>
  <updated>${updated}</updated>
${entries}
</feed>
`
}

async function main() {
  for (const lang of LANGS) {
    const posts = blogPosts(lang)
    const dir = join(OUT_ROOT, lang, 'blog')
    await mkdir(dir, { recursive: true })
    await writeFile(join(dir, 'rss.xml'), rss(lang, posts), 'utf8')
    await writeFile(join(dir, 'atom.xml'), atom(lang, posts), 'utf8')
  }
  console.log(`feeds: rss.xml and atom.xml for ${LANGS.join(', ')}`)
}

main().catch((error) => {
  console.error(`feeds: ${error.message}`)
  process.exitCode = 1
})
