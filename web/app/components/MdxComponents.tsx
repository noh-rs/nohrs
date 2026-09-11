import { useId, useState, type KeyboardEvent, type ReactNode } from 'react'

/** A short aside inside an article. Marked by a rule, not a coloured panel. */
function Callout({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <aside className="my-7 border-l-2 border-tan pl-5">
      {title ? (
        <p className="mb-1.5 font-mono text-[0.6875rem] tracking-[0.12em] text-tan-ink uppercase">
          {title}
        </p>
      ) : null}
      <div className="text-[0.9375rem] text-muted">{children}</div>
    </aside>
  )
}

function Screenshot({ src, alt, caption }: { src: string; alt: string; caption?: string }) {
  return (
    <figure className="my-9">
      <img src={src} alt={alt} loading="lazy" decoding="async" className="block h-auto w-full" />
      {caption ? (
        <figcaption className="mt-3 font-mono text-xs text-muted">{caption}</figcaption>
      ) : null}
    </figure>
  )
}

/**
 * Proper tablist semantics rather than a row of `aria-pressed` buttons: the
 * options are mutually exclusive, and a screen reader has to be able to tell
 * that — and to associate the selected label with the code it reveals. Arrow
 * keys move between tabs, which is what the role promises.
 */
function CodeTabs({ tabs }: { tabs: { label: string; children: ReactNode }[] }) {
  const [active, setActive] = useState(0)
  const id = useId()

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const delta = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0
    if (!delta) return
    event.preventDefault()
    const next = (active + delta + tabs.length) % tabs.length
    setActive(next)
    document.getElementById(`${id}-tab-${next}`)?.focus()
  }

  return (
    <div className="my-7">
      <div role="tablist" className="flex gap-1 border-b border-line-soft" onKeyDown={onKeyDown}>
        {tabs.map((tab, index) => (
          <button
            key={tab.label}
            id={`${id}-tab-${index}`}
            type="button"
            role="tab"
            aria-selected={index === active}
            aria-controls={`${id}-panel-${index}`}
            tabIndex={index === active ? 0 : -1}
            onClick={() => setActive(index)}
            className={`-mb-px border-b px-3 py-2 font-mono text-xs transition-colors duration-200 ${
              index === active ? 'border-tan-ink text-ink' : 'border-transparent text-muted hover:text-ink'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {tabs.map((tab, index) => (
        <div
          key={tab.label}
          id={`${id}-panel-${index}`}
          role="tabpanel"
          aria-labelledby={`${id}-tab-${index}`}
          hidden={index !== active}
          // A panel holding only a `<pre>` has nothing focusable in it, so
          // without this Tab leaves the tablist and skips the code entirely —
          // and a keyboard reader can never scroll a wide block sideways.
          tabIndex={0}
          className="pt-3"
        >
          {tab.children}
        </div>
      ))}
    </div>
  )
}

/**
 * Tables scroll inside their own box rather than widening the page.
 *
 * Cells are usually inline code, and a token like `cx.background_executor()`
 * has no wrap point in it — so a narrow screen cannot shrink a column by
 * reflowing it, and the table pushes the whole document sideways instead.
 */
function Table({ children }: { children: ReactNode }) {
  return (
    <div className="overflow-x-auto">
      <table>{children}</table>
    </div>
  )
}

function YouTube({ id, title }: { id: string; title: string }) {
  return (
    <div className="my-9 aspect-video w-full overflow-hidden rounded border border-line-soft">
      <iframe
        src={`https://www.youtube-nocookie.com/embed/${id}`}
        title={title}
        loading="lazy"
        allow="accelerometer; clipboard-write; encrypted-media; picture-in-picture"
        allowFullScreen
        className="h-full w-full border-0"
      />
    </div>
  )
}

export const mdxComponents = { Callout, Screenshot, CodeTabs, YouTube, table: Table }
