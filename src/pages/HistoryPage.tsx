import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { formatBytes, formatWhen } from '../lib/format'
import { ledgerHistory, toErrorDescriptor, type ErrorDescriptor } from '../lib/ipc'
import type { LedgerBatch } from '../lib/types'

/**
 * What this app has deleted, read back from the ledger.
 *
 * Deliberately has no restore button. Trash-mode deletes are restored from Finder with
 * "Put Back" — verified to work on items trashed through NSFileManager — and a
 * permanent delete cannot be restored by anyone, so a button here could only ever be a
 * false promise. What this page is for is the other question: which run of the app
 * touched a given path, and when.
 */
export function HistoryPage() {
  const { t, i18n } = useTranslation()
  const locale = i18n.language
  const [batches, setBatches] = useState<LedgerBatch[] | null>(null)
  const [error, setError] = useState<ErrorDescriptor | null>(null)

  useEffect(() => {
    let stale = false
    ledgerHistory()
      .then((found) => {
        if (!stale) setBatches(found)
      })
      .catch((e: unknown) => {
        if (!stale) setError(toErrorDescriptor(e))
      })
    return () => {
      stale = true
    }
  }, [])

  return (
    <section className="card">
      <div className="card__head">
        <div>
          <span className="card__title">{t('history.title')}</span>
          <p className="card__note">{t('history.intro')}</p>
        </div>
      </div>

      {error && <p className="alert">{t(error.key, { detail: error.detail })}</p>}
      {batches !== null && batches.length === 0 && (
        <p className="placeholder">{t('history.empty')}</p>
      )}

      <ul className="rows">
        {batches?.map((batch) => (
          <Batch key={batch.batch} batch={batch} locale={locale} />
        ))}
      </ul>
    </section>
  )
}

function Batch({ batch, locale }: { batch: LedgerBatch; locale: string }) {
  const { t } = useTranslation()
  // Paths are rendered only once a batch is opened. A month of use is thousands of
  // them, and the reason to come here is usually one specific run.
  const [open, setOpen] = useState(false)

  return (
    <li className="row">
      <div className="row__body">
        <span className="row__title">{formatWhen(batch.atMs, locale)}</span>
        <p className="row__desc">
          {t(batch.mode === 'trash' ? 'history.modeTrash' : 'history.modePermanent')}
          {' · '}
          {t('outcome.removedCount', { count: batch.removed.length })}
          {batch.failed.length > 0 &&
            ` · ${t('history.failedCount', { count: batch.failed.length })}`}
        </p>
        {/* The batch id is what a ledger line carries, so it is the handle for
            asking about one specific run. */}
        <p className="row__path num">{batch.batch}</p>
        {!batch.finished && <p className="row__desc">{t('history.interrupted')}</p>}

        <button type="button" className="btn btn--quiet" onClick={() => setOpen(!open)}>
          {t(open ? 'history.hidePaths' : 'history.showPaths')}
        </button>

        {open && (
          <ul className="places">
            {batch.removed.map((entry) => (
              <li className="places__row" key={entry.path}>
                <span className="row__path num">{entry.path}</span>
                <span className="row__size num">{formatBytes(entry.bytes, locale)}</span>
              </li>
            ))}
            {batch.failed.map((entry) => (
              <li className="places__row" key={entry.path}>
                <span className="places__where">{t('history.notRemoved')}</span>
                <span className="row__path num">{entry.path}</span>
                {/* macOS's own words. Ours would be a guess at what it meant. */}
                <span className="row__desc">{entry.detail}</span>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="row__meta">
        <span className="row__size num">{formatBytes(batch.bytes, locale)}</span>
      </div>
    </li>
  )
}
