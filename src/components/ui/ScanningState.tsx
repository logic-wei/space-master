import { useTranslation } from 'react-i18next'

import { formatBytes } from '../../lib/format'

/**
 * What a scan looks like while it runs.
 *
 * The skeleton is hidden from assistive technology — three grey bars carry nothing to
 * announce — while the running total is a live region, because it is the only evidence
 * that a walk over a large tree is still making progress.
 */
export function ScanningState({ bytes, locale }: { bytes: number; locale: string }) {
  const { t } = useTranslation()

  return (
    <div className="scanning">
      <p className="scanning__line" role="status" aria-live="polite">
        <span className="spinner" aria-hidden="true" />
        {bytes > 0
          ? t('scan.scanningFound', { size: formatBytes(bytes, locale) })
          : t('scan.scanning')}
      </p>

      <ul className="skeleton" aria-hidden="true">
        {[0, 1, 2].map((row) => (
          <li className="skeleton__row" key={row}>
            <div className="skeleton__line" />
            <div className="skeleton__line skeleton__line--sub" />
          </li>
        ))}
      </ul>
    </div>
  )
}
