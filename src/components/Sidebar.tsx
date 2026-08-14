import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { LocalePicker } from './LocalePicker'
import { TABS, type Tab } from '../lib/tabs'

/**
 * The app's only navigation surface.
 *
 * `footer` is a slot rather than a fetch of its own: the volume figures belong to the
 * whole window, so `App` owns that request and hands the result down.
 */
export function Sidebar({
  tab,
  onSelect,
  footer,
}: {
  tab: Tab
  onSelect: (tab: Tab) => void
  footer: ReactNode
}) {
  const { t } = useTranslation()

  return (
    <aside className="side">
      <div className="side__brand">
        <h1 className="side__title">{t('app.name')}</h1>
        <p className="side__sub">{t('app.tagline')}</p>
      </div>

      <nav className="nav" aria-label={t('app.nav')}>
        {TABS.map((id) => (
          <button
            key={id}
            type="button"
            className={`nav__item${tab === id ? ' nav__item--on' : ''}`}
            aria-current={tab === id}
            onClick={() => onSelect(id)}
          >
            {t(`${id}.title`)}
          </button>
        ))}
      </nav>

      <div className="side__spacer" />

      <div className="side__foot">
        {footer}
        <LocalePicker />
      </div>
    </aside>
  )
}
