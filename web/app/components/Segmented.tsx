import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'

export type SegmentedOption<T extends string> = {
  value: T
  label: ReactNode
  /** Used for the accessible name when `label` is a glyph rather than a word. */
  title?: string
}

/**
 * A two-or-three-way switch whose pill slides to the selected option, so the
 * control reads as one thing with a position rather than a row of buttons that
 * light up. The thumb is measured rather than computed from a fraction: the
 * options have different label widths (`EN`/`JA`, `☀`/`☾`).
 */
export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
}: {
  options: SegmentedOption<T>[]
  value: T
  onChange: (value: T) => void
  label: string
}) {
  const container = useRef<HTMLDivElement>(null)
  const [thumb, setThumb] = useState<{ left: number; width: number } | null>(null)

  const measure = useCallback(() => {
    const root = container.current
    if (!root) return
    const buttons = [...root.querySelectorAll('button')]
    const active = buttons.find((button) => button.dataset.value === value)
    const first = buttons[0]
    if (!active || !first) return
    setThumb({ left: active.offsetLeft - first.offsetLeft, width: active.offsetWidth })
  }, [value])

  useEffect(() => {
    measure()
    // Web fonts land after hydration and change the label widths under the thumb.
    const observer = new ResizeObserver(measure)
    if (container.current) observer.observe(container.current)
    return () => observer.disconnect()
  }, [measure])

  return (
    <div className="seg" role="group" aria-label={label} ref={container}>
      <span
        className="thumb"
        aria-hidden="true"
        style={
          thumb
            ? { width: `${thumb.width}px`, transform: `translateX(${thumb.left}px)` }
            : { opacity: 0 }
        }
      />
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          data-value={option.value}
          aria-pressed={option.value === value}
          aria-label={option.title}
          title={option.title}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  )
}
