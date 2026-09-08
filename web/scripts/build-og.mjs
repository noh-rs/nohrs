/**
 * Renders the Open Graph cards into `public/og/` before the site is built.
 *
 * docs/web.md §2 puts Satori on a Worker; this runs it at build time instead.
 * The inputs (a title and a language) are known when the site is built, so
 * generating on request would mean paying for a rasteriser at the edge, and
 * caching it, to produce a file that never changes.
 *
 * Like the other build scripts, a failure here degrades rather than blocks:
 * pages keep their `og:image` tag, and the card is simply missing.
 */
import { mkdir, writeFile, readFile, access } from 'node:fs/promises'
import { join } from 'node:path'
import satori from 'satori'
import { Resvg } from '@resvg/resvg-js'
import { WEB_ROOT, LANGS, blogPosts } from './content-index.mjs'

const OUT_DIR = join(WEB_ROOT, 'public', 'og')
const FONT_CACHE = join(WEB_ROOT, 'node_modules', '.cache', 'og-fonts')

const PAPER = '#FCFAF8'
const INK = '#191510'
const MUTED = '#6F6459'
const LINE = '#E3DBD0'
const TAN_INK = '#94541F'

// An empty User-Agent makes Google Fonts serve TrueType; Satori cannot read woff2.
async function fetchFont(family, weight, file) {
  const cached = join(FONT_CACHE, file)
  try {
    await access(cached)
    return readFile(cached)
  } catch {
    // not cached yet
  }
  const cssUrl = `https://fonts.googleapis.com/css2?family=${family}:wght@${weight}&display=swap`
  const css = await (await fetch(cssUrl, { headers: { 'User-Agent': '' } })).text()
  const url = /src: url\((https:[^)]+\.ttf)\)/.exec(css)?.[1]
  if (!url) throw new Error(`no TrueType face for ${family} ${weight}`)
  const buffer = Buffer.from(await (await fetch(url)).arrayBuffer())
  await mkdir(FONT_CACHE, { recursive: true })
  await writeFile(cached, buffer)
  return buffer
}

function card({ eyebrow, title, footer }) {
  return {
    type: 'div',
    props: {
      style: {
        width: 1200,
        height: 630,
        display: 'flex',
        flexDirection: 'column',
        justifyContent: 'space-between',
        background: PAPER,
        color: INK,
        padding: '72px 80px',
        fontFamily: 'Inter',
      },
      children: [
        {
          type: 'div',
          props: {
            style: { display: 'flex', alignItems: 'center', gap: 16, color: MUTED, fontSize: 22, letterSpacing: 3 },
            children: [
              { type: 'div', props: { children: eyebrow.toUpperCase() } },
              { type: 'div', props: { style: { flex: 1, height: 1, background: LINE } } },
            ],
          },
        },
        {
          type: 'div',
          props: {
            style: {
              display: 'flex',
              fontSize: title.length > 46 ? 56 : 72,
              lineHeight: 1.12,
              letterSpacing: -2,
              fontWeight: 600,
              maxWidth: 980,
            },
            children: title,
          },
        },
        {
          type: 'div',
          props: {
            style: {
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'flex-end',
              borderTop: `1px solid ${LINE}`,
              paddingTop: 28,
              fontSize: 24,
              color: MUTED,
            },
            children: [
              { type: 'div', props: { style: { color: TAN_INK }, children: footer } },
              { type: 'div', props: { children: 'nohrs.app' } },
            ],
          },
        },
      ],
    },
  }
}

async function render(fonts, name, spec) {
  const svg = await satori(card(spec), { width: 1200, height: 630, fonts })
  const png = new Resvg(svg, { fitTo: { mode: 'width', value: 1200 } }).render().asPng()
  await writeFile(join(OUT_DIR, name), png)
}

/** The one raster icon the site needs; everything else uses the SVG favicon. */
async function appleTouchIcon() {
  const svg = await readFile(join(WEB_ROOT, 'public', 'favicon.svg'), 'utf8')
  const png = new Resvg(svg, { fitTo: { mode: 'width', value: 180 } }).render().asPng()
  await writeFile(join(WEB_ROOT, 'public', 'apple-touch-icon.png'), png)
}

async function main() {
  await mkdir(OUT_DIR, { recursive: true })
  await appleTouchIcon()

  const fonts = [
    { name: 'Inter', data: await fetchFont('Inter', 600, 'inter-600.ttf'), weight: 600, style: 'normal' },
    {
      name: 'Noto Sans JP',
      data: await fetchFont('Noto+Sans+JP', 500, 'noto-sans-jp-500.ttf'),
      weight: 500,
      style: 'normal',
    },
  ]

  await render(fonts, 'default.png', {
    eyebrow: 'Nohrs',
    title: 'Launcher × Explorer',
    footer: 'A keyboard-driven file explorer for macOS, built in Rust',
  })

  let count = 1
  for (const lang of LANGS) {
    for (const post of blogPosts(lang)) {
      // Keyed by language as well as slug: the card carries the article's own
      // title, which differs between the two trees.
      await render(fonts, `blog-${lang}-${post.slug}.png`, {
        eyebrow: lang === 'ja' ? 'ブログ' : 'Blog',
        title: post.title,
        footer: post.author,
      })
      count += 1
    }
  }

  console.log(`og: ${count} cards in public/og`)
}

main().catch((error) => {
  console.warn(`og: skipped — ${error.message}`)
})
