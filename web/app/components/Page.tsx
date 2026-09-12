import type { ReactNode } from 'react'

export function Eyebrow({ children }: { children: ReactNode }) {
  return (
    <p className="eyebrow">
      <span>{children}</span>
    </p>
  )
}

/**
 * A landing-page band. Sections are separated by a hairline and generous
 * vertical space — never by a background fill or a card, which is what keeps
 * the page reading as one sheet of paper.
 */
export function Section({
  id,
  eyebrow,
  title,
  lede,
  compact = false,
  children,
}: {
  id?: string
  eyebrow?: string
  title?: string
  lede?: ReactNode
  /** For a section that directly follows a `PageHeader`, whose own padding is already there. */
  compact?: boolean
  children?: ReactNode
}) {
  return (
    <section id={id} className="border-t border-line-soft">
      <div className={compact ? 'frame py-[clamp(36px,4vw,56px)]' : 'frame py-[clamp(64px,9vw,116px)]'}>
        {eyebrow ? <Eyebrow>{eyebrow}</Eyebrow> : null}
        {title ? (
          <h2 className="mb-5 text-[clamp(1.5rem,2.6vw,2.05rem)] leading-tight">{title}</h2>
        ) : null}
        {lede ? <div className="mb-9 max-w-[65ch] text-ink-2">{lede}</div> : null}
        {children}
      </div>
    </section>
  )
}

/** The heading block every non-landing page opens with. */
export function PageHeader({
  eyebrow,
  title,
  lede,
  children,
}: {
  eyebrow?: string
  title: string
  lede?: ReactNode
  children?: ReactNode
}) {
  return (
    <header className="frame pt-[clamp(48px,6vw,80px)] pb-[clamp(32px,4vw,52px)]">
      {eyebrow ? <Eyebrow>{eyebrow}</Eyebrow> : null}
      <h1 className="text-[clamp(2rem,4.4vw,3rem)] leading-[1.08] tracking-[-0.035em]">{title}</h1>
      {lede ? <p className="mt-5 max-w-[58ch] text-[1.0625rem] text-ink-2">{lede}</p> : null}
      {children}
    </header>
  )
}

/**
 * The site's list row: a mono key, a description, and an optional trailing
 * cell, separated by hairlines. Roadmap phases, releases, commits, stack
 * entries and community links are all this shape, which is why none of them
 * needs a card.
 */
export function DefinitionRows({ children }: { children: ReactNode }) {
  return <dl className="border-t border-line">{children}</dl>
}

export function DefinitionRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-1 gap-x-6 gap-y-1 border-b border-line-soft py-[18px] sm:grid-cols-[136px_minmax(0,1fr)]">
      <dt className="font-mono text-[0.8125rem] tracking-[0.04em] text-muted">{label}</dt>
      <dd className="m-0 text-[0.9375rem] text-ink-2">{children}</dd>
    </div>
  )
}
