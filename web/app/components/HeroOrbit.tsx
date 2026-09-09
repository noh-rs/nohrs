import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from 'react'
import { createPortal } from 'react-dom'
import { t, type Lang } from '~/lib/i18n'
import { angleOf, originFor } from '~/lib/orbit'

/**
 * `centre` is the point of each screen the panel frames, and `zoom` how close
 * it stands. A whole 900px window shrunk into a 300px panel is a white
 * rectangle with grey lines in it; a detail of one is legible, and opening the
 * panel is then worth doing — the shot unzooms from that point as it grows.
 *
 * Every panel is cut by the edge it faces, and since each is turned to face
 * outward, what survives is always its lower half. The vertical centres are
 * alike for that reason: it is the band that stays on screen.
 */
const SHOTS = [
  { id: 'source', centre: [0.72, 0.45], zoom: 1.9 },
  { id: 'explorer', centre: [0.3, 0.11], zoom: 1.95 },
  { id: 'matches', centre: [0.42, 0.28], zoom: 1.9 },
  { id: 'preview', centre: [0.75, 0.2], zoom: 1.95 },
  { id: 'search', centre: [0.42, 0.13], zoom: 1.95 },
] as const

/**
 * How much of a panel's frame may hang past the top of its shot. Every panel in
 * the ring is cut by the edge it faces, so its top is not on the page — and the
 * app's content sits at the top of the window, which the frame has to be able
 * to reach. Kept well under the fraction of a panel that is actually off
 * screen — which holds at every width, since the ring is the layout everywhere.
 */
const SPILL = 0.3

const SHOT_WIDTH = 900
const SHOT_HEIGHT = 549

/** A shallow arc, drawn as the top of a circle so the name sits along the ring. */
const ARC = 'M -140 34.7 A 300 300 0 0 1 140 34.7'

function angleAt(index: number): number {
  return angleOf(index, SHOTS.length)
}

function focusOf({ centre, zoom }: (typeof SHOTS)[number]): string {
  const x = originFor(centre[0], zoom)
  const y = originFor(centre[1], zoom, SPILL)
  return `${(x * 100).toFixed(2)}% ${(y * 100).toFixed(2)}%`
}

type Phase = 'measuring' | 'open' | 'closing'

/**
 * The hero: five screens of the app on a ring, the tagline and the CTAs in the
 * middle of it. Clicking a screen — the panel or its name — lifts that panel out
 * of the ring and turns it upright as it grows to cover the page.
 *
 * Every panel is a frame of the recorded demo (`assets/doc/demo.gif`), so each
 * one is a state the app really reaches. docs/web.md §6.1 forbids mocking a
 * screen that does not exist, which is why the ring holds no Launcher and no
 * Plugin panel: those are not built yet.
 */
