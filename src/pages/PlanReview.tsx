import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useItemTitle } from '../i18n/catalog'
import { formatBytes } from '../lib/format'
import { executeClean, toErrorDescriptor, type ErrorDescriptor } from '../lib/ipc'
import type { CleanOutcome, CleanPlan, FailureEntry, PlanEntry, Rejection } from '../lib/types'

/**
 * Shows what a clean would do, then carries it out. Both halves of the plan matter:
 * the accepted list is what the user approves, and the rejected list is the only place
 * a Guard refusal becomes visible instead of silently shrinking the total.
 *
 * Once an outcome exists the plan is gone — its token was consumed — so the review
 * controls are replaced rather than left available to click again.
 */
export function PlanReview({
  plan,
  locale,
  onClose,
  onExecuted,
}: {
  plan: CleanPlan
  locale: string
  onClose: () => void
  /** Fired after a clean finishes so the caller can discard its now-stale scan. */
  onExecuted: () => void
}) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  const [running, setRunning] = useState(false)
  const [outcome, setOutcome] = useState<CleanOutcome | null>(null)
  const [error, setError] = useState<ErrorDescriptor | null>(null)

  const copy = async () => {
    await navigator.clipboard.writeText(JSON.stringify(plan, null, 2))
    setCopied(true)
  }

  const run = async () => {
    setRunning(true)
    setError(null)
    try {
      setOutcome(await executeClean(plan.token))
      onExecuted()
    } catch (e: unknown) {
      setError(toErrorDescriptor(e))
    } finally {
      setRunning(false)
    }
  }

  if (outcome) {
    return <OutcomeReport outcome={outcome} locale={locale} onClose={onClose} />
  }

  const trash = plan.mode === 'trash'

  return (
    <section className="plan">
      <div className="card__head">
        <span className="card__title">{t('plan.title')}</span>
        <span className="card__path">
          {t('plan.mode')}: {t(trash ? 'plan.modeTrash' : 'plan.modePermanent')}
        </span>
      </div>

      <p className={trash ? 'notice' : 'alert'}>
        {t(trash ? 'plan.confirmTrash' : 'plan.confirmPermanent')}
      </p>

      <h3 className="plan__heading">
        {t('plan.accepted')} · {t('plan.itemCount', { count: plan.accepted.length })}
      </h3>
      {plan.accepted.length === 0 ? (
        <p className="placeholder">{t('plan.nothingAccepted')}</p>
      ) : (
        <ul className="plan__list">
          {plan.accepted.map((entry) => (
            <AcceptedRow key={entry.path} entry={entry} locale={locale} />
          ))}
        </ul>
      )}

      <h3 className="plan__heading">{t('plan.rejected')}</h3>
      {plan.rejected.length === 0 ? (
        <p className="placeholder">{t('plan.noneRejected')}</p>
      ) : (
        <ul className="plan__list">
          {plan.rejected.map((rejection) => (
            <RejectedRow key={rejection.path} rejection={rejection} />
          ))}
        </ul>
      )}

      <dl className="stats stats--tight">
        <div className="stats__item">
          <dt className="stats__label">{t('plan.estimated')}</dt>
          <dd className="stats__value stats__value--ok num">
            {formatBytes(plan.estimatedBytes, locale)}
          </dd>
        </div>
      </dl>
      <p className="plan__note">{t('plan.estimatedNote')}</p>

      {error && <p className="alert">{t(error.key, { detail: error.detail })}</p>}

      <div className="foot">
        <button type="button" className="btn btn--quiet" onClick={() => void copy()}>
          {t(copied ? 'plan.copied' : 'plan.copy')}
        </button>
        <button type="button" className="btn" onClick={onClose} disabled={running}>
          {t('plan.close')}
        </button>
        <button
          type="button"
          className="btn btn--primary"
          disabled={running || plan.accepted.length === 0}
          onClick={() => void run()}
        >
          {running
            ? t('plan.executing')
            : t(trash ? 'plan.executeTrash' : 'plan.executePermanent')}
        </button>
      </div>
    </section>
  )
}

/**
 * What actually happened. Three lists rather than a success count: an entry the Guard
 * refused at the last moment and one the OS would not delete are different problems.
 */
