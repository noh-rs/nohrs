import fallbackData from '../data/github.json'

export type Release = {
  tag: string
  name: string
  date: string | null
  prerelease: boolean
  url: string
  highlight: string | null
  assets: { name: string; url: string; size: number }[]
}

export type GitHubData = {
  fetchedAt: string
  source: string
  repo: {
    nameWithOwner: string
    description: string
    stars: number
    forks: number
    openIssues: number
    license: string
    defaultBranch: string
  }
  releases: Release[]
  commits: { sha: string; title: string; date: string; author: string; url: string }[]
}

/**
 * `github.generated.json` is written by `scripts/fetch-github.mjs` during
 * `prebuild` and is not committed; the glob resolves to nothing when it has
 * not run (a bare `vite dev`), and the committed snapshot is used instead.
 */
const generated = Object.values(
  import.meta.glob<{ default: GitHubData }>('../data/github.generated.json', { eager: true }),
)[0]

export const github: GitHubData = generated?.default ?? (fallbackData as GitHubData)

/** Whether anything has been released at all — drives the releases page. */
export const hasReleases: boolean = github.releases.length > 0

/**
 * The newest release that actually carries a macOS binary, if there is one.
 *
 * The download page renders this exact release, and the CTA is shown when it
 * exists, so the promise and the page cannot disagree. Asking whether *any*
 * release has an asset while rendering `releases[0]` was that disagreement: a
 * release published with notes before its build finished would light up the
 * CTA and then show an empty page.
 */
export const downloadRelease: Release | undefined = github.releases.find(
  (release) => release.assets.length > 0,
)

/**
 * Whether the site can offer a download at all. This, not `hasReleases`, drives
 * the primary CTA: ADR 0008's rule is that the site must never offer a download
 * with nothing behind the link.
 */
export const hasDownloads: boolean = downloadRelease !== undefined
