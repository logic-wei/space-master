import { useCallback, useEffect, useRef, useState } from 'react'

import { cancelScan, toErrorDescriptor, type ErrorDescriptor } from '../lib/ipc'
import type { AdvisoryRow, ScanEvent, ScanItem, ScanReport } from '../lib/types'

export interface ScanState {
  status: 'idle' | 'scanning' | 'done'
  items: ScanItem[]
  advisories: AdvisoryRow[]
  bytes: number
  cancelled: boolean
  /** The backend scan this result belongs to. `preview_clean` requires it. */
  generation: number | null
  error: ErrorDescriptor | null
}

const IDLE: ScanState = {
  status: 'idle',
  items: [],
  advisories: [],
  bytes: 0,
  cancelled: false,
  generation: null,
  error: null,
}

function apply(prev: ScanState, event: ScanEvent): ScanState {
  switch (event.kind) {
    case 'progress':
      return { ...prev, bytes: event.bytes }
    case 'itemDone': {
      const items = [...prev.items, event.item]
      return { ...prev, items, bytes: items.reduce((n, i) => n + i.bytes, 0) }
    }
  }
}

/** A scan command from `lib/ipc`, which streams events and resolves to a report. */
export type Runner = (onEvent: (event: ScanEvent) => void) => Promise<ScanReport>

/**
 * Runs a scan and keeps its partial results.
 *
 * Cancellation is not immediate, so a scan that has been superseded can still emit
 * events. Rather than matching on the backend generation — which is not known until
 * the report resolves, i.e. after those events have already arrived — each run gets
 * a local id, and events from anything but the newest run are dropped.
 *
 * Only one scan runs at a time process-wide: `cancelScan` cancels whatever is going,
 * and the backend hands each run a fresh generation. Two pages scanning at once would
 * leave the older report unable to resolve its ids, so the pages do not offer it.
 */
export function useScan(run: Runner) {
  const runId = useRef(0)
  const [state, setState] = useState<ScanState>(IDLE)

  const start = useCallback(async () => {
    const id = ++runId.current
    setState({ ...IDLE, status: 'scanning' })
    try {
      const report = await run((event) => {
        if (runId.current === id) setState((prev) => apply(prev, event))
      })
      if (runId.current !== id) return
      const group = report.groups[0]
      setState({
        status: 'done',
        items: group ? group.items : [],
        advisories: group ? group.advisories : [],
        bytes: report.bytes,
        cancelled: report.cancelled,
        generation: report.generation,
        error: null,
      })
    } catch (e: unknown) {
      if (runId.current !== id) return
      setState({ ...IDLE, error: toErrorDescriptor(e) })
    }
  }, [run])

  const stop = useCallback(() => {
    void cancelScan()
  }, [])

  // Called once a clean has run: the measurements on screen describe a disk that no
  // longer exists, and the generation they carry is spent. Showing stale sizes next to
  // a finished clean invites acting on them a second time.
  const reset = useCallback(() => {
    runId.current += 1
    setState(IDLE)
  }, [])

  // A scan left running after unmount keeps a thread pool busy walking directories
  // nobody is waiting on.
  useEffect(
    () => () => {
      runId.current += 1
      void cancelScan()
    },
    [],
  )

  return { ...state, start, stop, reset }
}
