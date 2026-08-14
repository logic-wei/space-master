use tauri::State;

use crate::model::error::{AppError, AppResult};
use crate::simctl::{self, SimOutcome, SimReport};
use crate::state::AppState;

/// Every simulator on the machine, with the sizes simctl already reports.
///
/// No generation and no progress channel, unlike the catalog scans: nothing is walked,
/// so there is no partial state to stream and no stale scan to invalidate. The udid is
/// a stable handle that outlives any number of these calls.
#[tauri::command]
pub async fn run_simulator_scan() -> AppResult<SimReport> {
    // Spawning `xcrun` and parsing its output is blocking work, however briefly.
    tauri::async_runtime::spawn_blocking(simctl::list)
        .await
        .map_err(|e| AppError::Scan(e.to_string()))?
}

/// Deletes the named devices. Irreversible — there is no Trash for a simulator.
///
/// Takes udids rather than a plan token, which is the one place this app accepts an
/// identifier the frontend could have made up. It is safe because a udid is not a path:
/// [`simctl::delete`] matches it against a strict hex shape and then against the live
/// device list, so a value that names nothing simply comes back refused.
#[tauri::command]
pub async fn delete_simulators(
    state: State<'_, AppState>,
    udids: Vec<String>,
) -> AppResult<SimOutcome> {
    let ledger_dir = state.ledger_dir()?.clone();
    tauri::async_runtime::spawn_blocking(move || simctl::delete(&udids, &ledger_dir))
        .await
        .map_err(|e| AppError::Scan(e.to_string()))?
}
