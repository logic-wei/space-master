//! Directory measurement.
//!
//! Four things here fail *silently* if written carelessly — none of them produce
//! an error, they just produce a number that looks plausible and is wrong:
//!
//!   1. `jwalk`'s `skip_hidden` defaults to `true`, which would omit `~/.npm`,
//!      `~/.cargo`, `.next`, and `.gradle` — most of what we exist to find.
//!   2. `jwalk` has no `same_file_system` option, so descending into a mounted
//!      volume has to be stopped by comparing `st_dev` ourselves.
//!   3. Hard-linked files get counted once per link unless inodes are tracked.
//!   4. A cancelled scan that keeps walking reports a partial total as if it were
//!      final.
//!
//! Each has a regression test in `tests/measure_vs_du.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use jwalk::{Parallelism, WalkDirGeneric};
use serde::Serialize;

use super::size::{self, LinkLedger};

/// Something skipped during a scan. Reported rather than fatal: one unreadable
/// directory should not discard an otherwise complete measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    PermissionDenied,
    /// Symlinks are neither followed nor counted; their target is either already
    /// in the walk or deliberately out of scope.
    SymlinkSkipped,
    /// A separate volume mounted inside the scanned tree.
    OtherVolumeSkipped,
    ReadError,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanIssue {
    pub path: PathBuf,
    pub kind: IssueKind,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    /// Sum of `st_blocks * 512` over every file, with hard links counted once.
    pub bytes: u64,
    pub files: u64,
    pub dirs: u64,
    /// True when the scan stopped early. The caller must not present `bytes` as a
    /// total in this case.
    pub cancelled: bool,
    /// Epoch milliseconds of the most recently used file found, or `None` if no file
    /// was measured. What makes a 7 GB model cache a decision rather than a number.
    pub last_used_ms: Option<i64>,
    pub issues: Vec<ScanIssue>,
}

/// The later of a file's modification and access time, in epoch milliseconds.
///
/// Files only. Reading a directory updates that directory's own atime, so including
/// directories would have every scan report itself as the last use.
fn last_used_ms(md: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    md.mtime().max(md.atime()).saturating_mul(1000)
}

pub struct MeasureCtx {
    pub pool: Arc<rayon::ThreadPool>,
    pub cancel: Arc<AtomicBool>,
}

/// State attached to each entry by the parallel readdir stage, consumed by the
/// serial accounting loop.
#[derive(Debug, Default)]
pub struct EntryState {
    md: Option<std::fs::Metadata>,
    issue: Option<IssueKind>,
}

pub(crate) fn classify(err: &std::io::Error) -> IssueKind {
    use std::io::ErrorKind;
    match err.kind() {
        ErrorKind::PermissionDenied => IssueKind::PermissionDenied,
        ErrorKind::NotFound => IssueKind::Missing,
        _ => IssueKind::ReadError,
    }
}

/// Measures the on-disk size of `root`, invoking `progress` with the running byte
/// total. Throttling is the caller's job — see [`crate::scan::progress`].
pub fn measure(
    root: &Path,
    ctx: &MeasureCtx,
    progress: &mut dyn FnMut(&Measurement),
) -> Measurement {
    let mut out = Measurement::default();

    let root_md = match std::fs::symlink_metadata(root) {
        Ok(md) => md,
        Err(e) => {
            out.issues.push(ScanIssue {
                path: root.to_path_buf(),
                kind: classify(&e),
            });
            return out;
        }
    };
    let ft = root_md.file_type();
    if ft.is_symlink() {
        out.issues.push(ScanIssue {
            path: root.to_path_buf(),
            kind: IssueKind::SymlinkSkipped,
        });
        return out;
    }
    if !ft.is_dir() {
        out.bytes = size::on_disk(&root_md);
        out.files = 1;
        out.last_used_ms = Some(last_used_ms(&root_md));
        return out;
    }

    // Every entry is compared against the *root's* device rather than $HOME's, so
    // measuring a directory that contains a mount point stops at the boundary
    // whichever volume the root happens to be on.
    let root_dev = {
        use std::os::unix::fs::MetadataExt;
        root_md.dev()
    };
    out.dirs = 1;

    let cancel = Arc::clone(&ctx.cancel);

    let walker = WalkDirGeneric::<((), EntryState)>::new(root)
        // See point 1 in the module comment. This is the single most consequential
        // line in the file.
        .skip_hidden(false)
        .follow_links(false)
        // Ordering costs a sort per directory and buys nothing: we only sum.
        .sort(false)
        .parallelism(Parallelism::RayonExistingPool {
            pool: Arc::clone(&ctx.pool),
            busy_timeout: None,
        })
        .process_read_dir(move |_depth, _path, _read_state, children| {
            if cancel.load(Ordering::Relaxed) {
                for child in children.iter_mut().flatten() {
                    child.read_children = None;
                }
                return;
            }
            for child in children.iter_mut().flatten() {
                if child.file_type.is_symlink() {
                    child.client_state.issue = Some(IssueKind::SymlinkSkipped);
                    child.read_children = None;
                    continue;
                }
                match child.metadata() {
                    Ok(md) => {
                        use std::os::unix::fs::MetadataExt;
                        if md.dev() != root_dev {
                            child.client_state.issue = Some(IssueKind::OtherVolumeSkipped);
                            child.read_children = None;
                            continue;
                        }
                        child.client_state.md = Some(md);
                    }
                    Err(e) => {
                        let kind = e.io_error().map_or(IssueKind::ReadError, classify);
                        child.client_state.issue = Some(kind);
                        child.read_children = None;
                    }
                }
            }
        });

    let mut ledger = LinkLedger::new();
    for result in walker {
        // Checked per entry rather than per directory: a single directory can
        // hold hundreds of thousands of files, and cancellation should feel
        // immediate.
        if ctx.cancel.load(Ordering::Relaxed) {
            out.cancelled = true;
            break;
        }
        let entry = match result {
            Ok(entry) => entry,
            Err(e) => {
                out.issues.push(ScanIssue {
                    path: e.path().unwrap_or(root).to_path_buf(),
                    kind: e.io_error().map_or(IssueKind::ReadError, classify),
                });
                continue;
            }
        };
        // Depth 0 is the root, already accounted for above.
        if entry.depth == 0 {
            continue;
        }
        if let Some(kind) = entry.client_state.issue {
            out.issues.push(ScanIssue {
                path: entry.path(),
                kind,
            });
            continue;
        }
        let Some(md) = &entry.client_state.md else {
            continue;
        };
        if md.is_dir() {
            out.dirs += 1;
        } else {
            out.files += 1;
            let used = last_used_ms(md);
            out.last_used_ms = Some(out.last_used_ms.map_or(used, |seen| seen.max(used)));
        }
        out.bytes += ledger.account(md);
        progress(&out);
    }

    out
}
