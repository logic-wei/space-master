import i18next from 'i18next'
import { initReactI18next } from 'react-i18next'
import { en, type Translation } from './locales/en'
import { zhCN } from './locales/zh-CN'

export const SUPPORTED_LOCALES = ['en', 'zh-CN'] as const
export type Locale = (typeof SUPPORTED_LOCALES)[number]

const FALLBACK: Locale = 'en'
const OVERRIDE_KEY = 'spacemaster.locale'

function isLocale(value: string | null): value is Locale {
  return value !== null && (SUPPORTED_LOCALES as readonly string[]).includes(value)
}

/**
 * In a WKWebView, `navigator.languages` mirrors the macOS preferred-language
 * order, so it is the system locale without needing an extra Tauri plugin.
 */
function detectSystemLocale(): Locale {
  for (const tag of navigator.languages ?? [navigator.language]) {
    if (isLocale(tag)) return tag
    // Match on the primary subtag so `zh-Hans-CN` and `zh-TW` still land on
    // Simplified Chinese rather than falling all the way back to English.
    const primary = tag.split('-')[0]
    const match = SUPPORTED_LOCALES.find((l) => l.split('-')[0] === primary)
    if (match) return match
  }
  return FALLBACK
}

/** A manual choice in Settings wins over the system locale, and persists. */
export function readStoredLocale(): Locale | null {
  const stored = localStorage.getItem(OVERRIDE_KEY)
  return isLocale(stored) ? stored : null
}

export async function setLocale(locale: Locale): Promise<void> {
  localStorage.setItem(OVERRIDE_KEY, locale)
  await i18next.changeLanguage(locale)
  document.documentElement.lang = locale
}

export const initialLocale: Locale = readStoredLocale() ?? detectSystemLocale()

void i18next.use(initReactI18next).init({
  resources: {
    'en': { translation: en },
    'zh-CN': { translation: zhCN },
  },
  lng: initialLocale,
  fallbackLng: FALLBACK,
  // Keys are nested objects, but `.` and `:` still need to work as literal
  // characters in interpolated values, not as separators in our own keys.
  interpolation: { escapeValue: false },
})

document.documentElement.lang = initialLocale

declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'translation'
    resources: { translation: Translation }
  }
}

export default i18next
