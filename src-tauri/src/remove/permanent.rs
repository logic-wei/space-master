//! Unrecoverable deletion. The only file in the tree allowed to call `std::fs`'s
//! removal functions — see `clippy.toml`.

use std::fs;

use crate::safety::guard::{DeleteMode, SafeTarget};

/// Deletes a vetted target outright.
///
/// `fs::remove_dir_all` rather than a hand-rolled recursive walk: since the fix for
/// CVE-2022-21658 it descends with `openat` + `O_NOFOLLOW` and re-checks each
/// component, so a directory swapped for a symlink partway through cannot redirect
/// the deletion outside the tree. A recursion written here would have to reproduce
/// that, and getting it subtly wrong is exactly the failure this app cannot afford.
#[allow(clippy::disallowed_methods)]
pub fn remove(target: &SafeTarget) -> std::io::Result<()> {
    debug_assert!(
        matches!(target.mode(), DeleteMode::Permanent),
        "permanent::remove received a target vetted for the Trash"
    );
    if target.is_dir() {
        fs::remove_dir_all(target.path())
    } else {
        fs::remove_file(target.path())
    }
}
