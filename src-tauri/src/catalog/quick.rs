//! Tier A: the one-click clean list.
//!
//! Every entry here is deleted **permanently**, so this table is also the bound
//! on how much damage a permanent delete can do — Guard rule R15 refuses
//! `DeleteMode::Permanent` for anything not at or below one of these paths.
//!
//! Admission criteria, all of which must hold:
//!   1. The owning tool recreates the data automatically on next use.
//!   2. Losing it costs bandwidth or CPU, never user work.
//!   3. The path is specific to one tool, not a shared directory.
//!
//! Things deliberately *not* here, because they fail (2): `~/.cargo/registry`
//! and Xcode `DerivedData` (slow rebuilds), pnpm's store (hard-linked into
//! existing `node_modules`, so removing it breaks checked-out projects). Those
//! belong to the professional mode, where deletion is recoverable.

use crate::model::item::ItemScope;

#[derive(Debug, Clone, Copy)]
pub struct QuickEntry {
    /// Stable identifier. Doubles as the i18n key suffix (`catalog.<id>.*`) and
    /// as the id the frontend sends back in `preview_clean`, so it must not
    /// change once shipped.
    pub id: &'static str,
    /// Path relative to `$HOME`.
    pub rel: &'static str,
    pub scope: ItemScope,
}

const fn self_dir(id: &'static str, rel: &'static str) -> QuickEntry {
    QuickEntry {
        id,
        rel,
        scope: ItemScope::SelfDir,
    }
}

const fn children(id: &'static str, rel: &'static str) -> QuickEntry {
    QuickEntry {
        id,
        rel,
        scope: ItemScope::Children,
    }
}

pub const QUICK_ENTRIES: &[QuickEntry] = &[
    // npm's content-addressed cache; refetched on demand.
    self_dir("npmCacache", ".npm/_cacache"),
    self_dir("npmLogs", ".npm/_logs"),
    // Downloaded bottle archives. `brew` redownloads them.
    self_dir("homebrewCache", "Library/Caches/Homebrew"),
    self_dir("bunInstallCache", ".bun/install/cache"),
    self_dir("pipCache", "Library/Caches/pip"),
    self_dir("yarnCacheLibrary", "Library/Caches/yarn"),
    self_dir("yarnCacheDot", ".cache/yarn"),
    self_dir("cocoapodsCache", "Library/Caches/CocoaPods"),
    // Xcode's own cache. Explicitly *not* DerivedData.
    self_dir("xcodeAppCache", "Library/Caches/com.apple.dt.Xcode"),
    // Application logs. Crash reports are a separate entry so the UI can
    // describe them differently.
    self_dir("appLogs", "Library/Logs"),
    self_dir("crashReports", "Library/Application Support/CrashReporter"),
    // Emptying the trash is what the user already asked the OS to do.
    children("trash", ".Trash"),
];

/// Whether `rel` (a path relative to `$HOME`) is at or below a Tier A entry.
/// Backs Guard rule R15.
pub fn covers(candidate: &std::path::Path, home: &std::path::Path) -> bool {
    QUICK_ENTRIES
        .iter()
        .any(|e| candidate.starts_with(home.join(e.rel)))
}
