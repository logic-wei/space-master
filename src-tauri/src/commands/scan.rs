use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;

use crate::model::error::{AppError, AppResult};
use crate::model::item::ScannedItem;
use crate::model::report::{
    AdvisoryRow, ScanEvent, ScanGroup, ScanReport, GROUP_DEV, GROUP_QUICK, GROUP_XCODE,
};
use crate::scan::session::ScanHandle;
use crate::scan::{dev_caches, quick, xcode};
use crate::state::AppState;

/// Measures the Tier A catalog, streaming progress over `channel`.
#[tauri::command]
pub async fn run_quick_scan(
    state: State<'_, AppState>,
    channel: Channel<ScanEvent>,
) -> AppResult<ScanReport> {
    let home = state.home.clone();
    run(
        state,
        channel,
        GROUP_QUICK,
        Vec::new(),
        move |handle, emit| quick::scan(handle, &home, emit),
    )
    .await
}

/// Measures the Tier B developer caches. A separate command rather than a second
/// group in one report: the two pages are scanned on their own, and a report holds
/// exactly what the page that asked for it can act on.
#[tauri::command]
pub async fn run_dev_scan(
    state: State<'_, AppState>,
    channel: Channel<ScanEvent>,
) -> AppResult<ScanReport> {
    let home = state.home.clone();
    let advisories = dev_caches::advisories(&home);
    run(
        state,
        channel,
        GROUP_DEV,
        advisories,
        move |handle, emit| dev_caches::scan(handle, &home, emit),
    )
    .await
}

/// Measures Xcode's device-support, DerivedData and archive directories.
#[tauri::command]
pub async fn run_xcode_scan(
    state: State<'_, AppState>,
    channel: Channel<ScanEvent>,
) -> AppResult<ScanReport> {
    let home = state.home.clone();
    run(
        state,
        channel,
        GROUP_XCODE,
        Vec::new(),
        move |handle, emit| xcode::scan(handle, &home, emit),
    )
    .await
}

/// Runs `scan` on a blocking thread and turns its result into a report.
///
/// Blocking is the point: walking a cache directory is minutes of syscalls, and
/// doing that on a tokio worker would stall every other command — including the
/// cancellation this scan is waiting to hear about.
async fn run<F>(
    state: State<'_, AppState>,
    channel: Channel<ScanEvent>,
    group: &'static str,
    advisories: Vec<AdvisoryRow>,
    scan: F,
) -> AppResult<ScanReport>
where
    F: FnOnce(&ScanHandle, &mut dyn FnMut(ScanEvent)) -> Vec<ScannedItem> + Send + 'static,
{
    let scanner = Arc::clone(&state.scanner);
    let index = Arc::clone(&state.index);

    tauri::async_runtime::spawn_blocking(move || {
        let handle = scanner.begin();
        let scanned = scan(&handle, &mut |event| {
            // A closed channel means the window went away mid-scan. The scan is
            // about to be cancelled by whatever replaced it; nothing to report.
            let _ = channel.send(event);
        });

        let bytes = scanned.iter().map(|s| s.item.bytes).sum();
        let items = scanned.iter().map(|s| s.item.clone()).collect();
        index
            .lock()
            .expect("item index lock")
            .replace(handle.generation, &scanned);

        ScanReport {
            generation: handle.generation,
            cancelled: handle.cancel.load(std::sync::atomic::Ordering::Relaxed),
            bytes,
            groups: vec![ScanGroup {
                id: group,
                bytes,
                items,
                advisories,
            }],
        }
    })
    .await
    .map_err(|e| AppError::Scan(e.to_string()))
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    state.scanner.cancel();
}
