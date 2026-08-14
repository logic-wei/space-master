import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { PrivacyNotice } from './PrivacyNotice'
import { CopyButton } from './ui/CopyButton'
import { ScanningState } from './ui/ScanningState'
import { Stat, Stats } from './ui/Stat'
import type { ScanState } from '../hooks/useScan'
import { isAdvisoryId, isCatalogId, isNoteId } from '../i18n/catalog'
import { formatBytes, formatLastUsed } from '../lib/format'
import { previewClean, toErrorDescriptor, type ErrorDescriptor } from '../lib/ipc'
import type { AdvisoryRow, CleanPlan, DeleteMode, ScanItem } from '../lib/types'
import { PlanReview } from '../pages/PlanReview'

/**
 * Wording for one screen. Passed in rather than derived from a key prefix so that
 * every i18n key stays a literal in the page that owns it — a prefix would make
 * `grep` unable to answer whether a key is still used.
 */
export interface PanelText {
  title: string
  intro: string
  empty: string
  /** Shown for rows the catalog has no wording for, e.g. children of `~/.cache`. */
  unknownDescription?: string
}

/**
 * The scan-select-review screen, shared by every catalog page.
 *
 * The two policy decisions a page makes are the delete mode and whether anything
 * starts ticked, and they are related: pre-ticking is only defensible where losing a
 * row costs a download. Tier B caches cost a rebuild, so those pages pass
 * `preselect={false}` and let the user choose.
 */
export function CleanPanel({
  scan,
  mode,
  preselect,
  text,
}: {
  scan: ScanState & { start: () => Promise<void>; stop: () => void; reset: () => void }
  mode: DeleteMode
  preselect: boolean
  text: PanelText
}) {
  const { t, i18n } = useTranslation()
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set())
  const [plan, setPlan] = useState<CleanPlan | null>(null)
  const [planError, setPlanError] = useState<ErrorDescriptor | null>(null)

  // A finished scan preselects everything that would actually free space. Rows at
  // zero are left unchecked so the count reflects what the button will do.
  useEffect(() => {
    if (scan.generation === null) return
    setSelected(
      preselect ? new Set(scan.items.filter((i) => i.bytes > 0).map((i) => i.id)) : new Set(),
    )
  }, [scan.generation, scan.items, preselect])

  const toggle = useCallback((id: string) => {
    setSelected((prev) => {
      const next = new Set(prev)
      if (!next.delete(id)) next.add(id)
      return next
    })
  }, [])

  const review = useCallback(async () => {
    if (scan.generation === null) return
    setPlanError(null)
    try {
      setPlan(await previewClean(scan.generation, [...selected], mode))
    } catch (e: unknown) {
      setPlanError(toErrorDescriptor(e))
    }
  }, [scan.generation, selected, mode])

  const locale = i18n.language
  const selectedBytes = scan.items
    .filter((i) => selected.has(i.id))
    .reduce((n, i) => n + i.bytes, 0)
  const selectable = scan.items.filter((i) => i.bytes > 0)
  const allSelected = selectable.length > 0 && selectable.every((i) => selected.has(i.id))
  // Sorted only once the scan is over. Sorting while rows still arrive would have
  // them jump past each other under the pointer.
  const rows =
    scan.status === 'done' ? [...scan.items].sort((a, b) => b.bytes - a.bytes) : scan.items
  const unreadable = scan.items.some((i) =>
    i.issues.some((issue) => issue.kind === 'permissionDenied'),
  )

  return (
    <section className="card">
      <div className="card__head">
        <div>
          <h2 className="card__title">{text.title}</h2>
          <p className="card__note">{text.intro}</p>
        </div>
        {scan.status === 'scanning' ? (
          <button type="button" className="btn" onClick={scan.stop}>
            {t('scan.cancel')}
          </button>
        ) : (
          <button
            type="button"
            className="btn btn--primary"
            onClick={() => void scan.start()}
          >
            {t(scan.status === 'done' ? 'scan.rescan' : 'scan.scan')}
          </button>
        )}
      </div>

      {scan.error && <p className="alert">{t(scan.error.key, { detail: scan.error.detail })}</p>}
      {scan.cancelled && <p className="notice">{t('scan.cancelled')}</p>}

      {/* The `~/.Trash` row is the one that lands here: macOS lets us move things into
          it without Full Disk Access but not list what is already inside. Without the
          grant its size reads as unknown, and a row we cannot measure is a row whose
          deletion we cannot honestly offer. */}
      <PrivacyNotice needed={unreadable} reason={t('scan.needsAccess')} />

      {scan.status === 'scanning' && <ScanningState bytes={scan.bytes} locale={locale} />}

      {scan.status === 'done' && scan.items.length === 0 && (
        <p className="placeholder">{text.empty}</p>
      )}

      {rows.length > 0 && (
        <>
          <ul className="rows">
            {rows.map((item) => (
              <Row
                key={item.id}
                item={item}
                checked={selected.has(item.id)}
                onToggle={toggle}
                locale={locale}
                unknownDescription={text.unknownDescription}
              />
            ))}
          </ul>

          <div className="foot">
            <button
              type="button"
              className="btn btn--quiet"
              onClick={() =>
                setSelected(allSelected ? new Set() : new Set(selectable.map((i) => i.id)))
              }
            >
              {t(allSelected ? 'scan.selectNone' : 'scan.selectAll')}
            </button>

            <Stats tight>
              <Stat label={t('scan.total')} value={formatBytes(scan.bytes, locale)} />
              <Stat
                label={t('scan.selected')}
                value={formatBytes(selectedBytes, locale)}
                tone="ok"
              />
            </Stats>

            <button
              type="button"
              className="btn btn--primary"
              disabled={selected.size === 0 || scan.generation === null}
              title={selected.size === 0 ? t('scan.reviewHint') : undefined}
              onClick={() => void review()}
            >
              {t('scan.review')}
            </button>
          </div>
        </>
      )}

      {scan.advisories.map((advisory) => (
        <Advisory key={advisory.id} advisory={advisory} />
      ))}

      {planError && <p className="alert">{t(planError.key, { detail: planError.detail })}</p>}
      {plan && (
        <PlanReview
          plan={plan}
          locale={locale}
          onClose={() => setPlan(null)}
          onExecuted={scan.reset}
        />
      )}
    </section>
  )
}

