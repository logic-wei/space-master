export interface VolumeInfo {
  mountPoint: string
  totalBytes: number
  /** total - available: this volume's files plus other volumes in the APFS container. */
  usedBytes: number
  /** Blocks available to an unprivileged user — matches the Avail column of `df`. */
  availableBytes: number
}

/** Mirrors the Rust `AppError` serialization: a stable code plus raw context. */
export interface AppErrorPayload {
  kind: 'invalidPath' | 'io' | 'scan' | 'staleScan' | 'stalePlan'
  detail: string
}

/** Mirrors `fsutil::walk::IssueKind`. */
export type IssueKind =
  | 'permissionDenied'
  | 'symlinkSkipped'
  | 'otherVolumeSkipped'
  | 'readError'
  | 'missing'

export interface ScanIssue {
  path: string
  kind: IssueKind
}

export type ItemScope = 'selfDir' | 'children'

export interface ScanItem {
  /** Also the i18n key suffix, and the only handle the UI has on a path. */
  id: string
  /** Display only. No command accepts a path. */
  path: string
  bytes: number
  files: number
  /**
   * Epoch ms of the most recently touched file inside, or `null` if nothing was
   * measured. The deciding factor for a cache we have no wording for.
   */
  lastUsedMs: number | null
  scope: ItemScope
  /**
   * Key suffix under `notes.*` for wording shared by every row discovered alongside
   * this one, or `null` for a row the catalog names on its own.
   */
  note: string | null
  issues: ScanIssue[]
}

/** A cache we deliberately offer no delete button for. Carries no id to send back. */
export interface AdvisoryRow {
  /** i18n key suffix under `advisories.*`. */
  id: string
  path: string
  /** Shown verbatim for the user to run themselves. */
  command: string
}

export interface ScanGroup {
  id: string
  bytes: number
  items: ScanItem[]
  advisories: AdvisoryRow[]
}

export interface ScanReport {
  generation: number
  /** When true, `bytes` is a partial sum and must not be shown as a total. */
  cancelled: boolean
  bytes: number
  groups: ScanGroup[]
}

/** Progress messages sent while a scan runs, tagged by `kind`. */
export type ScanEvent =
  | { kind: 'progress'; generation: number; group: string; itemId: string; bytes: number }
  | { kind: 'itemDone'; generation: number; group: string; item: ScanItem }

export type DeleteMode = 'trash' | 'permanent'

/** Mirrors `safety::guard::RuleId`. Every value has wording under `rules.*`. */
export type RuleId =
  | 'nulByte'
  | 'notAbsolute'
  | 'nonNormalComponent'
  | 'tooShallow'
  | 'outsideRoots'
  | 'rootNotDeletable'
  | 'protected'
  | 'wouldTakeProtected'
  | 'systemBundle'
  | 'ownAppData'
  | 'missing'
  | 'symlink'
  | 'notFileOrDir'
  | 'otherVolume'
  | 'pathAliased'
  | 'appRunning'
  | 'permanentNotAllowed'
  | 'overlapping'

export interface Rejection {
  path: string
  rule: RuleId
  detail: string | null
}

export interface PlanEntry {
  itemId: string
  path: string
  bytes: number
  isDir: boolean
}

export interface CleanPlan {
  /** Handed back to `executeClean`. Single-use, and the only way to name these paths. */
  token: number
  generation: number
  mode: DeleteMode
  accepted: PlanEntry[]
  /** Shown rather than swallowed: a total below what the scan reported needs a
   *  visible reason. */
  rejected: Rejection[]
  estimatedBytes: number
}

export interface RemovedEntry {
  itemId: string
  path: string
  bytes: number
}

/** Mirrors `model::outcome::FailureKind`. Wording lives under `failures.*`. */
export type FailureKind = 'permissionDenied' | 'inaccessible' | 'failed'

export interface FailureEntry {
  itemId: string
  path: string
  kind: FailureKind
  /** Raw OS text, for diagnostics. Never rendered as prose on its own. */
  detail: string
}

export interface CleanOutcome {
  batch: string
  mode: DeleteMode
  removed: RemovedEntry[]
  /** Refused on the re-check immediately before deleting, not during the preview. */
  rejected: Rejection[]
  failed: FailureEntry[]
  /** Sum of `removed`. In Trash mode the bytes moved rather than disappeared. */
  bytes: number
  /**
   * What the volume gave back, measured either side of the batch. `null` in Trash
   * mode, where nothing is released until the Trash is emptied.
   */
  freedBytes: number | null
}

/**
 * One simulator. Mirrors `simctl::SimDevice`.
 *
 * The `udid` is the only handle, and unlike everywhere else in this app it is sent
 * back to the backend directly rather than via a plan token — it is not a path, and
 * the backend checks its shape and looks it up in the live device list.
 */
