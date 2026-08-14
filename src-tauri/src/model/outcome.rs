//! What actually happened when a plan was executed.
//!
//! Three separate lists, not one count. A clean that removed 8 of 10 targets is not
//! "mostly successful" — the user needs to know which two survived and why, and the
//! two reasons are different: the Guard changed its mind, or the OS refused.

use std::path::PathBuf;

use serde::Serialize;

use crate::safety::guard::{DeleteMode, Rejection};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedEntry {
    pub item_id: String,
    pub path: PathBuf,
    pub bytes: u64,
}

/// Stable code for why a deletion failed. The frontend owns the wording.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureKind {
    /// The OS refused. On macOS this usually means Full Disk Access is missing.
    PermissionDenied,
    /// Gone, or unreadable. The trash API does not distinguish the two.
    Inaccessible,
    /// Anything else the OS reported.
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureEntry {
    pub item_id: String,
    pub path: PathBuf,
    pub kind: FailureKind,
    /// The raw OS message. Diagnostics only — never rendered as UI copy.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanOutcome {
    /// Ledger batch id, so a report can be traced back to its records.
    pub batch: String,
    pub mode: DeleteMode,
    pub removed: Vec<RemovedEntry>,
    /// Refused by the Guard on the re-check immediately before deleting. Time passes
    /// between approving a plan and running it: an app may have started, a directory
    /// may have been replaced by a symlink.
    pub rejected: Vec<Rejection>,
    pub failed: Vec<FailureEntry>,
    /// Sum of `removed`. In Trash mode nothing is released yet — the bytes move, they
    /// do not disappear — so the UI must not present this as space regained.
    pub bytes: u64,
    /// What the volume actually gave back, from `statfs` either side of the batch.
    /// `None` in Trash mode, where the answer is always zero and a measurement would
    /// only report other processes' writes as our result.
    pub freed_bytes: Option<u64>,
}
