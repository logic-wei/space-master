const UNITS = ['byte', 'kilobyte', 'megabyte', 'gigabyte', 'terabyte'] as const

// Intl.NumberFormat is not cheap to construct, and byte counts get formatted
// per row during a scan.
const cache = new Map<string, Intl.NumberFormat>()

function formatter(locale: string, unit: (typeof UNITS)[number], digits: number) {
  const key = `${locale}|${unit}|${digits}`
  let f = cache.get(key)
  if (!f) {
    f = new Intl.NumberFormat(locale, {
      style: 'unit',
      unit,
      unitDisplay: 'narrow',
      maximumFractionDigits: digits,
      minimumFractionDigits: unit === 'byte' ? 0 : digits,
    })
    cache.set(key, f)
  }
  return f
}

/**
 * Decimal (base-1000) byte formatting, matching Finder and "About This Mac →
 * Storage". Note that `df -h` uses base-1024 and will show smaller numbers for
 * the same byte count.
 *
 * Unit names and decimal separators come from Intl, so nothing here is
 * hardcoded per language.
 */
export function formatBytes(bytes: number, locale: string, digits = 1): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—'

  let value = bytes
  let unit = 0
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000
    unit += 1
  }
  return formatter(locale, UNITS[unit], unit === 0 ? 0 : digits).format(value)
}

const absolute = new Map<string, Intl.DateTimeFormat>()

/**
 * A date and time for the audit log, where "3 months ago" is the wrong answer: what
 * is being asked there is which run of the app a deletion belonged to.
 */
export function formatWhen(ms: number, locale: string): string {
  let f = absolute.get(locale)
  if (!f) {
    f = new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' })
    absolute.set(locale, f)
  }
  return f.format(new Date(ms))
}

const relative = new Map<string, Intl.RelativeTimeFormat>()

/** Days, then months, then years — the granularity at which a cache stops mattering. */
const STEPS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['day', 86_400_000],
  ['month', 2_629_800_000],
  ['year', 31_557_600_000],
]

/**
 * "3 months ago" for an epoch-millisecond timestamp.
 *
 * Coarse on purpose. The question this answers is whether a cache is still in use,
 * and a precise date invites reading the number as a fact about the files rather
 * than what it is: the newest access time we happened to observe, which the scan
 * itself can nudge.
 */
export function formatLastUsed(ms: number, locale: string, now = Date.now()): string {
  let f = relative.get(locale)
  if (!f) {
    f = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' })
    relative.set(locale, f)
  }
  // Clamped at zero: an mtime in the future says the clock moved, not that the
  // cache will be used tomorrow.
  const elapsed = Math.max(0, now - ms)
  let [unit, size] = STEPS[0]
  for (const step of STEPS) {
    if (elapsed >= step[1]) [unit, size] = step
  }
  return f.format(-Math.floor(elapsed / size), unit)
}
