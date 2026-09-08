/**
 * Reads the repository's live signals once, at build time, and writes them to
 * `app/data/github.generated.json`. Nothing on the site calls GitHub at
 * request time: the numbers change slowly, and an edge request that depends on
 * a third-party API is a request that can fail.
 *
 * The release count decides the primary CTA (ADR 0008, 改訂 2026-09-08), so a
 * failed fetch must not be allowed to invent one. On any error this falls back
 * to the committed `app/data/github.json`, whose release list is empty — the
 * side that shows `Star on GitHub` rather than a download link to nowhere.
 */
import { readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { WEB_ROOT } from './content-index.mjs'

const REPO = 'noh-rs/nohrs'
const FALLBACK = join(WEB_ROOT, 'app', 'data', 'github.json')
const OUT = join(WEB_ROOT, 'app', 'data', 'github.generated.json')

const headers = {
  Accept: 'application/vnd.github+json',
  'User-Agent': 'nohrs-web-build',
  ...(process.env.GITHUB_TOKEN ? { Authorization: `Bearer ${process.env.GITHUB_TOKEN}` } : {}),
}

// A stalled response is the one failure the fallback below cannot reach: the
// promise never settles, so the build hangs instead of using the snapshot.
const TIMEOUT_MS = 20_000

async function api(path) {
  const response = await fetch(`https://api.github.com/${path}`, {
    headers,
    signal: AbortSignal.timeout(TIMEOUT_MS),
  })
  if (!response.ok) throw new Error(`${response.status} ${response.statusText} for ${path}`)
  return response.json()
}

function firstLine(message) {
  return message.split('\n', 1)[0]
}

/**
 * The first line of a release body that reads as a sentence.
 *
 * The body is Markdown but this is rendered as plain text, so the markers that
 * would otherwise show through are removed: heading hashes, asterisks and
 * backticks, and links reduced to their text. Underscores are deliberately
 * left alone — in release notes they are far more often part of an identifier
 * than an emphasis marker.
 *
 * Returns null rather than an empty string so the caller's `??` actually falls
 * through to the release name — a body that opens with a blank line used to
 * render as nothing at all.
 */
function highlightOf(body) {
  for (const line of (body ?? '').split('\n')) {
    const text = line
      .replace(/^\s*#{1,6}\s*/, '')
      .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
      .replace(/[*`]/g, '')
      .trim()
    if (text) return text
  }
  return null
}

async function main() {
  const [repo, releases, commits] = await Promise.all([
    api(`repos/${REPO}`),
    api(`repos/${REPO}/releases?per_page=20`),
    api(`repos/${REPO}/commits?per_page=5`),
  ])

  const data = {
    fetchedAt: new Date().toISOString().slice(0, 10),
    source: 'api',
    repo: {
      nameWithOwner: repo.full_name,
      description: repo.description,
      stars: repo.stargazers_count,
      forks: repo.forks_count,
      openIssues: repo.open_issues_count,
      license: repo.license?.spdx_id ?? 'MIT',
      defaultBranch: repo.default_branch,
    },
    releases: releases
      .filter((release) => !release.draft)
      .map((release) => ({
        tag: release.tag_name,
        name: release.name || release.tag_name,
        date: release.published_at?.slice(0, 10) ?? null,
        prerelease: release.prerelease,
        url: release.html_url,
        highlight: highlightOf(release.body),
        assets: (release.assets ?? [])
          .filter((asset) => /\.(dmg|zip|tar\.gz|pkg)$/.test(asset.name))
          .map((asset) => ({ name: asset.name, url: asset.browser_download_url, size: asset.size })),
      })),
    commits: commits.map((commit) => ({
      sha: commit.sha.slice(0, 7),
      title: firstLine(commit.commit.message),
      date: commit.commit.author.date.slice(0, 10),
      author: commit.author?.login ?? commit.commit.author.name,
      url: commit.html_url,
    })),
  }

  await writeFile(OUT, `${JSON.stringify(data, null, 2)}\n`, 'utf8')
  console.log(`github: ${data.repo.stars} stars, ${data.releases.length} releases`)
}

main().catch(async (error) => {
  console.warn(`github: API unavailable (${error.message}) — using the committed snapshot`)
  await writeFile(OUT, await readFile(FALLBACK, 'utf8'), 'utf8')
})
