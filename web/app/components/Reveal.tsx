import { useEffect, useRef, type ElementType, type ReactNode } from 'react'

/**
 * Fades a block in as it crosses the fold. The markup renders visible and the
 * hiding class is only applied once the observer is running, so a JS failure
 * costs the animation rather than the content.
 */
export function Reveal({
  children,
  as: Tag = 'div',
  className = '',
}: {
  children: ReactNode
  as?: ElementType
  className?: string
}) {
  const ref = useRef<HTMLElement>(null)

  useEffect(() => {
    const element = ref.current
    if (!element) return
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
    if (!('IntersectionObserver' in window)) return

    document.documentElement.classList.add('js-reveal')

    if (element.getBoundingClientRect().top < window.innerHeight) {
      element.classList.add('in')
      return
    }

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            entry.target.classList.add('in')
            observer.unobserve(entry.target)
          }
        }
      },
      { rootMargin: '0px 0px -12% 0px' },
    )
    observer.observe(element)
    return () => observer.disconnect()
  }, [])

  return (
    <Tag ref={ref} className={`reveal ${className}`}>
      {children}
    </Tag>
  )
}