export interface SimDevice {
  udid: string
  name: string
  /** e.g. `com.apple.CoreSimulator.SimRuntime.iOS-26-4`. Shown with the prefix cut. */
  runtime: string
  bytes: number
  /** RFC 3339 as simctl printed it, or `null` for a device never booted. */
  lastBootedAt: string | null
  booted: boolean
  /** False once the runtime is gone: the device can never boot again. */
  available: boolean
  path: string
}

export interface SimReport {
  /**
   * False when `xcrun simctl` could not run at all. Distinct from an empty `devices`:
   * "no Xcode command line tools" and "no simulators" are different answers.
   */
  toolsPresent: boolean
  bytes: number
  devices: SimDevice[]
}

/** Mirrors `simctl::SimRefusal`. Wording lives under `simulators.refusals.*`. */
export type SimRefusalReason = 'booted' | 'unknown'

export interface SimOutcome {
  batch: string
  removed: { udid: string; name: string; bytes: number }[]
  refused: { udid: string; reason: SimRefusalReason }[]
  failed: { udid: string; name: string; detail: string }[]
  bytes: number
  /** Always present: deleting a device frees the space immediately. */
  freedBytes: number | null
}

/** What macOS lets this process do. Mirrors `commands::privacy::PrivacyStatus`. */
export interface PrivacyStatus {
  /** Without it, app containers can be measured but not moved to the Trash. */
  fullDiskAccess: boolean
  /** False in development, where the grant belongs to the terminal instead. */
  runningAsBundle: boolean
}

/** Mirrors `scan::orphans::candidates::Location`. Wording under `orphans.where.*`. */
export type OrphanLocation =
  | 'caches'
  | 'preferences'
  | 'applicationSupport'
  | 'containers'
  | 'groupContainers'
  | 'savedState'
  | 'logs'
  | 'webKit'
  | 'httpStorages'
  | 'applicationScripts'

/** Mirrors `scan::orphans::score::Veto`. Wording under `orphans.protected.*`. */
export type OrphanVeto =
  | 'system'
  | 'ownData'
  | 'installed'
  | 'installedFamily'
  | 'launchdJob'
  | 'running'

/** Mirrors `scan::orphans::score::Evidence`. Wording under `orphans.evidence.*`. */
export type OrphanEvidence =
  | 'unusedOver180d'
  | 'unusedOver1y'
  | 'unusedOver2y'
  | 'recentActivity'
  | 'ageUnknown'
  | 'manyLocations'
  | 'onlyPreferences'
  | 'standardId'
  | 'shortId'
  | 'sameVendor'
  | 'holdsDatabase'
  | 'large'
  | 'tiny'

/**
 * How sure we are, and therefore how much work the user has to do to tick the row.
 * Deliberately four coarse names rather than the underlying score, which would read as
 * precision we do not have.
 */
export type OrphanBucket = 'likely' | 'possible' | 'unclear' | 'keep'

/** One directory a leftover was found in. Its index is the handle `revealOrphan` takes. */
export interface OrphanPlace {
  location: OrphanLocation
  path: string
  /** Zero for a protected row, which is never measured. */
  bytes: number
}

export interface OrphanRow {
  /** The bundle identifier, and the id `previewClean` resolves. */
  id: string
  places: OrphanPlace[]
  bytes: number
  bucket: OrphanBucket
  /** Set when the row is not on offer at all. Shown, never hidden. */
  veto: OrphanVeto | null
  evidence: OrphanEvidence[]
}

export interface OrphanReport {
  generation: number
  cancelled: boolean
  /**
   * False when the installed-app list could not be trusted, in which case `rows` is
   * empty. A missing app would turn all of its data into apparent leftovers, so there is
   * no partial answer worth showing.
   */
  reliable: boolean
  appsNamed: number
  appsUnnamed: number
  rows: OrphanRow[]
}

/** A batch whose closing record never arrived: the app stopped mid-delete. */
export interface UnfinishedBatch {
  batch: string
  atMs: number
  mode: DeleteMode
  removed: number
  bytes: number
}

export interface LedgerRemoved {
  itemId: string
  path: string
  bytes: number
}

export interface LedgerFailed {
  path: string
  /** The OS error verbatim, untranslated — see the Rust side for why. */
  detail: string
}

/** One past clean, as the ledger recorded it while it ran. */
export interface LedgerBatch {
  batch: string
  atMs: number
  mode: DeleteMode
  planned: number
  removed: LedgerRemoved[]
  failed: LedgerFailed[]
  bytes: number
  finished: boolean
}
