import { useTranslation } from 'react-i18next'

import { formatBytes } from '../lib/format'
import type { VolumeInfo } from '../lib/types'

/**
 * The disk, as a fixture of the sidebar rather than a card competing with the page.
 *
 * Available is the only figure at full size, because it is the number the user opened
 * the app to change; used and total are the context that makes it mean something.
 */
export function VolumeSummary({ volume }: { volume: VolumeInfo }) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language
  const usedRatio = volume.totalBytes > 0 ? volume.usedBytes / volume.totalBytes : 0

  return (
    <section className="vol">
      <span className="vol__label">{t('volume.title')}</span>
      <span className="vol__path num">{volume.mountPoint}</span>

      <div
        className="bar"
        role="img"
        aria-label={t('volume.usedPercent', { percent: Math.round(usedRatio * 100) })}
      >
        <div className="bar__fill" style={{ width: `${usedRatio * 100}%` }} />
      </div>

      <div className="vol__free">
        <span className="vol__label">{t('volume.available')}</span>
        <span className="vol__figure num">{formatBytes(volume.availableBytes, locale)}</span>
      </div>

      <dl className="vol__rest">
        <div className="vol__line">
          <dt>{t('volume.used')}</dt>
          <dd className="num">{formatBytes(volume.usedBytes, locale)}</dd>
        </div>
        <div className="vol__line">
          <dt>{t('volume.total')}</dt>
          <dd className="num">{formatBytes(volume.totalBytes, locale)}</dd>
        </div>
      </dl>
    </section>
  )
}
