//! Measures the Tier A catalog.
//!
//! Each entry is resolved to the concrete paths a clean would remove, with a size
//! per path, during this one traversal. Deferring that to delete time would mean
//! the number the user approved and the paths we act on came from two different
//! views of the filesystem.

use std::path::Path;
use std::sync::Arc;

use crate::catalog::quick::QUICK_ENTRIES;
use crate::fsutil::walk::{classify, MeasureCtx, ScanIssue};
use crate::model::item::{ItemScope, ScannedItem};
use crate::model::report::{ScanEvent, GROUP_QUICK};
use crate::scan::items::{measure_all, Spec};
use crate::scan::session::ScanHandle;

pub fn scan(handle: &ScanHandle, home: &Path, emit: &mut dyn FnMut(ScanEvent)) -> Vec<ScannedItem> {
    let ctx = MeasureCtx {
        pool: Arc::clone(&handle.pool),
        cancel: Arc::clone(&handle.cancel),
    };

    let mut specs = Vec::new();
    for entry in QUICK_ENTRIES {
        let path = home.join(entry.rel);
        // A tool that was never installed has no cache directory. That is not an
        // issue to report, just nothing to clean. Checked without following links,
        // so a symlinked cache directory surfaces as a visible refusal later rather
        // than being measured through.
        if std::fs::symlink_metadata(&path).is_err() {
            continue;
        }
        specs.push(match entry.scope {
            ItemScope::SelfDir => Spec::whole(entry.id.to_string(), path),
            ItemScope::Children => {
                let (targets, issue) = children_of(&path);
                Spec {
                    id: entry.id.to_string(),
                    path,
                    scope: ItemScope::Children,
                    targets,
                    note: None,
                    issues: issue.into_iter().collect(),
                }
            }
        });
    }

    measure_all(handle, GROUP_QUICK, specs, &ctx, emit)
}

/// Immediate children of a `Children`-scoped entry, plus the reason the listing
/// failed if it did.
///
/// The issue is not optional bookkeeping. Listing `~/.Trash` requires Full Disk
/// Access, so without it this returns nothing — and a row reporting zero bytes with
/// no issue is indistinguishable from a genuinely empty one. That is how a trash
/// holding gigabytes gets rendered as "already empty".
fn children_of(dir: &Path) -> (Vec<std::path::PathBuf>, Option<ScanIssue>) {
    let read = match std::fs::read_dir(dir) {
        Ok(read) => read,
        Err(e) => {
            return (
                Vec::new(),
                Some(ScanIssue {
                    path: dir.to_path_buf(),
                    kind: classify(&e),
                }),
            );
        }
    };
    let children = read.filter_map(Result::ok).map(|e| e.path()).collect();
    (children, None)
}
