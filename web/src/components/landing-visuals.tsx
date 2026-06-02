import * as React from 'react'
import { Command, FileText, Search, Shield } from 'lucide-react'
import { cn } from '#/lib/utils'
import { Kbd } from '#/components/ui/kbd'

/* Landing-page visuals: the per-feature mini-UI mocks for the feature
   showcase, the isometric line figures for the principles section, the
   contributor avatar marquee, and the tabbed code panel. Kept in one file so
   the page route stays composition-only. Every mock is decorative
   (`aria-hidden` at the call site) and honest — "coming" surfaces are clearly
   previews, not real screenshots. */

/** Explorer: window bar, sidebar with a selected folder, file list with one
    active row, and a preview pane. The one feature that ships today. */
export function ExplorerSketch() {
  return (
    <div
      aria-hidden
      className="surface overflow-hidden rounded-lg border border-border bg-background"
    >
      <div className="flex items-center gap-1.5 border-b border-border bg-muted/50 px-3 py-2">
        <span className="size-2 rounded-full bg-foreground/20" />
        <span className="size-2 rounded-full bg-foreground/20" />
        <span className="size-2 rounded-full bg-foreground/20" />
        <span className="ml-2 font-mono text-[10px] text-muted-foreground">
          ~/projects
        </span>
      </div>
      <div className="grid grid-cols-[92px_1fr_96px]">
        <div className="space-y-1 border-r border-border p-2.5">
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className="flex items-center gap-1.5">
              <span
                className={cn(
                  'size-2.5 rounded-sm',
                  i === 1 ? 'bg-brand' : 'bg-foreground/15',
                )}
              />
              <span
                className={cn(
                  'h-1.5 rounded-full',
                  i === 1 ? 'bg-foreground/30' : 'bg-foreground/12',
                )}
                style={{ width: `${[40, 52, 34, 46][i]}px` }}
              />
            </div>
          ))}
        </div>
        <div className="space-y-1 p-2.5">
          {[0, 1, 2, 3, 4, 5].map((i) => (
            <div
              key={i}
              className={cn(
                'flex items-center gap-2 rounded px-1.5 py-1',
                i === 2 && 'bg-brand/15 ring-1 ring-brand/30',
              )}
            >
              <span
                className={cn(
                  'size-2 rounded-[3px]',
                  i === 2 ? 'bg-brand-emphasis' : 'bg-foreground/20',
                )}
              />
              <span
                className="h-1.5 rounded-full bg-foreground/15"
                style={{ width: `${[64, 84, 72, 56, 92, 70][i]}px` }}
              />
            </div>
          ))}
        </div>
        <div className="space-y-1.5 border-l border-border p-2.5">
          <div className="mb-2 h-8 rounded bg-foreground/8" />
          {[100, 80, 90, 60].map((w, i) => (
            <div
              key={i}
              className="h-1.5 rounded-full bg-foreground/12"
              style={{ width: `${w}%` }}
            />
          ))}
        </div>
      </div>
    </div>
  )
}

