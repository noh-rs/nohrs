/**
 * Downloads the woff2 files Google Fonts would otherwise serve from
 * fonts.gstatic.com and writes them, plus the rewritten @font-face CSS, into
 * `public/fonts/`. ADR 0008 forbids linking Google Fonts directly (FOUT, GDPR,
 * edge latency), and Noto Sans JP has to stay split across its ~120 unicode
 * ranges — the browser then fetches only the ranges a page actually uses,
 * which is what "subset the Japanese font" means in practice.
 *
 * A failure here degrades to the fallback stack rather than breaking the
 * build: no network in a sandbox should not stop someone running the site.
 */
import { mkdir, writeFile, readdir, access } from 'node:fs/promises'
import { join } from 'node:path'
import { WEB_ROOT } from './content-index.mjs'

const OUT_DIR = join(WEB_ROOT, 'public', 'fonts')

const FAMILIES = [
  'Inter:wght@400;500;600',
  'JetBrains+Mono:wght@400;500',
  'Noto+Sans+JP:wght@400;500;700',
]

// Google serves woff2 only to user agents it recognises as supporting it.
const UA =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36'

async function fetchText(url) {
  const response = await fetch(url, { headers: { 'User-Agent': UA } })
  if (!response.ok) throw new Error(`${response.status} ${response.statusText} for ${url}`)
  return response.text()
}

async function main() {
  await mkdir(OUT_DIR, { recursive: true })

  const url = `https://fonts.googleapis.com/css2?${FAMILIES.map((f) => `family=${f}`).join('&')}&display=swap`
  const css = await fetchText(url)

  const remote = [...new Set(css.match(/https:\/\/fonts\.gstatic\.com\/[^)]+/g) ?? [])]
  if (remote.length === 0) throw new Error('no font files referenced in the Google Fonts CSS')

  const names = new Map()
  for (const href of remote) {
    const name = href.split('/').slice(-2).join('-')
    names.set(href, name)
  }

  let downloaded = 0
  await Promise.all(
    [...names].map(async ([href, name]) => {
      const target = join(OUT_DIR, name)
      try {
        await access(target)
        return
      } catch {
        // not cached yet
      }
      const response = await fetch(href, { headers: { 'User-Agent': UA } })
      if (!response.ok) throw new Error(`${response.status} for ${href}`)
      await writeFile(target, Buffer.from(await response.arrayBuffer()))
      downloaded += 1
    }),
  )

  let local = css
  for (const [href, name] of names) local = local.split(href).join(`/fonts/${name}`)
  await writeFile(join(OUT_DIR, 'fonts.css'), local, 'utf8')

  const total = (await readdir(OUT_DIR)).length - 1
  console.log(`fonts: ${total} files in public/fonts (${downloaded} newly downloaded)`)
}

main().catch((error) => {
  console.warn(`fonts: skipped — ${error.message}`)
  console.warn('fonts: the site will fall back to system faces; run again with network access.')
})
