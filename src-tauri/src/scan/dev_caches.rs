//! Measures the Tier B named caches, and whatever happens to live in `~/.cache`.
//!
//! Two halves with different characters. The named entries come from a table we
//! wrote and can describe in words. The `~/.cache` children are discovered at scan
//! time and we know nothing about them beyond their size and when they were last
//! touched — which, for the 7 GB model cache that is usually sitting there, is the
//! whole decision.

use std::path::Path;
use std::sync::Arc;

use crate::catalog::dev::{ADVISORIES, CACHE_HOME, DEV_ENTRIES};
use crate::fsutil::walk::MeasureCtx;
use crate::model::item::ScannedItem;
use crate::model::report::{AdvisoryRow, ScanEvent, GROUP_DEV};
use crate::scan::items::{measure_all, Spec};
use crate::scan::session::ScanHandle;

pub fn scan(handle: &ScanHandle, home: &Path, emit: &mut dyn FnMut(ScanEvent)) -> Vec<ScannedItem> {
    let ctx = MeasureCtx {
        pool: Arc::clone(&handle.pool),
        cancel: Arc::clone(&handle.cancel),
    };

    let mut specs: Vec<Spec> = DEV_ENTRIES
        .iter()
        .filter_map(|entry| {
            let path = home.join(entry.rel);
            // A tool that was never installed has no cache to clean. Not following
            // links, so a symlinked cache is left for the Guard to refuse visibly.
            std::fs::symlink_metadata(&path)
                .is_ok()
                .then(|| Spec::whole(entry.id.to_string(), path))
        })
        .collect();
    specs.extend(cache_home(home));

    measure_all(handle, GROUP_DEV, specs, &ctx, emit)
}

/// One row per child of `~/.cache`.
///
/// Listed individually rather than as a single total because it is a shared
/// directory whose occupants are unrelated to each other, and on a developer's
/// machine one of them is routinely larger than the rest of this group put together.
/// A single row would make it an all-or-nothing choice.
///
/// The id is the path relative to `$HOME`. We have no wording for most of these, and
/// the frontend's fallback for an id it does not recognise is to print the id — so
/// the id may as well be something a user recognises.
fn cache_home(home: &Path) -> Vec<Spec> {
    let dir = home.join(CACHE_HOME);
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<Spec> = read
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            Some(Spec::whole(format!("{CACHE_HOME}/{name}"), e.path()))
        })
        .collect();
    // Directory order is arbitrary and changes between scans. The UI sorts by size,
    // but a stable order underneath keeps equal-sized rows from swapping places.
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// The advisories worth showing, which is the ones whose path actually exists.
/// Telling a user how to prune a store they never created is noise that makes the
/// rest of the page look less considered.
pub fn advisories(home: &Path) -> Vec<AdvisoryRow> {
    ADVISORIES
        .iter()
        .filter(|a| home.join(a.rel).exists())
        .map(|a| AdvisoryRow {
            id: a.id,
            path: home.join(a.rel),
            command: a.command,
        })
        .collect()
}
