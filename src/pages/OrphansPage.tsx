import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { PrivacyNotice } from '../components/PrivacyNotice'
import { ScanningState } from '../components/ui/ScanningState'
import { Stat, Stats } from '../components/ui/Stat'
import { formatBytes } from '../lib/format'
import {
  cancelScan,
  previewClean,
  revealOrphan,
  runOrphanScan,
  toErrorDescriptor,
  type ErrorDescriptor,
} from '../lib/ipc'
import type {
  CleanPlan,
  OrphanBucket,
  OrphanEvidence,
  OrphanLocation,
  OrphanReport,
  OrphanRow,
} from '../lib/types'
import { PlanReview } from './PlanReview'

/**
 * Leftovers always move to the Trash. Guard rule R15 is what actually guarantees it —
 * these paths are outside the one-click catalog, so `permanent` on any of them is
 * refused whatever this page asks for.
 */
const MODE = 'trash' as const

/** Most to least sure. Also the render order, which is the order to read them in. */
const BUCKETS = ['likely', 'possible', 'unclear', 'keep'] as const

/**
 * How much work it takes to tick a row, per bucket.
 *
 * `likely` starts ticked; `possible` has a checkbox only once the row is expanded, so a
 * selection there is always made by someone who has read the evidence; `unclear` and
 * `keep` cannot be ticked here at all. The collapsing is the same idea one level up: a
 * group nobody opened contributes nothing.
 */
const OPEN_BY_DEFAULT: Record<OrphanBucket, boolean> = {
  likely: true,
  possible: true,
  unclear: false,
  keep: false,
}

/**
 * Evidence that argues for keeping the data, styled apart from the rest.
 *
 * Duplicates the sign of the backend's weights, deliberately: which way a fact reads is
 * presentation, and the wording it sits next to lives here too. A weight changing sign
 * without this list following would show a red chip on a green argument — visibly odd,
 * and it changes nothing about what may be deleted.
 */
const AGAINST: ReadonlySet<OrphanEvidence> = new Set<OrphanEvidence>([
  'recentActivity',
  'onlyPreferences',
  'shortId',
  'sameVendor',
  'holdsDatabase',
  'tiny',
])

/**
 * Locations macOS protects with Full Disk Access.
 *
 * They can be listed and measured without it, but not moved to the Trash — and
 * NSFileManager reports that as a generic error, mid-batch, after the user has already
 * approved the plan. Nearly every leftover with real size in it lives in one of these.
 */
const NEEDS_ACCESS: ReadonlySet<OrphanLocation> = new Set<OrphanLocation>([
  'containers',
  'groupContainers',
])

/**
 * Data belonging to software that is no longer installed.
 *
 * The one page that does not use `CleanPanel`. Its rows arrive all at once rather than
 * one at a time — a row means nothing until it has been placed in a bucket relative to
 * the others — and what decides whether the user ticks one is the evidence rather than
 * the size, so the layout has little in common with the catalog pages beyond the
 * scan-select-review flow.
 */
