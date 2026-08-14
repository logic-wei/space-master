//! Measures Xcode's caches, one row per discovered child.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::catalog::xcode::{XcodeGroup, XCODE_GROUPS};
use crate::fsutil::walk::MeasureCtx;
use crate::model::item::ScannedItem;
use crate::model::report::{ScanEvent, GROUP_XCODE};
use crate::scan::items::{measure_all, Spec};
use crate::scan::session::ScanHandle;

pub fn scan(handle: &ScanHandle, home: &Path, emit: &mut dyn FnMut(ScanEvent)) -> Vec<ScannedItem> {
    let ctx = MeasureCtx {
        pool: Arc::clone(&handle.pool),
        cancel: Arc::clone(&handle.cancel),
    };

    let specs: Vec<Spec> = XCODE_GROUPS.iter().flat_map(|g| rows(home, g)).collect();
    measure_all(handle, GROUP_XCODE, specs, &ctx, emit)
}

/// The rows for one group: every directory `depth` levels below its root.
///
/// The id is the group id plus the row's path relative to that root, which keeps it
/// unique — two archives cut in different date directories can share a name.
fn rows(home: &Path, group: &XcodeGroup) -> Vec<Spec> {
    let root = home.join(group.rel);
    let mut level = vec![root.clone()];
    for _ in 0..group.depth {
        level = level.iter().flat_map(|dir| child_dirs(dir)).collect();
    }

    let mut out: Vec<Spec> = level
        .into_iter()
        .filter_map(|path| {
            let rel = path.strip_prefix(&root).ok()?;
            let id = format!("{}/{}", group.id, rel.to_string_lossy());
            Some(Spec::whole(id, path).with_note(group.id))
        })
        .collect();
    // Directory order is arbitrary and changes between scans. The UI sorts by size;
    // a stable order underneath keeps equal-sized rows from swapping places.
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Immediate subdirectories, or nothing if the directory is absent or unreadable.
///
/// Failing quietly is tolerable here in a way it is not for `~/.Trash`: these are
/// ordinary directories in the user's home with no TCC gate on listing them, so the
/// realistic reason to get nothing back is that Xcode was never installed. There is
/// also no row to hang an issue on — the rows *are* the children.
fn child_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    read.filter_map(Result::ok)
        // Directories only. Xcode leaves plists and `.DS_Store` alongside, and a
        // stray file measured as a row would read as an empty one.
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
        .map(|e| e.path())
        .collect()
}
