import { useState, type ReactNode } from 'react'

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

function CodeTabs({ tabs }: { tabs: { label: string; children: ReactNode }[] }) {
  const [active, setActive] = useState(0)
  return (
    <div className="my-7">
      <div className="flex gap-1 border-b border-line-soft">
        {tabs.map((tab, index) => (
          <button
            key={tab.label}
            type="button"
            onClick={() => setActive(index)}
            aria-pressed={index === active}
            className={`-mb-px border-b px-3 py-2 font-mono text-xs transition-colors duration-200 ${
              index === active ? 'border-tan-ink text-ink' : 'border-transparent text-muted hover:text-ink'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>
      <div className="pt-3">{tabs[active]?.children}</div>
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

export const mdxComponents = { Callout, Screenshot, CodeTabs, YouTube }