export function HeroOrbit({ lang, children }: { lang: Lang; children: ReactNode }) {
  const strings = t(lang).hero
  const cards = useRef<Array<HTMLButtonElement | null>>([])
  const panel = useRef<HTMLElement>(null)
  const closer = useRef<HTMLButtonElement>(null)
  const [view, setView] = useState<{ index: number; phase: Phase } | null>(null)

  /** Maps the opened panel onto the card it grew from, in viewport coordinates. */
  const place = useCallback((index: number) => {
    const node = panel.current
    const card = cards.current[index]
    if (!node || !card) return
    // The card is rotated about its own centre, so the box around it is centred
    // on the same point — which is what the panel has to be moved onto.
    const box = card.getBoundingClientRect()
    node.style.setProperty('--from-x', `${box.left + box.width / 2 - window.innerWidth / 2}px`)
    node.style.setProperty('--from-y', `${box.top + box.height / 2 - window.innerHeight / 2}px`)
    node.style.setProperty('--from-k', (card.offsetWidth / node.offsetWidth).toFixed(4))
    node.style.setProperty('--from-a', `${angleAt(index)}deg`)
    node.style.setProperty('--from-zoom', String(SHOTS[index].zoom))
    node.style.setProperty('--from-focus', focusOf(SHOTS[index]))
  }, [])

  // The panel is measured against a card, so it has to be laid out once before
  // the transform that maps one onto the other can exist. It is invisible for
  // that frame; `open` is what makes it visible and starts the transition.
  useEffect(() => {
    if (view?.phase !== 'measuring') return
    place(view.index)
    // Read a layout value back, so the browser resolves the style with the panel
    // still on the card. Without it both states land in one recalculation and
    // the panel is simply already open.
    void panel.current?.offsetWidth
    setView({ index: view.index, phase: 'open' })
  }, [view, place])

  const dismiss = useCallback(() => {
    if (!view || view.phase === 'closing') return
    // Re-measured rather than reused: the window may have been resized, and the
    // panel has to land back on where the card is now.
    place(view.index)
    setView({ index: view.index, phase: 'closing' })
  }, [view, place])

  useEffect(() => {
    if (view?.phase !== 'closing') return
    const index = view.index
    const node = panel.current
    const finish = () => {
      setView(null)
      cards.current[index]?.focus()
    }
    node?.addEventListener('transitionend', finish, { once: true })
    // `prefers-reduced-motion` collapses the duration to nothing, and a
    // transition that never runs never ends; the timer closes it either way.
    const timer = window.setTimeout(finish, 700)
    return () => {
      node?.removeEventListener('transitionend', finish)
      window.clearTimeout(timer)
    }
  }, [view])

  useEffect(() => {
    if (view?.phase !== 'open') return
    closer.current?.focus()
  }, [view?.phase])

  const isOpen = view !== null
  useEffect(() => {
    if (!isOpen) return
    const body = document.body
    const overflow = body.style.overflow
    const padding = body.style.paddingRight
    // Taking the scrollbar away shifts the page under the backdrop, and the
    // sticky header with it; its width is given back as padding.
    const gutter = window.innerWidth - document.documentElement.clientWidth
    body.style.overflow = 'hidden'
    if (gutter > 0) body.style.paddingRight = `${gutter}px`
    return () => {
      body.style.overflow = overflow
      body.style.paddingRight = padding
    }
  }, [isOpen])

  useEffect(() => {
    if (!isOpen) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        dismiss()
      } else if (event.key === 'Tab') {
        // Close is the only control in the dialog, so the trap is just: stay.
        event.preventDefault()
        closer.current?.focus()
      }
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [isOpen, dismiss])

  const shown = view === null ? null : strings.shots[SHOTS[view.index].id]

  return (
    <div className="orbit">
      <ul className="orbit-ring" aria-label={strings.screens}>
        {SHOTS.map((shot, index) => {
          const { id, zoom } = shot
          const angle = angleAt(index)
          const radians = (angle * Math.PI) / 180
          const sin = Math.sin(radians)
          const cos = Math.cos(radians)
          // The ring is squared off a little — a superellipse, not an ellipse —
          // so the diagonals reach out towards the corners of the stage
          // instead of floating whole in the middle of the page while the
          // panels on the axes are cropped. Taken all the way to a rectangle
          // it puts them *in* the corners, where a panel is a sliver.
          const reach = (Math.abs(sin) ** 3 + Math.abs(cos) ** 3) ** (-1 / 3)
          const copy = strings.shots[id]
          const open = () => setView({ index, phase: 'measuring' })
          return (
            <li
              key={id}
              className="orbit-item"
              style={
                {
                  '--sin': (sin * reach).toFixed(4),
                  '--cos': (cos * reach).toFixed(4),
                  '--a': `${angle}deg`,
                } as CSSProperties
              }
            >
              <button
                type="button"
                className="orbit-card"
                aria-haspopup="dialog"
                onClick={open}
                ref={(node) => {
                  cards.current[index] = node
                }}
              >
                <img
                  src={`/shots/${id}.png`}
                  alt=""
                  width={SHOT_WIDTH}
                  height={SHOT_HEIGHT}
                  decoding="async"
                  style={{ '--zoom': zoom, '--focus': focusOf(shot) } as CSSProperties}
                />
                <span className="orbit-name">{copy.label}</span>
              </button>
              {/* The name is the card's other half, not a second control: the
                  button above already carries it for anything that is not a
                  mouse. Only the glyphs take the pointer, so the boxes these
                  overlapping arcs occupy do not swallow each other's clicks. */}
              <svg
                className="orbit-tag"
                viewBox="-150 -40 300 80"
                width="300"
                height="80"
                aria-hidden="true"
                focusable="false"
              >
                <path id={`orbit-arc-${id}`} d={ARC} fill="none" />
                <text textAnchor="middle" onClick={open}>
                  <textPath href={`#orbit-arc-${id}`} startOffset="50%">
                    {copy.label}
                  </textPath>
                </text>
              </svg>
            </li>
          )
        })}
      </ul>

      <div className="orbit-core">{children}</div>

      {view !== null &&
        shown !== null &&
        createPortal(
          <div
            className="orbit-view"
            data-phase={view.phase}
            role="dialog"
            aria-modal="true"
            aria-label={shown.label}
          >
            <div className="orbit-scrim" onClick={dismiss} />
            <figure className="orbit-panel" ref={panel}>
              <div className="orbit-shot">
                <img
                  src={`/shots/${SHOTS[view.index].id}.png`}
                  alt=""
                  width={SHOT_WIDTH}
                  height={SHOT_HEIGHT}
                />
              </div>
              <figcaption className="orbit-caption">
                <span className="orbit-caption-label">{shown.label}</span>
                <span>{shown.caption}</span>
              </figcaption>
            </figure>
            <button type="button" className="orbit-close" onClick={dismiss} ref={closer}>
              <span aria-hidden="true">Esc</span>
              <span className="sr-only">{strings.close}</span>
              <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">
                <path d="M2 2 L12 12 M12 2 L2 12" stroke="currentColor" strokeWidth="1.5" />
              </svg>
            </button>
          </div>,
          document.body,
        )}
    </div>
  )
}
