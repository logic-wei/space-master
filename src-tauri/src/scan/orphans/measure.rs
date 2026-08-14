//! Measuring a candidate that survived the veto.
//!
//! Only survivors are measured. Most candidates on a normal machine belong to installed
//! software and are refused without being looked at, and walking a container tree to find
//! that out afterwards would be work spent on an answer we already have.

use std::path::{Path, PathBuf};

use jwalk::WalkDirGeneric;
use serde::Serialize;

use crate::fsutil::walk::{measure, MeasureCtx};
use crate::scan::orphans::candidates::{Candidate, Location};

/// How deep to look for a database before giving up.
///
/// Containers bury them: `Data/Library/Application Support/<app>/store.sqlite` is six
/// levels down. Past that the file is inside something the app itself treats as a nested
/// store, and one more level of searching does not change the conclusion.
const DATABASE_DEPTH: usize = 8;

/// One directory a candidate's data was found in, measured.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub location: Location,
    /// Shown, and revealed in Finder. Travels outward only.
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Footprint {
    pub places: Vec<Place>,
    /// Total across every place, hard links counted once within each.
    pub bytes: u64,
    /// A database was found somewhere that is not a cache. See
    /// [`Location::is_disposable`].
    pub holds_database: bool,
}

pub fn footprint(candidate: &Candidate, ctx: &MeasureCtx) -> Footprint {
    let mut out = Footprint::default();
    for hit in &candidate.hits {
        let measured = measure(&hit.path, ctx, &mut |_| {});
        out.bytes = out.bytes.saturating_add(measured.bytes);
        out.places.push(Place {
            location: hit.location,
            path: hit.path.clone(),
            bytes: measured.bytes,
        });
        if !out.holds_database && !hit.location.is_disposable() {
            out.holds_database = holds_database(&hit.path);
        }
    }
    out
}

/// Whether any name under `root` looks like a database.
///
/// A second traversal of a tree [`measure`] has just walked, which sounds wasteful and is
/// not: the directory entries are in the page cache, no `stat` is needed to read a name,
/// and `any` stops at the first match. Merging it into `measure` would mean a second
/// walker carrying its own copy of the hard-link and volume-boundary accounting, and two
/// copies of that logic drifting apart is a worse trade than one extra pass.
fn holds_database(root: &Path) -> bool {
    WalkDirGeneric::<((), ())>::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .max_depth(DATABASE_DEPTH)
        .into_iter()
        .filter_map(Result::ok)
        .any(|entry| names_a_database(&entry.file_name().to_string_lossy()))
}

/// Matches `.sqlite` anywhere in the name rather than only at the end, because SQLite's
/// own files are `x.sqlite`, `x.sqlite3`, `x.sqlite-wal` and `x.sqlite-shm`, and the
/// journal being present is the same evidence as the database being present.
fn names_a_database(name: &str) -> bool {
    name.contains(".sqlite") || name.ends_with(".db") || name.ends_with(".realm")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_sidecars_count_as_a_database() {
        for name in [
            "store.sqlite",
            "store.sqlite3",
            "store.sqlite-wal",
            "store.sqlite-shm",
            "history.db",
            "objects.realm",
        ] {
            assert!(names_a_database(name), "{name}");
        }
    }

    #[test]
    fn ordinary_names_are_not_databases() {
        for name in [
            "Cache.data",
            "settings.plist",
            "readme.dbf",
            "sqlite",
            "db",
            "index.html",
        ] {
            assert!(!names_a_database(name), "{name}");
        }
    }
}
