import * as React from 'react'
import { cn } from '#/lib/utils'

/* A terminal-styled code block. `label` shows a minimal prompt/path header;
   callers compose the body with the token helpers below for light syntax
   coloring (no highlighter dependency). */
export function CodeBlock({
  label,
  className,
  children,
}: {
  label?: string
  className?: string
  children: React.ReactNode
}) {
  return (
    <div
      className={cn(
        'border-gradient overflow-hidden rounded-xl bg-card',
        className,
      )}
    >
      {label ? (
        <div className="flex items-center gap-2 border-b border-border bg-muted/50 px-4 py-2.5">
          <span aria-hidden className="flex gap-1.5">
            <span className="size-2.5 rounded-full bg-foreground/15" />
            <span className="size-2.5 rounded-full bg-foreground/15" />
            <span className="size-2.5 rounded-full bg-foreground/15" />
          </span>
          <span className="ml-1 font-mono text-xs text-muted-foreground">
            {label}
          </span>
        </div>
      ) : null}
      <pre className="overflow-x-auto p-4 font-mono text-sm leading-relaxed text-foreground">
        <code>{children}</code>
      </pre>
    </div>
  )
}

/** Shell prompt sigil (`$`). */
export function Prompt() {
  return <span className="select-none text-brand-emphasis">$ </span>
}

/** A comment / muted line. */
export function Comment({ children }: { children: React.ReactNode }) {
  return <span className="text-muted-foreground">{children}</span>
}