export function OrphansPage() {
  const { t, i18n } = useTranslation()
  const locale = i18n.language
  const runId = useRef(0)
  const [scanning, setScanning] = useState(false)
  const [bytes, setBytes] = useState(0)
  const [report, setReport] = useState<OrphanReport | null>(null)
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set())
  const [error, setError] = useState<ErrorDescriptor | null>(null)
  const [plan, setPlan] = useState<CleanPlan | null>(null)

  const start = useCallback(async () => {
    const id = ++runId.current
    setScanning(true)
    setError(null)
    setReport(null)
    setSelected(new Set())
    setBytes(0)
    try {
      const next = await runOrphanScan((event) => {
        if (runId.current === id && event.kind === 'progress') setBytes(event.bytes)
      })
      if (runId.current !== id) return
      setReport(next)
      setSelected(new Set(next.rows.filter((r) => r.bucket === 'likely').map((r) => r.id)))
    } catch (e: unknown) {
      if (runId.current === id) setError(toErrorDescriptor(e))
    } finally {
      if (runId.current === id) setScanning(false)
    }
  }, [])

  // A scan left running after unmount keeps a thread pool walking container trees
  // nobody is waiting on.
  useEffect(
    () => () => {
      runId.current += 1
      void cancelScan()
    },
    [],
  )

  const toggle = useCallback((id: string) => {
    setSelected((prev) => {
      const next = new Set(prev)
      if (!next.delete(id)) next.add(id)
      return next
    })
  }, [])

  const reveal = useCallback(
    (id: string, place: number) => {
      if (report === null) return
      void revealOrphan(report.generation, id, place).catch((e: unknown) => {
        setError(toErrorDescriptor(e))
      })
    },
    [report],
  )

  const review = async () => {
    if (report === null) return
    setError(null)
    try {
      setPlan(await previewClean(report.generation, [...selected], MODE))
    } catch (e: unknown) {
      setError(toErrorDescriptor(e))
    }
  }

  const rows = report?.rows ?? []
  const selectedBytes = rows.filter((r) => selected.has(r.id)).reduce((n, r) => n + r.bytes, 0)
  const offered = rows.filter((r) => r.bucket !== 'keep')
  // Only warned about when it would actually bite: a run that offers nothing under a
  // container needs no permission the app does not already have.
  const blocked = offered.some((r) => r.places.some((p) => NEEDS_ACCESS.has(p.location)))

  return (
    <section className="card">
      <div className="card__head">
        <div>
          <h2 className="card__title">{t('orphans.title')}</h2>
          <p className="card__note">{t('orphans.intro')}</p>
        </div>
        {scanning ? (
          <button type="button" className="btn" onClick={() => void cancelScan()}>
            {t('scan.cancel')}
          </button>
        ) : (
          <button type="button" className="btn btn--primary" onClick={() => void start()}>
            {t(report ? 'scan.rescan' : 'scan.scan')}
          </button>
        )}
      </div>

      {error && <p className="alert">{t(error.key, { detail: error.detail })}</p>}
      {report?.cancelled && <p className="notice">{t('scan.cancelled')}</p>}

      {scanning && <ScanningState bytes={bytes} locale={locale} />}

      {/* Not an error: the feature declines to answer rather than answering badly. */}
      {report && !report.reliable && (
        <p className="notice">
          {t('orphans.unreliable', {
            named: report.appsNamed,
            unnamed: report.appsUnnamed,
          })}
        </p>
      )}

      {report?.reliable && rows.length === 0 && (
        <p className="placeholder">{t('orphans.empty')}</p>
      )}

      <PrivacyNotice needed={blocked} reason={t('orphans.needsAccess')} />

      {rows.length > 0 && (
        <>
          {BUCKETS.map((bucket) => (
            <BucketGroup
              key={bucket}
              bucket={bucket}
              rows={rows.filter((r) => r.bucket === bucket)}
              selected={selected}
              onToggle={toggle}
              onReveal={reveal}
              locale={locale}
            />
          ))}

          <div className="foot">
            <Stats tight>
              <Stat
                label={t('orphans.offered')}
                value={formatBytes(
                  offered.reduce((n, r) => n + r.bytes, 0),
                  locale,
                )}
              />
              <Stat
                label={t('scan.selected')}
                value={formatBytes(selectedBytes, locale)}
                tone="ok"
              />
            </Stats>

            <button
              type="button"
              className="btn btn--primary"
              disabled={selected.size === 0}
              title={selected.size === 0 ? t('scan.reviewHint') : undefined}
              onClick={() => void review()}
            >
              {t('scan.review')}
            </button>
          </div>
          <p className="plan__note">{t('orphans.trashNote')}</p>
        </>
      )}

      {plan && (
        <PlanReview
          plan={plan}
          locale={locale}
          onClose={() => setPlan(null)}
          onExecuted={() => {
            // The measurements on screen describe a disk that no longer exists, and the
            // generation they carry is spent.
            runId.current += 1
            setReport(null)
            setSelected(new Set())
          }}
        />
      )}
    </section>
  )
}

