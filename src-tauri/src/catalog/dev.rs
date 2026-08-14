//! Tier B: developer caches offered by the professional mode.
//!
//! A different admission bar from Tier A. These hold no user work either, but
//! rebuilding them costs real time — a cargo registry means recompiling a dependency
//! tree, a model cache means redownloading gigabytes. So they are never pre-selected
//! and never deleted permanently: Guard rule R15 confines permanent deletion to Tier
//! A, which means everything here is recoverable from the Trash by construction.

/// A cache with wording of its own. `rel` is relative to `$HOME`.
pub struct DevEntry {
    /// Stable id and i18n key suffix (`catalog.<id>.*`).
    pub id: &'static str,
    pub rel: &'static str,
}

const fn entry(id: &'static str, rel: &'static str) -> DevEntry {
    DevEntry { id, rel }
}

pub const DEV_ENTRIES: &[DevEntry] = &[
    // Downloaded crate sources and their index. `cargo build` refetches.
    entry("cargoRegistry", ".cargo/registry"),
    entry("pubCache", ".pub-cache"),
    entry("gradleCaches", ".gradle/caches"),
    entry("mavenRepository", ".m2/repository"),
    entry("goModCache", "go/pkg/mod"),
    // Cloned podspec repos, distinct from the download cache in Tier A.
    entry("cocoapodsRepos", ".cocoapods/repos"),
    // Cloned dependency repositories and downloaded binary artifacts. `swift build`
    // and Xcode's package resolution refetch them. Under `Library/Caches` rather than
    // a dotfile, which is why it also turns up as a protected row on the leftovers
    // page — the same directory, offered here and ruled out there.
    entry("swiftpmCache", "Library/Caches/org.swift.swiftpm"),
];

/// XDG cache home. Its children are listed one row each rather than as a single
/// total: it is a shared directory, and on a developer's machine it is usually the
/// largest thing in this group by an order of magnitude.
pub const CACHE_HOME: &str = ".cache";

/// A cache we deliberately do not delete, and the command that does it safely.
///
/// These are not disabled rows — they are not items at all, so no id exists for the
/// frontend to send. Their paths are also outside every root in
/// [`crate::safety::roots`], so the Guard would refuse them even if one appeared.
pub struct Advisory {
    /// Stable id and i18n key suffix (`advisories.<id>.*`).
    pub id: &'static str,
    pub rel: &'static str,
    /// Shown verbatim for the user to run. Never executed by us — we hold no shell
    /// permission, and running package managers on the user's behalf is a different
    /// product with a different risk profile.
    pub command: &'static str,
}

pub const ADVISORIES: &[Advisory] = &[
    // pnpm's store is hard-linked into the `node_modules` of every project on the
    // machine. Deleting it does not free the space those links hold, and it breaks
    // checked-out projects. `pnpm store prune` drops only the unreferenced parts.
    Advisory {
        id: "pnpmStore",
        rel: "Library/pnpm/store",
        command: "pnpm store prune",
    },
];
