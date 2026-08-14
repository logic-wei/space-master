import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { formatBytes, formatLastUsed } from '../lib/format'
import {
  deleteSimulators,
  runSimulatorScan,
  toErrorDescriptor,
  type ErrorDescriptor,
} from '../lib/ipc'
import type { SimDevice, SimOutcome, SimReport } from '../lib/types'

/**
 * Simulators, which do not go through the scan-plan-execute flow the other pages share.
 *
 * There is no plan step because there would be nothing in it: a udid resolves to no
 * path, so the Guard has no verdict to report and the user has nothing to review beyond
 * the rows already on screen. What replaces it is a confirmation of its own, because
 * this is the only deletion in the app with no Trash behind it.
 */
export function SimulatorsPage() {
  const { t, i18n } = useTranslation()
  const locale = i18n.language
  const [report, setReport] = useState<SimReport | null>(null)
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set())
  const [error, setError] = useState<ErrorDescriptor | null>(null)
  const [confirming, setConfirming] = useState(false)
  const [working, setWorking] = useState(false)
  const [outcome, setOutcome] = useState<SimOutcome | null>(null)

  const load = useCallback(async () => {
    setError(null)
    try {
      setReport(await runSimulatorScan())
    } catch (e: unknown) {
      setError(toErrorDescriptor(e))
    }
  }, [])

  // Listing is one `xcrun` call, so it runs on mount rather than behind a Scan button:
  // there is no long walk to consent to and nothing to cancel.
  useEffect(() => {
    void load()
  }, [load])

  const toggle = (udid: string) =>
    setSelected((prev) => {
      const next = new Set(prev)
      if (!next.delete(udid)) next.add(udid)
      return next
    })

  const run = async () => {
    setWorking(true)
    setError(null)
    try {
      setOutcome(await deleteSimulators([...selected]))
      setSelected(new Set())
      setConfirming(false)
      // The list is now wrong by definition, and a stale row here is a row whose
      // Delete button names a device that no longer exists.
      await load()
    } catch (e: unknown) {
      setError(toErrorDescriptor(e))
    } finally {
      setWorking(false)
    }
  }

  const devices = report?.devices ?? []
  // Biggest first. A booted device still gets a row — hiding it would leave the user
  // hunting for space they can see in Finder but not here.
  const rows = [...devices].sort((a, b) => b.bytes - a.bytes)
  const selectable = devices.filter((d) => !d.booted)
  const allSelected = selectable.length > 0 && selectable.every((d) => selected.has(d.udid))
  const selectedBytes = devices
    .filter((d) => selected.has(d.udid))
    .reduce((n, d) => n + d.bytes, 0)

  return (
    <section className="card">
      <div className="card__head">
        <div>
          <span className="card__title">{t('simulators.title')}</span>
          <p className="card__note">{t('simulators.intro')}</p>
        </div>
        <button type="button" className="btn" onClick={() => void load()} disabled={working}>
          {t('scan.rescan')}
        </button>
      </div>

      {error && <p className="alert">{t(error.key, { detail: error.detail })}</p>}

      {/* An absent toolchain is an answer, not a failure: a `notice`, never an `alert`. */}
      {report && !report.toolsPresent && <p className="notice">{t('simulators.noTools')}</p>}
      {report?.toolsPresent && devices.length === 0 && (
        <p className="placeholder">{t('simulators.empty')}</p>
      )}
      {!report && !error && <p className="placeholder">{t('scan.scanning')}</p>}

      {rows.length > 0 && (
        <>
          <ul className="rows">
            {rows.map((device) => (
              <DeviceRow
                key={device.udid}
                device={device}
                checked={selected.has(device.udid)}
                onToggle={toggle}
                locale={locale}
              />
            ))}
          </ul>

          <div className="foot">
            <button
              type="button"
              className="btn btn--quiet"
              onClick={() =>
                setSelected(allSelected ? new Set() : new Set(selectable.map((d) => d.udid)))
              }
            >
              {t(allSelected ? 'scan.selectNone' : 'scan.selectAll')}
            </button>

            <dl className="stats stats--tight">
              <div className="stats__item">
                <dt className="stats__label">{t('scan.total')}</dt>
                <dd className="stats__value num">{formatBytes(report?.bytes ?? 0, locale)}</dd>
              </div>
              <div className="stats__item">
                <dt className="stats__label">{t('scan.selected')}</dt>
                <dd className="stats__value stats__value--ok num">
                  {formatBytes(selectedBytes, locale)}
                </dd>
              </div>
            </dl>

            <button
              type="button"
              className="btn btn--primary"
              disabled={selected.size === 0 || confirming || working}
              title={selected.size === 0 ? t('simulators.deleteHint') : undefined}
              onClick={() => setConfirming(true)}
            >
              {t('simulators.delete')}
            </button>
          </div>
          {/* Below the totals, because it is about what those totals mean. simctl's
              sizes run low, the opposite direction to the catalog pages. */}
          <p className="plan__note">{t('simulators.sizeNote')}</p>
        </>
      )}

      {confirming && (
        <section className="plan">
          <h3 className="plan__heading">
            {t('simulators.confirmTitle', { count: selected.size })}
          </h3>
          <p className="alert">{t('simulators.confirmBody')}</p>
          <dl className="stats stats--tight">
            <div className="stats__item">
              <dt className="stats__label">{t('scan.selected')}</dt>
              <dd className="stats__value num">{formatBytes(selectedBytes, locale)}</dd>
            </div>
          </dl>
          <div className="foot">
            <button
              type="button"
              className="btn"
              onClick={() => setConfirming(false)}
              disabled={working}
            >
              {t('simulators.cancel')}
            </button>
            <button
              type="button"
              className="btn btn--primary"
              disabled={working}
              onClick={() => void run()}
            >
              {working ? t('simulators.working') : t('simulators.confirm')}
            </button>
          </div>
        </section>
      )}

      {outcome && (
        <Outcome outcome={outcome} locale={locale} onClose={() => setOutcome(null)} />
      )}
    </section>
  )
}

