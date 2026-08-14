//! The only subtrees a candidate may come from.
//!
//! Deny lists answer "what must never go"; this answers the stricter question
//! "what may ever go". A path that is not under one of these roots is refused
//! even if no deny entry matches it, so a bug elsewhere can at worst expose
//! paths inside this table.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    /// The root directory itself may be deleted. Used for directories that are
    /// wholly a cache belonging to one tool.
    Whole,
    /// Only proper descendants may be deleted. Used for directories that
    /// aggregate unrelated data from many apps — `Library/Caches` holds real
    /// databases alongside throwaway files, so it is only ever cleaned by name.
    ChildrenOnly,
}

#[derive(Debug, Clone, Copy)]
pub struct Root {
    /// Path relative to `$HOME`.
    pub rel: &'static str,
    pub kind: RootKind,
}

const fn whole(rel: &'static str) -> Root {
    Root {
        rel,
        kind: RootKind::Whole,
    }
}

const fn children(rel: &'static str) -> Root {
    Root {
        rel,
        kind: RootKind::ChildrenOnly,
    }
}

pub const ROOTS: &[Root] = &[
    // Per-app directories: cleaned by name, never wholesale.
    children("Library/Caches"),
    children("Library/Application Support"),
    children("Library/Containers"),
    children("Library/Group Containers"),
    children("Library/Application Scripts"),
    children("Library/Saved Application State"),
    children("Library/HTTPStorages"),
    children("Library/WebKit"),
    children("Library/Preferences"),
    // Xcode, simulators, DerivedData. `ChildrenOnly` because Library/Developer
    // also holds Xcode/UserData, which is denied outright.
    children("Library/Developer"),
    // Trash is emptied item by item so a partial failure is still progress.
    children(".Trash"),
    // Logs written by applications. Crash reports live elsewhere.
    whole("Library/Logs"),
    // Language and package-manager caches.
    whole(".npm/_cacache"),
    whole(".npm/_logs"),
    whole(".bun/install/cache"),
    whole(".cargo/registry"),
    whole(".gradle/caches"),
    whole(".m2/repository"),
    whole("go/pkg/mod"),
    whole(".pub-cache"),
    whole(".cocoapods/repos"),
    children(".cache"),
    children(".local/state"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Containment {
    /// Candidate is a proper descendant of a root.
    Below,
    /// Candidate is a root, and that root allows being deleted itself.
    IsWholeRoot,
    /// Candidate is a root that only permits deleting its children.
    IsChildrenOnlyRoot,
    Outside,
}

/// Classifies `candidate` against the static roots. Comparison is component-wise,
/// so `Library/Caches-backup` does not match `Library/Caches`.
pub fn classify(candidate: &Path, home: &Path) -> Containment {
    for root in ROOTS {
        let abs = home.join(root.rel);
        if candidate == abs {
            return match root.kind {
                RootKind::Whole => Containment::IsWholeRoot,
                RootKind::ChildrenOnly => Containment::IsChildrenOnlyRoot,
            };
        }
        if candidate.starts_with(&abs) {
            return Containment::Below;
        }
    }

    Containment::Outside
}
