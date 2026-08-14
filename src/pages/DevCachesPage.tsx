import { useTranslation } from 'react-i18next'

import { CleanPanel } from '../components/CleanPanel'
import { useScan } from '../hooks/useScan'
import { runDevScan } from '../lib/ipc'

/**
 * The Trash, always. These caches hold no work either, but rebuilding one costs a
 * dependency tree recompiled or gigabytes redownloaded — so the deletion has to stay
 * reversible. Guard rule R15 enforces this rather than trusting this constant: the
 * paths are outside the one-click catalog, and `permanent` on any of them is refused.
 */
const MODE = 'trash' as const

export function DevCachesPage() {
  const { t } = useTranslation()
  const scan = useScan(runDevScan)

  return (
    <CleanPanel
      scan={scan}
      mode={MODE}
      // Nothing starts ticked. On the one-click page every row is a download away
      // from being back; here a wrong tick costs an afternoon of rebuilding, so the
      // choice has to be made rather than confirmed.
      preselect={false}
      text={{
        title: t('dev.title'),
        intro: t('dev.intro'),
        empty: t('dev.empty'),
        unknownDescription: t('dev.unknownCache'),
      }}
    />
  )
}