function OutcomeReport({
  outcome,
  locale,
  onClose,
}: {
  outcome: CleanOutcome
  locale: string
  onClose: () => void
}) {
  const { t } = useTranslation()
  const title = useItemTitle()

  return (
    <section className="plan">
      <div className="card__head">
        <span className="card__title">{t('outcome.title')}</span>
        <span className="card__path num">
          {t('outcome.batch')}: {outcome.batch}
        </span>
      </div>

      <h3 className="plan__heading">
        {t('outcome.removed')} · {t('outcome.removedCount', { count: outcome.removed.length })}
      </h3>
      {outcome.removed.length === 0 ? (
        <p className="placeholder">{t('outcome.nothingRemoved')}</p>
      ) : (
        <>
          <ul className="plan__list">
            {outcome.removed.map((entry) => (
              <li className="plan__row" key={entry.path}>
                <span className="plan__item">
                  <span className="plan__name">{title(entry.itemId)}</span>
                  <span className="plan__path num">{entry.path}</span>
                </span>
                <span className="plan__size num">{formatBytes(entry.bytes, locale)}</span>
              </li>
            ))}
          </ul>
          <p className="notice">
            {t(outcome.mode === 'trash' ? 'outcome.trashNote' : 'outcome.permanentNote')}
          </p>
        </>
      )}

      {/* Both numbers, never just the flattering one. The reported total is what the
          user approved; the measured one is what `df` will show them afterwards. */}
      {outcome.freedBytes !== null && (
        <>
          <dl className="stats stats--tight">
            <div className="stats__item">
              <dt className="stats__label">{t('outcome.reported')}</dt>
              <dd className="stats__value num">{formatBytes(outcome.bytes, locale)}</dd>
            </div>
            <div className="stats__item">
              <dt className="stats__label">{t('outcome.freed')}</dt>
              <dd className="stats__value stats__value--ok num">
                {formatBytes(outcome.freedBytes, locale)}
              </dd>
            </div>
          </dl>
          <p className="plan__note">{t('outcome.freedNote')}</p>
        </>
      )}

      {outcome.failed.length > 0 && (
        <>
          <h3 className="plan__heading">{t('outcome.failed')}</h3>
          <ul className="plan__list">
            {outcome.failed.map((entry) => (
              <FailureRow key={entry.path} entry={entry} />
            ))}
          </ul>
        </>
      )}

      {outcome.rejected.length > 0 && (
        <>
          <h3 className="plan__heading">{t('outcome.rejected')}</h3>
          <p className="plan__note">{t('outcome.rejectedNote')}</p>
          <ul className="plan__list">
            {outcome.rejected.map((rejection) => (
              <RejectedRow key={rejection.path} rejection={rejection} />
            ))}
          </ul>
        </>
      )}

      <div className="foot">
        <button type="button" className="btn btn--primary" onClick={onClose}>
          {t('outcome.done')}
        </button>
      </div>
    </section>
  )
}

function AcceptedRow({ entry, locale }: { entry: PlanEntry; locale: string }) {
  const { t } = useTranslation()
  const title = useItemTitle()

  return (
    <li className="plan__row">
      <span className="plan__item">
        <span className="plan__name">{title(entry.itemId)}</span>
        <span className="plan__path num">{entry.path}</span>
      </span>
      <span className="plan__kind">{t(entry.isDir ? 'plan.directory' : 'plan.file')}</span>
      <span className="plan__size num">{formatBytes(entry.bytes, locale)}</span>
    </li>
  )
}

function RejectedRow({ rejection }: { rejection: Rejection }) {
  const { t } = useTranslation()

  return (
    <li className="plan__row plan__row--rejected">
      <span className="plan__path num">{rejection.path}</span>
      <span className="plan__reason">
        {t(`rules.${rejection.rule}`)}
        {rejection.detail ? ` (${rejection.detail})` : null}
      </span>
    </li>
  )
}

function FailureRow({ entry }: { entry: FailureEntry }) {
  const { t } = useTranslation()
  const title = useItemTitle()

  return (
    <li className="plan__row plan__row--rejected">
      <span className="plan__item">
        <span className="plan__name">{title(entry.itemId)}</span>
        <span className="plan__path num">{entry.path}</span>
      </span>
      {/* `detail` is the raw OS text: shown next to translated wording, never instead of it. */}
      <span className="plan__reason">
        {t(`failures.${entry.kind}`)} ({entry.detail})
      </span>
    </li>
  )
}
