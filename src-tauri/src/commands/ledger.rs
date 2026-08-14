use tauri::State;

use crate::model::error::AppResult;
use crate::remove::ledger::{self, BatchSummary};
use crate::state::AppState;

/// Everything this app has deleted, newest first.
///
/// Read-only by design. Recovery for a Trash-mode batch is Finder's "Put Back", which
/// was verified to work on items trashed through NSFileManager — so this is an audit
/// log rather than an undo stack, and there is deliberately no command that reverses a
/// batch. A permanent batch cannot be reversed by anyone.
#[tauri::command]
pub fn ledger_history(state: State<'_, AppState>) -> AppResult<Vec<BatchSummary>> {
    Ok(ledger::history(state.ledger_dir()?)?)
}
