export const SITE = {
  host: 'https://nohrs.app',
  shortHost: 'https://noh.rs',
  name: 'Nohrs',
  repo: 'noh-rs/nohrs',
  repoUrl: 'https://github.com/noh-rs/nohrs',
  discordUrl: 'https://discord.gg/dZM7fUtE94',
  xUrl: 'https://x.com/nohdotrs',
  xHandle: '@nohdotrs',
  discussionsUrl: 'https://github.com/noh-rs/nohrs/discussions',
  contributingUrl: 'https://github.com/noh-rs/nohrs/blob/develop/CONTRIBUTING.md',
  licenseUrl: 'https://github.com/noh-rs/nohrs/blob/develop/LICENSE',
  releasesUrl: 'https://github.com/noh-rs/nohrs/releases',
} as const

export const CF_ANALYTICS_TOKEN = import.meta.env.VITE_CF_ANALYTICS_TOKEN ?? ''
