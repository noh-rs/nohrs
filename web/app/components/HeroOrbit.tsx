import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from 'react'
import { createPortal } from 'react-dom'
import { FluidOrb } from '~/components/FluidOrb'
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

/** The dialog's description. One panel is open at a time, so one id will do. */
const CAPTION_ID = 'orbit-shot-caption'

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

/**
 * The flight, as one animation per part rather than a set of transitions.
 *
 * A transition's start value is whatever the engine had resolved when the end
 * value changed, and engines disagree about when that is. Driven that way this
 * panel broke twice: once in Chromium, where the step that put it on the card
 * consumed the animation and nothing moved, and again in WebKit, where the
 * panel painted at full size, shrank in place and grew back — the flight from
 * the card never ran at all. Keyframes state both ends outright, so there is
 * nothing left to resolve at the wrong moment, and no invisible measuring frame
 * to sequence around.
 */
const LIFT = 620
const EASE = 'cubic-bezier(0.42, 0.04, 0.18, 1)'
const SITE_EASE = 'cubic-bezier(0.16, 1, 0.3, 1)'

type Phase = 'open' | 'closing'

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
  const frame = useRef<HTMLDivElement>(null)
  const scrim = useRef<HTMLDivElement>(null)
  const closer = useRef<HTMLButtonElement>(null)
  const flights = useRef<Animation[]>([])
  const [view, setView] = useState<{ index: number; phase: Phase } | null>(null)
  const [warm, setWarm] = useState(false)

  /**
   * Runs the flight between the card at `index` and the opened panel. Returns
   * when the panel has arrived, so the caller can unmount on the way back.
   *
   * Both ends are stated outright, and the way out starts from where each part
   * has actually got to. That is two requirements at once: it must not jump to
   * fully open before flying back when a reader closes it halfway through, and
   * it must land on where the card is *now* — the ring is sized from the
   * viewport, so a window resized while the panel was open has moved it.
   * Playing the way in backwards would satisfy the first and fail the second.
   */
  const fly = useCallback((index: number, direction: 'in' | 'out'): Promise<void> => {
    const node = panel.current
    const shot = frame.current
    const veil = scrim.current
    const card = cards.current[index]
    if (!node || !shot || !veil || !card) return Promise.resolve()

    // The card is rotated about its own centre, so the box around it is centred
    // on the same point — which is what the panel has to be moved onto.
    const box = card.getBoundingClientRect()
    const x = box.left + box.width / 2 - window.innerWidth / 2
    const y = box.top + box.height / 2 - window.innerHeight / 2
    const k = card.offsetWidth / node.offsetWidth
    const { zoom } = SHOTS[index]

    const onCard = `translate(${x}px, ${y}px) rotate(${angleAt(index)}deg) scale(${k})`
    // Divided by the same factor the panel is scaled by, so the corners read at
    // the card's radius while it is still the size of a card.
    const cardRadius = `${(10 / k).toFixed(2)}px`
    // Read afresh every flight: a reader who turns reduced motion on while the
    // panel is open gets it on the way back out.
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    const ms = reduced ? 0 : LIFT
    // The copy waits for the panel to arrive, and leaves at once on the way
    // out: holding a caption over a panel already shrinking back is a stutter.
    const late = {
      duration: reduced ? 0 : direction === 'in' ? 300 : 150,
      delay: reduced || direction === 'out' ? 0 : 380,
      easing: SITE_EASE,
    }

    const image = shot.querySelector('img')
    // Static through the flight: the point the shot is framed on is the point
    // the zoom unwinds from, and it is the same at both ends.
    if (image) image.style.transformOrigin = focusOf(SHOTS[index])

    type Part = [Element | null, 'transform' | 'borderRadius' | 'opacity', string, string, KeyframeAnimationOptions]
    const parts: Part[] = [
      [node, 'transform', onCard, 'none', {}],
      [shot, 'borderRadius', cardRadius, '12px', {}],
      [image, 'transform', `scale(${zoom})`, 'none', {}],
      [veil, 'opacity', '0', '1', { duration: reduced ? 0 : 380, easing: SITE_EASE }],
      [node.querySelector('.orbit-caption'), 'opacity', '0', '1', late],
      [closer.current, 'opacity', '0', '1', late],
    ]

    // Where each part is at this instant, taken before anything is cancelled —
    // cancelling drops it back to the resting style, and then there would be
    // nothing left to read.
    const here = parts.map(([target, property]) =>
      target ? getComputedStyle(target)[property] : '',
    )
    // Only the flights this component started. The close button also carries a
    // CSS colour transition, and `getAnimations()` would hand that back first
    // if the pointer were on it.
    for (const previous of flights.current) previous.cancel()
    flights.current = []

    let arrived: Promise<void> | null = null
    parts.forEach(([target, property, atCard, atRest, options], part) => {
      if (!target) return
      const from = direction === 'in' ? atCard : here[part]
      const to = direction === 'in' ? atRest : atCard
      const run = target.animate([{ [property]: from }, { [property]: to }], {
        duration: ms,
        easing: EASE,
        // `forwards` is what holds the panel on the card at the end of the way
        // out, for the frame between arriving and unmounting.
        fill: 'both',
        ...options,
      })
      flights.current.push(run)
      // Cancelling rejects, and most of these are promises nobody is holding —
      // an unhandled rejection each time a flight is interrupted.
      const settled = run.finished.then(
        () => undefined,
        () => undefined,
      )
      if (target === node) arrived = settled
    })

    return arrived ?? Promise.resolve()
  }, [])

  // Layout, not passive: the flight has to be running before the browser paints
  // the panel, or its first frame is the panel at full size in the middle of the
  // page — which is the flash this used to have.
  useLayoutEffect(() => {
    if (view?.phase !== 'open') return
    void fly(view.index, 'in')
  }, [view?.index, view?.phase, fly])

  const dismiss = useCallback(() => {
    if (!view || view.phase === 'closing') return
    setView({ index: view.index, phase: 'closing' })
  }, [view])

  useLayoutEffect(() => {
    if (view?.phase !== 'closing') return
    const index = view.index
    let live = true
    // Re-measured on the way out rather than reused: the window may have been
    // resized, and the panel has to land back on where the card is now.
    void fly(index, 'out').then(() => {
      if (!live) return
      setView(null)
      cards.current[index]?.focus()
    })
    return () => {
      live = false
    }
  }, [view?.index, view?.phase, fly])

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
      {/* The one shape the ring turns around. It lights up while the pointer is
          in the middle with it — the panels have their own answer to a pointer,
          and this is the tagline's. */}
      <FluidOrb
        bloom={warm}
        className="pointer-events-none absolute top-1/2 left-1/2 z-0 aspect-square w-[min(62vw,500px)] -translate-x-1/2 -translate-y-1/2 max-[1080px]:hidden"
      />

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
          const open = () => setView({ index, phase: 'open' })
          return (
            <li
              key={id}
              className="orbit-item"
              // The pair below the horizon are the closest together of any two
              // on the ring; a narrow window is where that starts to matter.
              data-low={cos < -0.3}
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

      <div
        className="orbit-core"
        onPointerEnter={() => setWarm(true)}
        onPointerLeave={() => setWarm(false)}
      >
        {children}
      </div>

      {view !== null &&
        shown !== null &&
        createPortal(
          <div
            className="orbit-view"
            data-phase={view.phase}
            role="dialog"
            aria-modal="true"
            aria-label={shown.label}
            aria-describedby={CAPTION_ID}
          >
            <div className="orbit-scrim" onClick={dismiss} ref={scrim} />
            <figure className="orbit-panel" ref={panel}>
              <div className="orbit-shot" ref={frame}>
                <img
                  src={`/shots/${SHOTS[view.index].id}.png`}
                  alt=""
                  width={SHOT_WIDTH}
                  height={SHOT_HEIGHT}
                />
              </div>
              <figcaption className="orbit-caption">
                <span className="orbit-caption-label">{shown.label}</span>
                <span id={CAPTION_ID}>{shown.caption}</span>
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
