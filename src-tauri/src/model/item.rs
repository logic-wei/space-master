//! What a scan found, and what a clean would act on.
//!
//! Two views of the same thing, deliberately separated:
//!
//!   - [`ScanItem`] crosses the IPC boundary. It carries an `id` the frontend
//!     sends back, and a `path` for display only.
//!   - [`Target`] stays in the backend. It is the actual list of paths an item
//!     resolves to, resolved during the scan rather than at delete time so the
//!     numbers the user approved and the paths we delete come from one traversal.

use std::path::PathBuf;

use serde::Serialize;

use crate::fsutil::walk::ScanIssue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemScope {
    /// The directory itself is deleted.
    SelfDir,
    /// Each immediate child is deleted and the directory kept. Used where the
    /// directory is OS-managed and its absence would be surprising.
    Children,
}

/// One row in the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanItem {
    /// Stable identifier, also the i18n key suffix (`catalog.<id>.*`). This is the
    /// only handle the frontend has on a path: `preview_clean` takes ids, never
    /// paths.
    pub id: String,
    /// For display and for "Reveal in Finder". Travels outward only — showing the
    /// user what we are about to delete is the whole point, but no command accepts
    /// a path as input.
    pub path: PathBuf,
    /// On-disk size, hard links counted once. An upper bound on what deleting
    /// frees: APFS clones share blocks that every file reports as its own.
    pub bytes: u64,
    pub files: u64,
    /// Epoch milliseconds of the most recently used file inside, or `None` when no
    /// file was measured. For a named cache the size is enough to decide; for one we
    /// have no wording for, this is what separates "stale" from "used yesterday".
    pub last_used_ms: Option<i64>,
    pub scope: ItemScope,
    /// Stable code for wording shared by a whole class of discovered rows
    /// (`notes.<note>`), or `None` for a row the catalog names individually.
    ///
    /// Rows found by listing a directory have no wording of their own — their id is a
    /// device build or a project hash. Without this an Xcode archive would be a bare
    /// name and a size, with nothing saying it holds the dSYMs for a shipped build.
    pub note: Option<&'static str>,
    /// Anything skipped while measuring. Non-fatal; shown so a much-smaller-than-
    /// expected number has a visible explanation.
    pub issues: Vec<ScanIssue>,
}

/// A single path a clean would remove, with the size measured for it.
#[derive(Debug, Clone)]
pub struct Target {
    pub path: PathBuf,
    pub bytes: u64,
}

/// A scanned item together with the paths it resolves to.
#[derive(Debug, Clone)]
pub struct ScannedItem {
    pub item: ScanItem,
    pub targets: Vec<Target>,
}
