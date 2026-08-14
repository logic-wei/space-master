import { useTranslation } from 'react-i18next'
import { setLocale, SUPPORTED_LOCALES, type Locale } from '../i18n'

/**
 * Lives in the header for now. It moves into a Settings page in Phase 10, once
 * there is more than one setting to put there.
 */
export function LocalePicker() {
  const { t, i18n } = useTranslation()

  return (
    <label className="locale">
      <span className="locale__label">{t('language.label')}</span>
      <select
        className="locale__select"
        value={i18n.language}
        onChange={(e) => void setLocale(e.target.value as Locale)}
      >
        {SUPPORTED_LOCALES.map((locale) => (
          <option key={locale} value={locale}>
            {t(`language.${locale}`)}
          </option>
        ))}
      </select>
    </label>
  )
}
