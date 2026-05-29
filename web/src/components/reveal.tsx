import * as React from 'react'

/* Scroll-into-view reveal (web.md §2.5: "controlled, meaningful motion only").

   Built on a native IntersectionObserver rather than framer's `whileInView`
   so the reveal can NEVER leave content stuck invisible: under
   prefers-reduced-motion, when IntersectionObserver is unavailable, or if the
   observer never fires, the element falls back to fully visible. The hidden
   start state is only ever applied on the client, after we've confirmed motion
   is allowed — SSR/no-JS render visible. */
export function Reveal({
  children,
  delay = 0,
  className,
  as = 'div',
}: {
  children: React.ReactNode
  delay?: number
  className?: string
  as?: 'div' | 'section' | 'li'
}) {
  const Component = as
  const ref = React.useRef<HTMLElement | null>(null)
  // `armed` flips to true only on a client that can + should animate. Until
  // then (SSR, reduced-motion, missing APIs) the element renders visible.
  const [armed, setArmed] = React.useState(false)
  const [shown, setShown] = React.useState(false)

  React.useEffect(() => {
    const node = ref.current
    if (!node) return
    const reduced = window.matchMedia(
      '(prefers-reduced-motion: reduce)',
    ).matches
    if (reduced || typeof IntersectionObserver === 'undefined') return

    setArmed(true)
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            setShown(true)
            observer.disconnect()
            break
          }
        }
      },
      { rootMargin: '0px 0px -80px 0px' },
    )
    observer.observe(node)
    return () => observer.disconnect()
  }, [])

  const hidden = armed && !shown

  return (
    <Component
      ref={ref as React.Ref<never>}
      className={className}
      style={{
        opacity: hidden ? 0 : 1,
        transform: hidden ? 'translateY(16px)' : 'none',
        transition: armed
          ? `opacity 0.5s cubic-bezier(0.22,1,0.36,1) ${delay}s, transform 0.5s cubic-bezier(0.22,1,0.36,1) ${delay}s`
          : undefined,
        willChange: armed ? 'opacity, transform' : undefined,
      }}
    >
      {children}
    </Component>
  )
}
