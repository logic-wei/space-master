import type { ReactNode } from 'react'

/**
 * A row of figures, each in its own recessed tile.
 *
 * `tight` is the variant used inside a foot bar, where the tiles sit between two
 * buttons and so size to their content instead of splitting the width.
 */
export function Stats({ tight, children }: { tight?: boolean; children: ReactNode }) {
  return <dl className={`stats${tight ? ' stats--tight' : ''}`}>{children}</dl>
}

/**
 * One figure. `tone="ok"` marks space the user gets back — freed, available, selected
 * for deletion — and nothing else, so the one colour in a stats row always means the
 * same thing.
 */
export function Stat({
  label,
  value,
  tone,
}: {
  label: string
  value: string
  tone?: 'ok'
}) {
  return (
    <div className="stats__item">
      <dt className="stats__label">{label}</dt>
      <dd className={`stats__value num${tone === 'ok' ? ' stats__value--ok' : ''}`}>
        {value}
      </dd>
    </div>
  )
}
