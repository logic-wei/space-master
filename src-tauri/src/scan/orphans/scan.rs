//! The orphan scan: enumerate, veto, measure what is left, judge it.
//!
//! Unlike the catalog scans this one emits no per-row event. Those pages render a row the
//! moment it is measured, which works because their rows are independent. Here a row means
//! nothing until it has been placed in a bucket relative to the others, and the page is
//! grouped by bucket — so there is nothing useful to draw before the whole set exists.
//! Progress ticks still go out, because measuring a few dozen container trees takes long
//! enough that silence would look like a hang.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;

use crate::fsutil::walk::MeasureCtx;
use crate::model::item::{ItemScope, ScanItem, ScannedItem, Target};
use crate::model::report::{ScanEvent, GROUP_ORPHANS};
use crate::safety::running_apps::RunningApps;
use crate::scan::orphans::candidates::{self, Candidate};
use crate::scan::orphans::installed::Installed;
use crate::scan::orphans::measure::{footprint, Place};
use crate::scan::orphans::score::{self, Bucket, Evidence, VendorIndex, Veto};
use crate::scan::progress::Throttle;
use crate::scan::session::ScanHandle;

/// One row on the orphans page.
///
/// Not a [`ScanItem`]: an orphan is several directories rather than one, and what decides
/// whether the user ticks it is the evidence rather than the size. The two types are kept
/// apart so neither has fields the other leaves empty.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanRow {
    /// The bundle identifier, which is also what `preview_clean` is given back.
    pub id: String,
    pub places: Vec<Place>,
    pub bytes: u64,
    pub bucket: Bucket,
    /// Set when the row is not on offer at all. Displayed, never hidden.
    pub veto: Option<Veto>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanReport {
    pub generation: u64,
    pub cancelled: bool,
    /// False when the installed-app enumeration could not be trusted, in which case
    /// `rows` is empty. Missing one installed app turns all of its data into apparent
    /// leftovers, so there is no partial answer here worth showing.
    pub reliable: bool,
    /// How many apps the enumeration named, and how many it found but could not name.
    /// Reported so the reason for `reliable: false` is visible rather than asserted.
    pub apps_named: usize,
    pub apps_unnamed: usize,
    pub rows: Vec<OrphanRow>,
}

/// Runs the whole pipeline. `app_data_dir` is this app's own storage, vetoed so a clean
/// cannot take the ledger with it.
pub fn scan(
    handle: &ScanHandle,
    home: &Path,
    app_data_dir: Option<&Path>,
    emit: &mut dyn FnMut(ScanEvent),
) -> (OrphanReport, Vec<ScannedItem>) {
    let running = RunningApps::detect();
    let installed = Installed::detect(home, &running);

    let mut report = OrphanReport {
        generation: handle.generation,
        cancelled: false,
        reliable: installed.reliable(),
        apps_named: installed.named(),
        apps_unnamed: installed.unnamed(),
        rows: Vec::new(),
    };
    if !report.reliable {
        return (report, Vec::new());
    }

    let vendors = VendorIndex::build(&installed);
    let ctx = MeasureCtx {
        pool: Arc::clone(&handle.pool),
        cancel: Arc::clone(&handle.cancel),
    };
    let now = now_secs();

    let mut throttle = Throttle::default();
    let mut total = 0u64;
    let mut scanned = Vec::new();

    for candidate in candidates::collect(home) {
        if handle.cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }

        if let Some(veto) = score::veto(&candidate, &installed, &running, app_data_dir) {
            report.rows.push(vetoed(&candidate, veto));
            continue;
        }

        let f = footprint(&candidate, &ctx);
        let a = score::assess(&candidate, f.bytes, f.holds_database, now, &vendors);

        total += f.bytes;
        if throttle.admit(total) {
            emit(ScanEvent::Progress {
                generation: handle.generation,
                group: GROUP_ORPHANS,
                item_id: candidate.id.clone(),
                bytes: total,
            });
        }

        // Only a selectable row goes into the index. An id the user cannot tick is an id
        // `preview_clean` should refuse to resolve, and the cheapest way to guarantee
        // that is to never record it.
        if a.bucket != Bucket::Keep {
            scanned.push(ScannedItem {
                item: ScanItem {
                    id: candidate.id.clone(),
                    path: f.places[0].path.clone(),
                    bytes: f.bytes,
                    files: 0,
                    last_used_ms: candidate.last_modified().map(|s| s * 1000),
                    scope: ItemScope::SelfDir,
                    note: None,
                    issues: Vec::new(),
                },
                targets: f
                    .places
                    .iter()
                    .map(|p| Target {
                        path: p.path.clone(),
                        bytes: p.bytes,
                    })
                    .collect(),
            });
        }

        report.rows.push(OrphanRow {
            id: candidate.id,
            places: f.places,
            bytes: f.bytes,
            bucket: a.bucket,
            veto: None,
            evidence: a.evidence,
        });
    }

    (report, scanned)
}

/// A row that was refused before being measured. Its size is unknown rather than zero,
/// and the UI must not print a number for it.
fn vetoed(candidate: &Candidate, veto: Veto) -> OrphanRow {
    OrphanRow {
        id: candidate.id.clone(),
        places: candidate
            .hits
            .iter()
            .map(|h| Place {
                location: h.location,
                path: h.path.clone(),
                bytes: 0,
            })
            .collect(),
        bytes: 0,
        bucket: Bucket::Keep,
        veto: Some(veto),
        evidence: Vec::new(),
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
