//! What macOS will and will not let this process touch.
//!
//! Nothing here decides whether a deletion is *allowed* — that is the Guard's job. This
//! answers the different question of whether it can succeed at all, which macOS otherwise
//! only tells us by failing halfway through a batch.

use std::path::Path;

/// Whether this process has Full Disk Access.
///
/// Probed by reading TCC's own database, which is the one file the FDA grant is defined
/// in terms of: no other permission opens it, and holding FDA always does. Reading a
/// byte is enough — the contents are of no interest.
///
/// This matters because `~/Library/Containers` and `~/Library/Group Containers` are
/// protected: another app's container can be listed and measured without FDA but not
/// moved to the Trash, and NSFileManager reports that as a generic error rather than a
/// permission one. Nearly every leftover with real size in it is a container.
pub fn has_full_disk_access(home: &Path) -> bool {
    let tcc = home.join("Library/Application Support/com.apple.TCC/TCC.db");
    std::fs::File::open(tcc).is_ok()
}

/// The same probe against this user's home, for callers with no `home` to hand.
pub fn full_disk_access() -> bool {
    std::env::home_dir().is_some_and(|home| has_full_disk_access(&home))
}

/// Whether this process is running from an `.app` bundle rather than as a bare binary.
///
/// TCC grants attach to the bundle when there is one and to the launching terminal when
/// there is not, so a permission granted in development is a permission granted to the
/// terminal. The two builds genuinely have different capabilities and the UI has to be
/// able to say which one the user is looking at.
pub fn running_as_bundle() -> bool {
    std::env::current_exe().is_ok_and(|exe| {
        exe.ancestors()
            .any(|a| a.extension().is_some_and(|e| e == "app"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_home_without_a_tcc_database_reads_as_no_access() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(!has_full_disk_access(dir.path()));
    }

    /// Not an assertion about the result — it depends on how the tests were launched —
    /// but the probe must answer rather than panic on a real home directory.
    #[test]
    fn the_probe_answers_for_the_real_home() {
        let home = std::env::home_dir().expect("home");
        let _ = has_full_disk_access(&home);
    }
}
