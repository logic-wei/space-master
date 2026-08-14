import { useTranslation } from 'react-i18next'

import { CleanPanel } from '../components/CleanPanel'
import { useScan } from '../hooks/useScan'
import { runXcodeScan } from '../lib/ipc'

/**
 * The Trash, always. Guard rule R15 enforces it rather than trusting this constant:
 * none of these paths is under the one-click catalog, so `permanent` is refused.
 */
const MODE = 'trash' as const

export function XcodePage() {
  const { t } = useTranslation()
  const scan = useScan(runXcodeScan)

  return (
    <CleanPanel
      scan={scan}
      mode={MODE}
      // Nothing starts ticked, and here that matters more than on any other page: the
      // rows are named by Xcode, and one of them is the only copy of a shipped build's
      // dSYMs. Every tick has to be a decision the user made after reading the row.
      preselect={false}
      text={{
        title: t('xcode.title'),
        intro: t('xcode.intro'),
        empty: t('xcode.empty'),
      }}
    />
  )
}
