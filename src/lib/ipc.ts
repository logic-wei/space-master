import { Channel, invoke } from '@tauri-apps/api/core'
import type {
  AppErrorPayload,
  CleanOutcome,
  CleanPlan,
  DeleteMode,
  LedgerBatch,
  OrphanReport,
  PrivacyStatus,
  ScanEvent,
  ScanReport,
  SimOutcome,
  SimReport,
  UnfinishedBatch,
  VolumeInfo,
} from './types'

export function getVolumeInfo(): Promise<VolumeInfo> {
  return invoke<VolumeInfo>('get_volume_info')
}

/**
 * Measures the one-click catalog. `onEvent` receives throttled progress while the
 * scan runs; the resolved report is the authoritative result.
 */
export function runQuickScan(onEvent: (event: ScanEvent) => void): Promise<ScanReport> {
  return scan('run_quick_scan', onEvent)
}

/** Measures the developer caches offered by the professional mode. */
export function runDevScan(onEvent: (event: ScanEvent) => void): Promise<ScanReport> {
  return scan('run_dev_scan', onEvent)
}

/** Measures Xcode's device symbols, build output and archives. */
export function runXcodeScan(onEvent: (event: ScanEvent) => void): Promise<ScanReport> {
  return scan('run_xcode_scan', onEvent)
}

function scan(command: string, onEvent: (event: ScanEvent) => void): Promise<ScanReport> {
  const channel = new Channel<ScanEvent>()
  channel.onmessage = onEvent
  return invoke<ScanReport>(command, { channel })
}

export function cancelScan(): Promise<void> {
  return invoke('cancel_scan')
}

/**
 * Builds a reviewable plan from a selection. Takes item ids and the generation they
 * came from, never a path — which is why a bug here cannot widen what gets deleted.
 */
export function previewClean(
  generation: number,
  itemIds: string[],
  mode: DeleteMode,
): Promise<CleanPlan> {
  return invoke<CleanPlan>('preview_clean', { generation, itemIds, mode })
}

/**
 * Carries out a previewed plan. The token is the whole request: the paths come from
 * what the backend vetted, so there is nothing here for a frontend bug to widen.
 *
 * Single-use — a second call with the same token fails as `stalePlan` rather than
 * deleting twice.
 */
export function executeClean(token: number): Promise<CleanOutcome> {
  return invoke<CleanOutcome>('execute_clean', { token })
}

/**
 * Lists the simulators, sizes included. No channel: simctl reports each device's size
 * itself, so there is no walk to report progress on.
 */
export function runSimulatorScan(): Promise<SimReport> {
  return invoke<SimReport>('run_simulator_scan')
}

/**
 * Deletes simulators, irreversibly — there is no Trash for a device.
 *
 * The one command that takes an identifier the frontend could invent. Safe because a
 * udid is not a path: the backend matches its shape and looks it up in a freshly read
 * device list, so anything it does not recognize comes back under `refused`.
 */
export function deleteSimulators(udids: string[]): Promise<SimOutcome> {
  return invoke<SimOutcome>('delete_simulators', { udids })
}

/**
 * Looks for data belonging to software that is no longer installed.
 *
 * Its own report type rather than a `ScanReport`: the page groups by confidence instead
 * of size, and the report has to be able to say the whole feature is switched off.
 */
export function runOrphanScan(onEvent: (event: ScanEvent) => void): Promise<OrphanReport> {
  const channel = new Channel<ScanEvent>()
  channel.onmessage = onEvent
  return invoke<OrphanReport>('run_orphan_scan', { channel })
}

/**
 * Selects one of a row's directories in Finder, so the user can check our judgement
 * before acting on it. Takes an id and an offset into `places`, never a path.
 */
export function revealOrphan(generation: number, id: string, place: number): Promise<void> {
  return invoke('reveal_orphan', { generation, id, place })
}

/** What macOS currently lets this process do. */
export function getPrivacyStatus(): Promise<PrivacyStatus> {
  return invoke<PrivacyStatus>('get_privacy_status')
}

/**
 * Opens the Full Disk Access pane of System Settings. There is no way to request the
 * grant programmatically — the user adds the app and relaunches it.
 */
export function openPrivacySettings(): Promise<void> {
  return invoke('open_privacy_settings')
}

/** Batches with no closing record, i.e. the app stopped partway through a delete. */
export function unfinishedBatches(): Promise<UnfinishedBatch[]> {
  return invoke<UnfinishedBatch[]>('unfinished_batches')
}

/**
 * Every clean this app has performed, newest first.
 *
 * Read-only. There is no counterpart that reverses a batch: Trash-mode deletes are
 * restored with Finder's "Put Back", and a permanent delete cannot be restored at all.
 */
export function ledgerHistory(): Promise<LedgerBatch[]> {
  return invoke<LedgerBatch[]>('ledger_history')
}

function isAppError(e: unknown): e is AppErrorPayload {
  return typeof e === 'object' && e !== null && 'kind' in e && 'detail' in e
}

export interface ErrorDescriptor {
  key: `errors.${AppErrorPayload['kind']}` | 'errors.unknown'
  detail: string
}

/**
 * Normalizes anything thrown by `invoke` into a translation key plus its
 * interpolation params. The backend deliberately sends no prose, so all wording
 * is decided here by the caller's `t()`.
 */
export function toErrorDescriptor(e: unknown): ErrorDescriptor {
  if (isAppError(e)) return { key: `errors.${e.kind}`, detail: e.detail }
  return { key: 'errors.unknown', detail: e instanceof Error ? e.message : String(e) }
}
