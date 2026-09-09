import { t, type Lang } from '~/lib/i18n'

/**
 * The roadmap table. Hairline rows in the same vocabulary as the rest of the
 * page — the current phase is marked by ink weight and a tan status word, not
 * by filling its row, which read as a different design language when tried.
 */
export function Phases({ lang }: { lang: Lang }) {
  const strings = t(lang).roadmap

  return (
    <div className="border-t border-line">
      {strings.phases.map((phase) => {
        const current = phase.state === 'inProgress'
        return (
          <div
            key={phase.id}
            className="grid items-baseline gap-x-6 gap-y-1.5 border-b border-line-soft py-[19px] last:border-line md:grid-cols-[48px_68px_minmax(0,1fr)_auto] max-md:grid-cols-[48px_minmax(0,1fr)]"
          >
            <span
              className={`font-mono text-sm leading-relaxed font-medium ${current ? 'text-ink' : 'text-muted'}`}
            >
              {phase.id}
            </span>
            <span className="font-mono text-sm leading-relaxed text-muted tabular-nums max-md:col-start-2">
              {phase.version}
            </span>
            <span className="text-[0.9375rem] text-ink-2 max-md:col-start-2">{phase.theme}</span>
            <span
              className={`font-mono text-xs leading-relaxed tracking-[0.06em] whitespace-nowrap max-md:col-start-2 ${
                current ? 'text-tan-ink' : 'text-muted'
              }`}
            >
              {strings.states[phase.state]}
            </span>
          </div>
        )
      })}
    </div>
  )
}
