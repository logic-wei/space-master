import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  getVolumeInfo,
  toErrorDescriptor,
  unfinishedBatches,
  type ErrorDescriptor,
} from './lib/ipc'
import { formatBytes } from './lib/format'
import type { UnfinishedBatch, VolumeInfo } from './lib/types'
import { LocalePicker } from './components/LocalePicker'
import { DevCachesPage } from './pages/DevCachesPage'
import { HistoryPage } from './pages/HistoryPage'
import { OrphansPage } from './pages/OrphansPage'
import { QuickCleanPage } from './pages/QuickCleanPage'
import { SimulatorsPage } from './pages/SimulatorsPage'
import { XcodePage } from './pages/XcodePage'
import './App.css'

const TABS = ['quick', 'dev', 'xcode', 'simulators', 'orphans', 'history'] as const
type Tab = (typeof TABS)[number]

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
    <main className="shell">
      <header className="shell__head">
        <div>
          <h1 className="shell__title">{t('app.name')}</h1>
          <p className="shell__sub">{t('app.tagline')}</p>
        </div>
        <LocalePicker />
      </header>

      <UnfinishedNotice />

      {error && <p className="alert">{t(error.key, { detail: error.detail })}</p>}
      {!error && !volume && <p className="placeholder">{t('volume.loading')}</p>}
      {volume && <VolumeCard volume={volume} />}

      <nav className="tabs">
        {TABS.map((id) => (
          <button
            key={id}
            type="button"
            className={`tab${tab === id ? ' tab--on' : ''}`}
            aria-current={tab === id}
            onClick={() => setTab(id)}
          >
            {t(`${id}.title`)}
          </button>
        ))}
      </nav>

      {/* Mounted one at a time on purpose. The backend keeps the paths of a single
          scan, so a report left on a hidden page would resolve its ids against
          someone else's scan — and unmounting cancels the walk it left running. */}
      <Page />
    </main>
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

function VolumeCard({ volume }: { volume: VolumeInfo }) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language
  const usedRatio = volume.totalBytes > 0 ? volume.usedBytes / volume.totalBytes : 0

  return (
    <section className="card">
      <div className="card__head">
        <span className="card__title">{t('volume.title')}</span>
        <span className="card__path num">{volume.mountPoint}</span>
      </div>

      <div
        className="bar"
        role="img"
        aria-label={t('volume.usedPercent', { percent: Math.round(usedRatio * 100) })}
      >
        <div className="bar__fill" style={{ width: `${usedRatio * 100}%` }} />
      </div>

      <dl className="stats">
        <Stat
          label={t('volume.available')}
          value={formatBytes(volume.availableBytes, locale)}
          tone="ok"
        />
        <Stat label={t('volume.used')} value={formatBytes(volume.usedBytes, locale)} />
        <Stat label={t('volume.total')} value={formatBytes(volume.totalBytes, locale)} />
      </dl>
    </section>
  )
}

function Stat({ label, value, tone }: { label: string; value: string; tone?: 'ok' }) {
  return (
    <div className="stats__item">
      <dt className="stats__label">{label}</dt>
      <dd className={`stats__value num${tone === 'ok' ? ' stats__value--ok' : ''}`}>
        {value}
      </dd>
    </div>
  )
}
