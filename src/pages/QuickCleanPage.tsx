import { useTranslation } from 'react-i18next'

import { CleanPanel } from '../components/CleanPanel'
import { useScan } from '../hooks/useScan'
import { runQuickScan } from '../lib/ipc'

/**
 * Permanent, so the space is released immediately rather than sitting in the Trash.
 * Defensible only because Guard rule R15 confines permanent deletion to the very
 * catalog this page scans: every entry is rebuilt by its owning tool on next use.
 */
const MODE = 'permanent' as const

export function QuickCleanPage() {
  const { t } = useTranslation()
  const scan = useScan(runQuickScan)

  return (
    <CleanPanel
      scan={scan}
      mode={MODE}
      preselect
      text={{ title: t('quick.title'), intro: t('quick.intro'), empty: t('quick.empty') }}
    />
  )
}
