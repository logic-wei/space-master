//! Scan results and the progress events sent while producing them.

use std::path::PathBuf;

use serde::Serialize;

use super::item::ScanItem;

/// Identifies a group of items in the UI and in i18n keys (`groups.<id>`).
pub const GROUP_QUICK: &str = "quick";
pub const GROUP_DEV: &str = "dev";
pub const GROUP_XCODE: &str = "xcode";
pub const GROUP_ORPHANS: &str = "orphans";

/// A cache we deliberately offer no button for, and the command that clears it
/// safely. Carried in the report so the page can explain the gap where a row would
/// otherwise be; it holds no id the frontend could send to `preview_clean`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisoryRow {
    /// i18n key suffix (`advisories.<id>.*`).
    pub id: &'static str,
    pub path: PathBuf,
    /// Shown verbatim for the user to copy and run themselves.
    pub command: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanGroup {
    pub id: &'static str,
    pub bytes: u64,
    pub items: Vec<ScanItem>,
    pub advisories: Vec<AdvisoryRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    /// Which scan this is. `preview_clean` requires the same value, so a report the
    /// user left sitting while a newer scan ran cannot be acted on.
    pub generation: u64,
    /// True when the scan stopped early. `bytes` is then a partial sum and must not
    /// be presented as a total.
    pub cancelled: bool,
    pub bytes: u64,
    pub groups: Vec<ScanGroup>,
}

/// Sent over a Tauri channel while a scan runs. The report is the authoritative
/// result; these exist so the UI can show something before it arrives.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ScanEvent {
    /// Running total for the whole scan, plus the item currently being walked.
    /// Rate-limited by [`crate::scan::progress::Throttle`].
    Progress {
        generation: u64,
        group: &'static str,
        item_id: String,
        bytes: u64,
    },
    /// An item is finished and its row can be rendered with final numbers.
    ItemDone {
        generation: u64,
        group: &'static str,
        item: ScanItem,
    },
}
