import * as React from 'react'
import { cn } from '#/lib/utils'

/** A keyboard-shortcut chip, e.g. <Kbd>⌘K</Kbd>. Styled via the `.kbd` class. */
export function Kbd({
  className,
  children,
}: {
  className?: string
  children: React.ReactNode
}) {
  return <kbd className={cn('kbd', className)}>{children}</kbd>
}
