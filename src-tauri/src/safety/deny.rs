//! The paths that must never be deleted, no matter what asks for it.
//!
//! Entries are matched *component-wise* against the candidate, via
//! `Path::starts_with`. That distinction matters: a byte-prefix comparison would
//! treat `Library/Caches-backup` as being inside `Library/Caches`. `starts_with`
//! compares whole components, so it does not.
//!
//! Matching happens in both directions:
//!   - candidate is at or below a deny entry  -> obviously refused
//!   - candidate is an *ancestor* of a deny entry -> also refused, otherwise
//!     deleting `~/Library` would take `Library/Keychains` with it.

use std::path::{Path, PathBuf};

/// Subtrees under `$HOME` that are off limits. Some are plainly user data
/// (`Documents`); the rest are things whose loss is silent and unrecoverable —
/// credentials, cloud-sync mount points, and tool state that took real time to
/// build.
pub const DENY_HOME: &[&str] = &[
    // User documents and media.
    "Documents",
    "Desktop",
    "Downloads",
    "Pictures",
    "Movies",
    "Music",
    "Public",
    // Apps the user installed into their own home.
    "Applications",
    // Credentials and secrets.
    ".ssh",
    ".gnupg",
    ".aws",
    ".kube",
    ".docker",
    ".password-store",
    ".netrc",
    ".config",
    ".local/share/keyrings",
    "Library/Keychains",
    // Cloud sync. Deleting inside these propagates the deletion to the server,
    // so they are strictly worse than losing a local file.
    "Library/Mobile Documents",
    "Library/CloudStorage",
    "Library/Application Support/CloudDocs",
    // Personal data owned by first-party apps.
    "Library/Mail",
    "Library/Messages",
    "Library/Safari",
    "Library/Photos",
    "Library/Calendars",
    "Library/Reminders",
    "Library/Notes",
    "Library/Accounts",
    "Library/Cookies",
    "Library/Passes",
    "Library/IdentityServices",
    "Library/Sharing",
    "Library/Autosave Information",
    "Library/PersonalizationPortrait",
    "Library/Suggestions",
    "Library/Application Support/AddressBook",
    // iOS device backups. Users assume these survive; they are not a cache.
    "Library/Application Support/MobileSync",
    // Xcode user state: code snippets, key bindings, breakpoints, schemes.
    // It lives under Library/Developer next to genuine caches, so it needs an
    // explicit entry.
    "Library/Developer/Xcode/UserData",
    "Library/Developer/XCTestDevices",
    // Toolchains: technically re-downloadable, but slow enough that removing
    // them behind a "cache cleanup" label would feel like a bug.
    ".rustup/toolchains",
    ".nvm/versions",
    ".pyenv/versions",
    ".rbenv/versions",
    ".sdkman/candidates",
    // Version control and shell state living directly in $HOME.
    ".git",
];

/// Absolute subtrees outside `$HOME`. Nothing we clean lives here, and R12
/// (same-volume check) already rejects most of them, but the list makes the
/// intent explicit and covers volumes that happen to share `st_dev`.
pub const DENY_ABSOLUTE: &[&str] = &[
    "/System",
    "/Library",
    "/private",
    "/usr",
    "/bin",
    "/sbin",
    "/etc",
    "/var",
    "/tmp",
    "/opt",
    "/Volumes",
    "/Network",
    "/Applications",
    "/cores",
];

/// Directories whose immediate child is a bundle identifier.
pub const BUNDLE_ID_PARENTS: &[&str] = &[
    "Library/Containers",
    "Library/Group Containers",
    "Library/Application Scripts",
];

/// Bundle-identifier prefixes belonging to the OS. Their data is never ours to
/// delete, and an "uninstalled app" heuristic will otherwise flag Apple daemons
/// that simply have no `.app` on disk.
pub const PROTECTED_BUNDLE_PREFIXES: &[&str] = &["com.apple.", "group.com.apple."];

/// The deny entry that contains `candidate` (equal to it, or an ancestor of it).
pub fn containing_entry(candidate: &Path, home: &Path) -> Option<PathBuf> {
    DENY_HOME
        .iter()
        .map(|rel| home.join(rel))
        .chain(DENY_ABSOLUTE.iter().map(PathBuf::from))
        .find(|deny| candidate.starts_with(deny))
}

/// The deny entry that `candidate` would take with it, i.e. one that lies
/// *below* the candidate. Without this check, `~/Library` passes every other
/// rule while destroying Keychains.
pub fn descendant_entry(candidate: &Path, home: &Path) -> Option<PathBuf> {
    DENY_HOME
        .iter()
        .map(|rel| home.join(rel))
        .chain(DENY_ABSOLUTE.iter().map(PathBuf::from))
        .find(|deny| deny.starts_with(candidate) && deny != candidate)
}

/// The OS-owned bundle identifier `candidate` belongs to, if it sits under one
/// of the `BUNDLE_ID_PARENTS` directories.
pub fn protected_bundle(candidate: &Path, home: &Path) -> Option<String> {
    let id = bundle_id_component(candidate, home)?;
    PROTECTED_BUNDLE_PREFIXES
        .iter()
        .any(|p| id.starts_with(p))
        .then_some(id)
}

/// The bundle-identifier component of a path under `Library/Containers` and
/// friends — that is, the single component directly beneath the parent
/// directory. Returns `None` for paths that live elsewhere.
pub fn bundle_id_component(candidate: &Path, home: &Path) -> Option<String> {
    for parent in BUNDLE_ID_PARENTS {
        let base = home.join(parent);
        if let Ok(rest) = candidate.strip_prefix(&base) {
            return rest
                .components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().into_owned());
        }
    }
    None
}
