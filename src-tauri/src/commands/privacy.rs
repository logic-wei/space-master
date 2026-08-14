use std::process::Command;

use serde::Serialize;
use tauri::State;

use crate::model::error::AppResult;

use crate::safety::privacy;
use crate::state::AppState;

/// What this process is permitted to do, so the UI can warn before a batch fails rather
/// than after.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyStatus {
    /// Without it, app containers can be measured but not moved to the Trash.
    pub full_disk_access: bool,
    /// False in development, where the grant belongs to the terminal instead. The same
    /// build can therefore behave differently from the shipped `.app`, and the UI has to
    /// be able to say so rather than leave the user granting access to the wrong thing.
    pub running_as_bundle: bool,
}

#[tauri::command]
pub fn get_privacy_status(state: State<'_, AppState>) -> PrivacyStatus {
    PrivacyStatus {
        full_disk_access: privacy::has_full_disk_access(&state.home),
        running_as_bundle: privacy::running_as_bundle(),
    }
}

/// The Full Disk Access pane of System Settings.
///
/// There is no API to request the grant — the user has to add the app themselves and
/// relaunch it — so the most an app can do is land them on the right pane. A fixed URL
/// with no input to it, which is why this takes no arguments.
#[tauri::command]
pub fn open_privacy_settings() -> AppResult<()> {
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles")
        .status()?;
    Ok(())
}
