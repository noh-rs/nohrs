import * as React from 'react'
import { cn } from '#/lib/utils'

/* A surface with a 1px gradient hairline and a pointer-follow tan spotlight.
   The spotlight is pure CSS (`.spotlight`), disabled on touch and under
   prefers-reduced-motion; here we only feed it the pointer position. */
export function SpotlightCard({
  className,
  children,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) {
  function handleMove(event: React.MouseEvent<HTMLDivElement>) {
    const el = event.currentTarget
    const rect = el.getBoundingClientRect()
    el.style.setProperty('--mx', `${event.clientX - rect.left}px`)
    el.style.setProperty('--my', `${event.clientY - rect.top}px`)
  }

  return (
    <div
      onMouseMove={handleMove}
      className={cn(
        'spotlight border-gradient surface rounded-xl bg-card',
        'transition-[box-shadow,transform] duration-200',
        'hover:-translate-y-0.5 hover:shadow-xl hover:shadow-brand/5',
        className,
      )}
      {...props}
    >
      {children}
    </div>
  )
}
