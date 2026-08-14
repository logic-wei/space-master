import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { getPrivacyStatus, openPrivacySettings } from '../lib/ipc'
import type { PrivacyStatus } from '../lib/types'

/**
 * The Full Disk Access banner, plus the only thing an app can actually do about the
 * grant: open the pane where the user makes it themselves.
 *
 * Whether the permission would bite is decided by the caller and passed in as `needed`,
 * because the symptom differs per page — an unreadable folder here, a container that
 * cannot be trashed there. A banner shown when nothing on screen needs the permission
 * teaches the user to ignore banners.
 */
export function PrivacyNotice({ needed, reason }: { needed: boolean; reason: string }) {
  const { t } = useTranslation()
  const [privacy, setPrivacy] = useState<PrivacyStatus | null>(null)

  // Read on mount rather than with each scan: it says nothing about the disk, and the
  // answer only changes when the user changes it in System Settings and relaunches.
  useEffect(() => {
    let stale = false
    getPrivacyStatus()
      .then((status) => {
        if (!stale) setPrivacy(status)
      })
      .catch(() => {})
    return () => {
      stale = true
    }
  }, [])

  if (!needed || privacy === null || privacy.fullDiskAccess) return null

  return (
    <div className="notice notice--block">
      <div className="row__body">
        <p className="row__desc">{reason}</p>
        {/* In development the grant belongs to the terminal, so sending the user off to
            add the app itself would have them grant it to the wrong process. */}
        {!privacy.runningAsBundle && <p className="row__desc">{t('privacy.dev')}</p>}
      </div>
      <button
        type="button"
        className="btn btn--quiet"
        onClick={() => void openPrivacySettings()}
      >
        {t('privacy.openSettings')}
      </button>
    </div>
  )
}