function Row({
  item,
  checked,
  onToggle,
  locale,
  unknownDescription,
}: {
  item: ScanItem
  checked: boolean
  onToggle: (id: string) => void
  locale: string
  unknownDescription?: string
}) {
  const { t } = useTranslation()
  // Narrowed to a local rather than tested inline: the key is built by interpolation,
  // and react-i18next only accepts it once the id is known to be one we have wording for.
  const named = isCatalogId(item.id) ? item.id : null
  const note = item.note !== null && isNoteId(item.note) ? item.note : null
  const title = named ? t(`catalog.${named}.title`) : item.id
  const description = named
    ? t(`catalog.${named}.description`)
    : note
      ? t(`notes.${note}`)
      : unknownDescription
  const empty = item.bytes === 0
  // Zero bytes *and* a reported problem is not an empty folder — it is a folder we
  // could not look inside. Rendering that as "already empty" is how a trash holding
  // gigabytes gets dismissed as nothing.
  const unreadable = empty && item.issues.length > 0
  const needsPermission = item.issues.some((i) => i.kind === 'permissionDenied')
  // Symlinks are excluded by design and `du` excludes them too, so counting them as
  // "skipped" would cast doubt on a figure that is exactly right. A model cache is
  // mostly symlinks into its own blob store; the count would be in the hundreds.
  const skipped = item.issues.filter((i) => i.kind !== 'symlinkSkipped').length

  return (
    <li className={`row${empty ? ' row--empty' : ''}`}>
      <input
        type="checkbox"
        className="row__check"
        checked={checked}
        disabled={empty}
        onChange={() => onToggle(item.id)}
        aria-label={title}
      />
      <div className="row__body">
        <span className="row__title">{title}</span>
        {description && <p className="row__desc">{description}</p>}
        <p className="row__path num">{item.path}</p>
        {item.scope === 'children' && <p className="row__desc">{t('scan.scopeChildren')}</p>}
        {needsPermission ? (
          <p className="row__desc">{t('scan.needsPermission')}</p>
        ) : (
          skipped > 0 && <p className="row__desc">{t('scan.issues', { count: skipped })}</p>
        )}
      </div>
      <div className="row__meta">
        <span className="row__size num">
          {unreadable
            ? t('scan.sizeUnknown')
            : empty
              ? t('scan.alreadyEmpty')
              : formatBytes(item.bytes, locale)}
        </span>
        {!empty && (
          <span className="row__files num">{t('scan.fileCount', { count: item.files })}</span>
        )}
        {!empty && item.lastUsedMs !== null && (
          <span className="row__files">
            {t('scan.lastUsed', { when: formatLastUsed(item.lastUsedMs, locale) })}
          </span>
        )}
      </div>
    </li>
  )
}

/**
 * A cache with no checkbox, only the command that clears it safely.
 *
 * Shown rather than omitted: the user knows the store is there, and a page that
 * silently skips the largest thing in the directory looks incomplete rather than
 * careful. There is deliberately no button that runs the command — we hold no shell
 * permission, and running a package manager on the user's behalf is a different
 * product with a different risk profile.
 */
function Advisory({ advisory }: { advisory: AdvisoryRow }) {
  const { t } = useTranslation()
  const id = advisory.id
  if (!isAdvisoryId(id)) return null

  return (
    <div className="notice notice--block">
      <div className="row__body">
        <span className="row__title">{t(`advisories.${id}.title`)}</span>
        <p className="row__desc">{t(`advisories.${id}.description`)}</p>
        <p className="row__path num">{advisory.path}</p>
        <p className="row__path num">{advisory.command}</p>
      </div>
      <CopyButton
        value={advisory.command}
        label={t('scan.copyCommand')}
        copiedLabel={t('scan.copied')}
      />
    </div>
  )
}
