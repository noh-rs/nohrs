import * as React from 'react'
import { cn } from '#/lib/utils'

type Variant = 'brand' | 'outline' | 'ghost'
type Size = 'sm' | 'md' | 'lg'

const base =
  'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-[1.1em] [&_svg]:shrink-0'

const variants: Record<Variant, string> = {
  brand:
    'bg-brand text-brand-foreground shadow-sm shadow-brand/30 hover:bg-brand/90 hover:shadow-md hover:shadow-brand/40 active:bg-brand/85',
  outline: 'border border-border bg-transparent text-foreground hover:bg-muted',
  ghost: 'bg-transparent text-foreground hover:bg-muted',
}

const sizes: Record<Size, string> = {
  sm: 'h-9 px-3 text-sm',
  md: 'h-10 px-4 text-sm',
  lg: 'h-12 px-6 text-base',
}

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant
  size?: Size
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = 'brand', size = 'md', ...props }, ref) => (
    <button
      ref={ref}
      className={cn(base, variants[variant], sizes[size], className)}
      {...props}
    />
  ),
)
Button.displayName = 'Button'

/** Shared button styling for anchor/Link elements. */
export function buttonClasses(
  variant: Variant = 'brand',
  size: Size = 'md',
  className?: string,
) {
  return cn(base, variants[variant], sizes[size], className)
}
