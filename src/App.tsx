import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  getVolumeInfo,
  toErrorDescriptor,
  unfinishedBatches,
  type ErrorDescriptor,
} from './lib/ipc'
import type { UnfinishedBatch, VolumeInfo } from './lib/types'
import { Sidebar } from './components/Sidebar'
import type { Tab } from './lib/tabs'
import { VolumeSummary } from './components/VolumeSummary'
import { DevCachesPage } from './pages/DevCachesPage'
import { HistoryPage } from './pages/HistoryPage'
import { OrphansPage } from './pages/OrphansPage'
import { QuickCleanPage } from './pages/QuickCleanPage'
import { SimulatorsPage } from './pages/SimulatorsPage'
import { XcodePage } from './pages/XcodePage'
import './App.css'

const PAGES: Record<Tab, () => React.JSX.Element> = {
  quick: QuickCleanPage,
  dev: DevCachesPage,
  xcode: XcodePage,
  simulators: SimulatorsPage,
  orphans: OrphansPage,
  history: HistoryPage,
}

export default function App() {
  const { t } = useTranslation()
  const [volume, setVolume] = useState<VolumeInfo | null>(null)
  const [error, setError] = useState<ErrorDescriptor | null>(null)
  const [tab, setTab] = useState<Tab>('quick')
  const Page = PAGES[tab]

  useEffect(() => {
    let stale = false
    getVolumeInfo()
      .then((v) => {
        if (!stale) setVolume(v)
      })
      .catch((e: unknown) => {
        if (!stale) setError(toErrorDescriptor(e))
      })
    return () => {
      stale = true
    }
  }, [])

  return (
    <div className="app">
      <Sidebar
        tab={tab}
        onSelect={setTab}
        footer={
          volume ? (
            <VolumeSummary volume={volume} />
          ) : (
            /* A failed statfs is a line in the footer rather than a banner over the
               page: it says nothing about what the user came here to do. */
            <p className={`side__note${error ? ' side__note--error' : ''}`}>
              {error ? t(error.key, { detail: error.detail }) : t('volume.loading')}
            </p>
          )
        }
      />

      <main className="main">
        <UnfinishedNotice />

        {/* Mounted one at a time on purpose. The backend keeps the paths of a single
            scan, so a report left on a hidden page would resolve its ids against someone
            else's scan; the unmount is also what cancels the walk the outgoing page left
            running. The key is for this wrapper, which would otherwise survive the switch
            and never replay `pageIn`. */}
        <div className="page" key={tab}>
          <Page />
        </div>
      </main>
    </div>
  )
}

/**
 * Surfaces batches the ledger never saw close, i.e. the app stopped mid-delete. A
 * failure to read the ledger is swallowed on purpose: it says nothing about the
 * user's disk, and an error banner at every launch would train them to ignore banners.
 */
function UnfinishedNotice() {
  const { t } = useTranslation()
  const [batches, setBatches] = useState<UnfinishedBatch[]>([])

  useEffect(() => {
    let stale = false
    unfinishedBatches()
      .then((found) => {
        if (!stale) setBatches(found)
      })
      .catch(() => {})
    return () => {
      stale = true
    }
  }, [])

  if (batches.length === 0) return null
  const removed = batches.reduce((n, b) => n + b.removed, 0)

  return (
    <p className="notice">
      {t('ledger.unfinished', { count: removed })}
      <button type="button" className="btn btn--quiet" onClick={() => setBatches([])}>
        {t('ledger.dismiss')}
      </button>
    </p>
  )
}