/**
 * One confidence bucket, as a native `<details>`.
 *
 * Native rather than hand-rolled so the disclosure keeps its keyboard behaviour and so
 * find-in-page still reaches a closed group — the protected rows are the ones a
 * suspicious user goes looking for by name.
 */
function BucketGroup({
  bucket,
  rows,
  selected,
  onToggle,
  onReveal,
  locale,
}: {
  bucket: OrphanBucket
  rows: OrphanRow[]
  selected: ReadonlySet<string>
  onToggle: (id: string) => void
  onReveal: (id: string, place: number) => void
  locale: string
}) {
  const { t } = useTranslation()
  if (rows.length === 0) return null
  // Biggest first within a bucket. Across buckets the order is confidence, which is
  // what the grouping is for.
  const sorted = [...rows].sort((a, b) => b.bytes - a.bytes)

  return (
    <details className="group" open={OPEN_BY_DEFAULT[bucket]}>
      <summary className="group__head">
        <span className={`group__title group__title--${bucket}`}>
          {t(`orphans.buckets.${bucket}`)}
        </span>
        <span className="group__count num">{t('orphans.rowCount', { count: rows.length })}</span>
      </summary>
      <p className="card__note">{t(`orphans.bucketNotes.${bucket}`)}</p>
      <ul className="rows">
        {sorted.map((row) => (
          <Row
            key={row.id}
            row={row}
            checked={selected.has(row.id)}
            onToggle={onToggle}
            onReveal={onReveal}
            locale={locale}
          />
        ))}
      </ul>
    </details>
  )
}

function Row({
  row,
  checked,
  onToggle,
  onReveal,
  locale,
}: {
  row: OrphanRow
  checked: boolean
  onToggle: (id: string) => void
  onReveal: (id: string, place: number) => void
  locale: string
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  // `possible` is the bucket where a mistake is plausible, so its checkbox exists only
  // once the row is open. There is no select-all anywhere on this page for the same
  // reason: every tick here is a judgement about one piece of software.
  const tickable = row.bucket === 'likely' || (row.bucket === 'possible' && open)
  const locked = row.bucket === 'keep' || row.bucket === 'unclear'

  return (
    <li className={`row${locked ? ' row--empty' : ''}`}>
      {tickable ? (
        <input
          type="checkbox"
          className="row__check"
          checked={checked}
          onChange={() => onToggle(row.id)}
          aria-label={row.id}
        />
      ) : (
        <span className="row__check row__check--locked" aria-hidden="true">
          {locked ? '🔒' : ''}
        </span>
      )}

      <div className="row__body">
        <span className="row__title num">{row.id}</span>

        {row.veto && <p className="row__desc">{t(`orphans.protected.${row.veto}`)}</p>}

        {row.evidence.length > 0 && (
          <ul className="chips">
            {row.evidence.map((e) => (
              <li key={e} className={`chip${AGAINST.has(e) ? ' chip--against' : ''}`}>
                {t(`orphans.evidence.${e}`)}
              </li>
            ))}
          </ul>
        )}

        <button type="button" className="btn btn--quiet" onClick={() => setOpen(!open)}>
          {t(open ? 'orphans.hidePlaces' : 'orphans.showPlaces')}
        </button>

        {open && (
          <ul className="places">
            {row.places.map((place, i) => (
              <li className="places__row" key={place.path}>
                <span className="places__where">{t(`orphans.where.${place.location}`)}</span>
                <span className="row__path num">{place.path}</span>
                {/* The user's own check on our judgement, and the reason a row we are
                    only fairly sure about can be offered at all. */}
                <button
                  type="button"
                  className="btn btn--quiet"
                  onClick={() => onReveal(row.id, i)}
                >
                  {t('orphans.reveal')}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="row__meta">
        <span className="row__size num">
          {/* A protected row was never measured, so its size is unknown rather than
              zero and must not be printed as a number. */}
          {row.veto ? t('orphans.notMeasured') : formatBytes(row.bytes, locale)}
        </span>
        <span className="row__files">
          {t('orphans.placeCount', { count: row.places.length })}
        </span>
      </div>
    </li>
  )
}
