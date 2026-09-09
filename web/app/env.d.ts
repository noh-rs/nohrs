/// <reference types="vite/client" />

declare module 'virtual:nohrs-plugins' {
  const registry: unknown[]
  export default registry
}

declare module '*.mdx' {
  import type { ComponentType } from 'react'
  export const frontmatter: Record<string, unknown>
  const MDXContent: ComponentType<Record<string, unknown>>
  export default MDXContent
}

interface ImportMetaEnv {
  readonly VITE_GISCUS_REPO_ID?: string
  readonly VITE_GISCUS_CATEGORY_ID?: string
  readonly VITE_CF_ANALYTICS_TOKEN?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