function DeviceRow({
  device,
  checked,
  onToggle,
  locale,
}: {
  device: SimDevice
  checked: boolean
  onToggle: (udid: string) => void
  locale: string
}) {
  const { t } = useTranslation()
  const booted = device.lastBootedAt === null ? null : Date.parse(device.lastBootedAt)

  return (
    <li className={`row${device.booted ? ' row--empty' : ''}`}>
      <input
        type="checkbox"
        className="row__check"
        checked={checked}
        disabled={device.booted}
        onChange={() => onToggle(device.udid)}
        aria-label={device.name}
      />
      <div className="row__body">
        <span className="row__title">{device.name}</span>
        <p className="row__desc">{runtimeLabel(device.runtime)}</p>
        <p className="row__path num">{device.path}</p>
        {device.booted && <p className="row__desc">{t('simulators.running')}</p>}
        {!device.available && <p className="row__desc">{t('simulators.unavailable')}</p>}
      </div>
      <div className="row__meta">
        <span className="row__size num">{formatBytes(device.bytes, locale)}</span>
        <span className="row__files">
          {booted === null || Number.isNaN(booted)
            ? t('simulators.neverBooted')
            : t('scan.lastUsed', { when: formatLastUsed(booted, locale) })}
        </span>
      </div>
    </li>
  )
}

/**
 * `com.apple.CoreSimulator.SimRuntime.iOS-26-4` as `iOS 26.4`.
 *
 * Not translated: it is Apple's own name for the runtime, and the version numbers have
 * to stay recognizable against what Xcode shows.
 */
function runtimeLabel(runtime: string): string {
  const tail = runtime.split('.').pop() ?? runtime
  const [platform, ...version] = tail.split('-')
  return version.length > 0 ? `${platform} ${version.join('.')}` : platform
}

/**
 * What happened. Reported and measured sizes are both shown for the same reason as on
 * the catalog pages: a device's directory can share storage with another, so `df` will
 * disagree with the sum of the rows.
 */
function Outcome({
  outcome,
  locale,
  onClose,
}: {
  outcome: SimOutcome
  locale: string
  onClose: () => void
}) {
  const { t } = useTranslation()

  return (
    <section className="plan">
      <div className="card__head">
        <span className="card__title">{t('simulators.removed')}</span>
        <span className="card__path num">
          {t('outcome.batch')}: {outcome.batch}
        </span>
      </div>

      {outcome.removed.length === 0 ? (
        <p className="placeholder">{t('simulators.nothingRemoved')}</p>
      ) : (
        <ul className="plan__list">
          {outcome.removed.map((entry) => (
            <li className="plan__row" key={entry.udid}>
              <span className="plan__item">
                <span className="plan__name">{entry.name}</span>
                <span className="plan__path num">{entry.udid}</span>
              </span>
              <span className="plan__size num">{formatBytes(entry.bytes, locale)}</span>
            </li>
          ))}
        </ul>
      )}

      {outcome.freedBytes !== null && (
        <>
          <dl className="stats stats--tight">
            <div className="stats__item">
              <dt className="stats__label">{t('outcome.reported')}</dt>
              <dd className="stats__value num">{formatBytes(outcome.bytes, locale)}</dd>
            </div>
            <div className="stats__item">
              <dt className="stats__label">{t('simulators.freed')}</dt>
              <dd className="stats__value stats__value--ok num">
                {formatBytes(outcome.freedBytes, locale)}
              </dd>
            </div>
          </dl>
          <p className="plan__note">{t('simulators.freedNote')}</p>
        </>
      )}

      {outcome.refused.length > 0 && (
        <>
          <h3 className="plan__heading">{t('simulators.refusedTitle')}</h3>
          <ul className="plan__list">
            {outcome.refused.map((entry) => (
              <li className="plan__row plan__row--rejected" key={entry.udid}>
                <span className="plan__path num">{entry.udid}</span>
                <span className="plan__reason">{t(`simulators.refusals.${entry.reason}`)}</span>
              </li>
            ))}
          </ul>
        </>
      )}

      {outcome.failed.length > 0 && (
        <>
          <h3 className="plan__heading">{t('simulators.failedTitle')}</h3>
          <ul className="plan__list">
            {outcome.failed.map((entry) => (
              <li className="plan__row plan__row--rejected" key={entry.udid}>
                <span className="plan__item">
                  <span className="plan__name">{entry.name}</span>
                  <span className="plan__path num">{entry.udid}</span>
                </span>
                {/* simctl's own stderr, shown as diagnostics rather than as prose. */}
                <span className="plan__reason">{entry.detail}</span>
              </li>
            ))}
          </ul>
        </>
      )}

      <div className="foot">
        <button type="button" className="btn btn--primary" onClick={onClose}>
          {t('simulators.done')}
        </button>
      </div>
    </section>
  )
}