/** Launcher: a ⌘K command bar with a fuzzy-matched result list. */
export function LauncherMock() {
  const results = [
    { label: 'Open project…', hint: 'action' },
    { label: 'report-q3.pdf', hint: '~/docs' },
    { label: 'Settings', hint: 'app' },
  ]
  return (
    <div
      aria-hidden
      className="surface overflow-hidden rounded-lg border border-border bg-background"
    >
      <div className="flex items-center gap-2 border-b border-border px-3 py-2.5">
        <Search className="size-4 text-muted-foreground" />
        <span className="text-sm text-muted-foreground">repo</span>
        <span className="ml-0.5 inline-block h-4 w-px animate-pulse bg-brand" />
        <Kbd className="ml-auto">⌘K</Kbd>
      </div>
      <div className="space-y-0.5 p-1.5">
        {results.map((r, i) => (
          <div
            key={r.label}
            className={cn(
              'flex items-center gap-2.5 rounded-md px-2.5 py-1.5',
              i === 0 && 'bg-brand/15 ring-1 ring-brand/25',
            )}
          >
            <span
              className={cn(
                'size-2 rounded-[3px]',
                i === 0 ? 'bg-brand-emphasis' : 'bg-foreground/25',
              )}
            />
            <span className="text-xs text-foreground">{r.label}</span>
            <span className="ml-auto font-mono text-[10px] text-muted-foreground">
              {r.hint}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

/** Plugins: stacked plugin rows, each with explicit capability badges. */
export function PluginsMock() {
  const plugins = [
    { name: 'git-status', caps: ['fs:read', 'exec'], on: true },
    { name: 'translate', caps: ['net', 'clipboard'], on: false },
    { name: 'colors', caps: ['fs:read'], on: false },
  ]
  return (
    <div aria-hidden className="space-y-2">
      {plugins.map((p, i) => (
        <div
          key={p.name}
          className={cn(
            'flex items-center gap-2.5 rounded-lg border border-border bg-background px-3 py-2.5',
            i === 0 && 'border-brand/40',
          )}
        >
          <Shield
            className={cn(
              'size-4',
              i === 0 ? 'text-brand-emphasis' : 'text-muted-foreground',
            )}
          />
          <span className="font-mono text-xs text-foreground">{p.name}</span>
          <div className="ml-auto flex gap-1">
            {p.caps.map((cap) => (
              <span
                key={cap}
                className="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-[9px] text-muted-foreground"
              >
                {cap}
              </span>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}

/** Search: a query bar over ranked content matches. */
export function SearchMock() {
  const hits = [
    { file: 'README.md', n: '12' },
    { file: 'engine.rs', n: '5' },
    { file: 'notes.txt', n: '3' },
  ]
  return (
    <div
      aria-hidden
      className="surface overflow-hidden rounded-lg border border-border bg-background"
    >
      <div className="flex items-center gap-2 border-b border-border px-3 py-2.5">
        <FileText className="size-4 text-muted-foreground" />
        <span className="text-sm text-foreground">
          fn&nbsp;<span className="text-brand-emphasis">render</span>
        </span>
        <span className="ml-auto font-mono text-[10px] text-muted-foreground">
          20 matches
        </span>
      </div>
      <div className="space-y-1 p-2.5">
        {hits.map((h) => (
          <div key={h.file} className="flex items-center gap-2">
            <span className="font-mono text-[11px] text-muted-foreground">
              {h.file}
            </span>
            <span className="h-px flex-1 bg-border" />
            <span className="rounded bg-brand/15 px-1.5 font-mono text-[10px] text-brand-emphasis">
              {h.n}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

/* ---- Isometric line figures (principles section) ------------------------- */

/** Abstract isometric wireframes echoing Linear's FIG drawings: one base
    stroke in the inherited text color, one element in brand tan. */
export function IsoFigure({
  variant,
}: {
  variant: 'keys' | 'sandbox' | 'stack'
}) {
  const base = {
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 1.25,
    strokeLinejoin: 'round' as const,
    strokeLinecap: 'round' as const,
  }
  const brand = { ...base, stroke: 'var(--color-brand)', strokeWidth: 1.5 }

  return (
    <svg
      viewBox="0 0 220 170"
      className="h-44 w-full text-foreground/30"
      aria-hidden
    >
      {variant === 'keys' ? (
        <>
          <path {...base} d="M110 50 L195 95 L110 140 L25 95 Z" opacity={0.5} />
          {/* key caps laid across the iso plane */}
          {[
            [70, 90],
            [105, 108],
            [140, 126],
            [105, 72],
            [140, 90],
          ].map(([x, y], i) => (
            <path
              key={i}
              {...(i === 0 ? brand : base)}
              d={`M${x} ${y - 9} L${x + 18} ${y} L${x} ${y + 9} L${x - 18} ${y} Z`}
            />
          ))}
        </>
      ) : null}

      {variant === 'sandbox' ? (
        <>
          {/* outer cube */}
          <path {...base} d="M110 30 L172 65 L110 100 L48 65 Z" />
          <path {...base} d="M48 65 L48 112 L110 147 L172 112 L172 65" />
          <path {...base} d="M110 100 L110 147" opacity={0.6} />
          {/* inner cube — the sandbox */}
          <path {...brand} d="M110 66 L140 83 L110 100 L80 83 Z" />
          <path {...brand} d="M80 83 L80 106 L110 123 L140 106 L140 83" />
          <path {...brand} d="M110 100 L110 123" />
        </>
      ) : null}

      {variant === 'stack' ? (
        <>
          {/* stacked slabs rising — top one in brand */}
          {[120, 96, 72, 48].map((y, i) => (
            <g key={y}>
              <path
                {...(i === 3 ? brand : base)}
                d={`M110 ${y - 28} L165 ${y} L110 ${y + 28} L55 ${y} Z`}
                opacity={i === 3 ? 1 : 0.5 + i * 0.12}
              />
              {i < 3 ? (
                <>
                  <path
                    {...base}
                    d={`M55 ${y} L55 ${y - 14} M165 ${y} L165 ${y - 14} M110 ${y + 28} L110 ${y + 14}`}
                    opacity={0.4}
                  />
                </>
              ) : null}
            </g>
          ))}
        </>
      ) : null}
    </svg>
  )
}

/* ---- Contributor avatar marquee (Zed-style OSS band) --------------------- */

function gradientFor(seed: number): string {
  // Warm tan-leaning placeholder gradients (used only if live avatars fail).
  const hue = 18 + ((seed * 13) % 34)
  return `linear-gradient(135deg, hsl(${hue} 58% 80%), hsl(${hue - 8} 46% 60%))`
}

export type Contributor = {
  login: string
  avatarUrl: string
  htmlUrl: string
}

/** Two opposing rails of contributor avatars. Falls back to abstract warm
    gradient chips when live GitHub avatars are unavailable. Decorative. */
export function AvatarMarquee({
  contributors,
}: {
  contributors?: Array<Contributor>
}) {
  // Repeat the (often small) contributor set until the rail is full, then the
  // markup renders it twice so the -50% loop stays seamless even for a handful
  // of people.
  const MIN_PER_ROW = 18
  let people: Array<Contributor> | null = null
  if (contributors && contributors.length) {
    people = []
    while (people.length < MIN_PER_ROW) people.push(...contributors)
  }
  const placeholders = Array.from({ length: MIN_PER_ROW }, (_, i) => i)

  const row = (reverse: boolean) => (
    <div className="marquee-mask">
      <div
        className={cn('marquee gap-2.5 py-1.5', reverse && 'marquee-reverse')}
      >
        {people
          ? [...people, ...people].map((person, i) => (
              <img
                key={`${person.login}-${i}`}
                src={person.avatarUrl}
                alt=""
                width={32}
                height={32}
                loading="lazy"
                className="size-8 shrink-0 rounded-full bg-muted object-cover ring-1 ring-border"
              />
            ))
          : [...placeholders, ...placeholders].map((seed, i) => (
              <span
                key={i}
                className="size-8 shrink-0 rounded-full ring-1 ring-border"
                style={{ background: gradientFor(seed) }}
              />
            ))}
      </div>
    </div>
  )

  return (
    <div aria-hidden className="space-y-2.5">
      {row(false)}
      {row(true)}
    </div>
  )
}

/* ---- Tabbed code panel (Vercel-style 2-col engineering band) ------------- */

function Kw({ children }: { children: React.ReactNode }) {
  return <span className="text-brand-emphasis">{children}</span>
}
function Mut({ children }: { children: React.ReactNode }) {
  return <span className="text-muted-foreground">{children}</span>
}

const codeBodies: Array<React.ReactNode> = [
  <>
    <Kw>$ </Kw>git clone https://github.com/noh-rs/nohrs{'\n'}
    <Kw>$ </Kw>cd nohrs{'\n'}
    <Kw>$ </Kw>cargo run --features gui{'\n'}
    <Mut> Compiling nohrs v0.1.0</Mut>
    {'\n'}
    <Mut> Finished — launching nohrs</Mut>
  </>,
  <>
    <Mut># ~/.config/nohrs/config.toml</Mut>
    {'\n\n'}
    <Kw>[appearance]</Kw>
    {'\n'}
    theme = <span className="text-foreground">"rust-tan"</span>
    {'\n'}
    font&nbsp; = <span className="text-foreground">"Geist Mono"</span>
    {'\n\n'}
    <Kw>[keymap]</Kw>
    {'\n'}
    launcher = <span className="text-foreground">"cmd+k"</span>
    {'\n'}
    search&nbsp;&nbsp; = <span className="text-foreground">"cmd+shift+f"</span>
  </>,
  <>
    <Mut>// hello-plugin/src/plugin.rs</Mut>
    {'\n'}
    <Kw>#[nohrs::plugin]</Kw>
    {'\n'}
    <Kw>fn</Kw> register(host: <Kw>&mut</Kw> Host) {'{'}
    {'\n'}
    {'    '}host.command(<span className="text-foreground">"greet"</span>, |ctx|
    {' {'}
    {'\n'}
    {'        '}ctx.notify(
    <span className="text-foreground">"Hello from WASM 👋"</span>);
    {'\n'}
    {'    '}
    {'}'});
    {'\n'}
    {'}'}
    {'\n'}
    <Mut>// permissions: fs:read, clipboard</Mut>
  </>,
]

export function EngineTabs({ labels }: { labels: Array<string> }) {
  const [active, setActive] = React.useState(0)
  return (
    <div className="border-gradient overflow-hidden rounded-xl bg-card">
      <div
        role="tablist"
        aria-label="Code examples"
        className="flex items-center gap-1 border-b border-border bg-muted/40 px-2"
      >
        {labels.map((label, i) => (
          <button
            key={label}
            type="button"
            role="tab"
            aria-selected={i === active}
            onClick={() => setActive(i)}
            className={cn(
              'relative px-3 py-2.5 text-sm font-medium transition-colors',
              i === active
                ? 'text-ink'
                : 'text-muted-foreground hover:text-foreground',
            )}
          >
            {label}
            {i === active ? (
              <span
                aria-hidden
                className="absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-brand"
              />
            ) : null}
          </button>
        ))}
        <span className="ml-auto hidden pr-2 font-mono text-xs text-muted-foreground sm:inline">
          <Command className="mr-1 inline size-3" />
          ~/nohrs
        </span>
      </div>
      <pre className="overflow-x-auto p-4 font-mono text-[0.8rem] leading-relaxed text-foreground">
        <code>{codeBodies[active]}</code>
      </pre>
    </div>
  )
}
