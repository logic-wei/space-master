//! The reviewable output of `preview_clean`.
//!
//! Rejections are part of the plan rather than an error: an item the Guard refuses
//! is information the user should see. Dropping it silently would leave them
//! wondering why the total does not match what the scan reported.

use std::path::PathBuf;

use serde::Serialize;

use crate::safety::guard::{DeleteMode, Rejection};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntry {
    pub item_id: String,
    /// Included so the plan can be reviewed and audited. Outbound only.
    pub path: PathBuf,
    pub bytes: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanPlan {
    /// Identifies the plan `execute_clean` must be handed back. Not a secret — its
    /// value is that it names a set of paths this process already vetted, so the
    /// frontend still has no way to describe a target of its own.
    pub token: u64,
    pub generation: u64,
    pub mode: DeleteMode,
    pub accepted: Vec<PlanEntry>,
    pub rejected: Vec<Rejection>,
    /// Sum of `accepted`. An upper bound on space released: `st_blocks` counts
    /// cloned blocks once per file, while releasing them requires dropping every
    /// reference.
    pub estimated_bytes: u64,
}
