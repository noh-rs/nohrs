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
 * Whether a release carries a macOS binary. This, not `hasReleases`, drives
 * the primary CTA and the download page: ADR 0008's rule is that the site must
 * never offer a download with nothing behind the link, and a release published
 * with notes but no attached asset is exactly that case.
 */
export const hasDownloads: boolean = github.releases.some((release) => release.assets.length > 0)
