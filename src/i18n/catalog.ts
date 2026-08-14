import { useCallback } from 'react'
import { useTranslation } from 'react-i18next'

import { en, type AdvisoryId, type CatalogId, type NoteId } from './locales/en'

const CATALOG_IDS = new Set<string>(Object.keys(en.catalog))
const ADVISORY_IDS = new Set<string>(Object.keys(en.advisories))
const NOTE_IDS = new Set<string>(Object.keys(en.notes))

/** Narrows a backend id to one we have wording for. Unknown ids show their raw id. */
export function isCatalogId(id: string): id is CatalogId {
  return CATALOG_IDS.has(id)
}

/**
 * The same for advisories, except an unrecognised one is not rendered at all. An
 * advisory is nothing but its explanation — a bare path next to a shell command,
 * with no wording saying why, is worse than an absent row.
 */
export function isAdvisoryId(id: string): id is AdvisoryId {
  return ADVISORY_IDS.has(id)
}

/** The same for the wording shared by a class of discovered rows. */
export function isNoteId(id: string): id is NoteId {
  return NOTE_IDS.has(id)
}

/**
 * Maps an item id to its display name.
 *
 * Shared rather than local to the scan list on purpose: every screen that names an
 * item has to name it the same way. A plan that says only `.../Application Support/
 * CrashReporter` cannot be checked against a row the user ticked called "Crash report
 * history", which is how the wrong item gets approved.
 */
export function useItemTitle(): (id: string) => string {
  const { t } = useTranslation()
  return useCallback((id: string) => (isCatalogId(id) ? t(`catalog.${id}.title`) : id), [t])
}
