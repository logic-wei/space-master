use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::State;

use crate::model::error::{AppError, AppResult};
use crate::model::report::ScanEvent;
use crate::scan::orphans::scan::{self, OrphanReport};
use crate::state::AppState;

/// Looks for data belonging to software that is no longer installed.
///
/// Returns its own report rather than a [`ScanReport`](crate::model::report::ScanReport):
/// the page is grouped by confidence, not by size, and the report has to be able to say
/// that the whole feature is switched off because the installed-app enumeration could not
/// be trusted.
///
/// Selectable rows are written into the same item index the catalog scans use, so
/// `preview_clean` needs no orphan-specific path. Rows the user cannot tick are left out
/// of it entirely.
#[tauri::command]
pub async fn run_orphan_scan(
    state: State<'_, AppState>,
    channel: Channel<ScanEvent>,
) -> AppResult<OrphanReport> {
    let home = state.home.clone();
    let app_data_dir = state.app_data_dir.clone();
    let scanner = Arc::clone(&state.scanner);
    let index = Arc::clone(&state.index);
    let reveal = Arc::clone(&state.reveal);

    tauri::async_runtime::spawn_blocking(move || {
        let handle = scanner.begin();
        let (report, scanned) = scan::scan(&handle, &home, app_data_dir.as_deref(), &mut |event| {
            // A closed channel means the window went away mid-scan; the scan that
            // replaces this one will cancel it.
            let _ = channel.send(event);
        });
        index
            .lock()
            .expect("item index lock")
            .replace(handle.generation, &scanned);
        // Every row, protected ones included: a locked row is precisely the one the user
        // wants to look at before believing the verdict.
        let paths = report
            .rows
            .iter()
            .map(|row| {
                (
                    row.id.clone(),
                    row.places.iter().map(|p| p.path.clone()).collect(),
                )
            })
            .collect::<HashMap<_, _>>();
        reveal
            .lock()
            .expect("reveal index lock")
            .replace(handle.generation, paths);
        report
    })
    .await
    .map_err(|e| AppError::Scan(e.to_string()))
}

/// Selects one of a row's directories in Finder.
///
/// The user's own check on our judgement, and the reason the orphans page can offer rows
/// it is only fairly sure about. Takes an id and an offset rather than a path, like every
/// other command, so this cannot become a way to ask the app to open anything at all.
#[tauri::command]
pub async fn reveal_orphan(
    state: State<'_, AppState>,
    generation: u64,
    id: String,
    place: usize,
) -> AppResult<()> {
    let path = state
        .reveal
        .lock()
        .expect("reveal index lock")
        .path(generation, &id, place)?;

    // `-R` selects the entry in its parent window instead of opening it, which for a
    // directory is the difference between showing it and browsing into it. No shell, and
    // the argument is a path this process measured, so there is nothing to quote.
    Command::new("open").arg("-R").arg(&path).status()?;
    Ok(())
}
