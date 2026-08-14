//! Reading identity out of macOS bundles and launchd job files.
//!
//! Kept separate from the callers because two very different parts of the app depend on
//! it for opposite reasons: the Guard asks "is this app running" (a false negative
//! there means refusing to delete something deletable, which is harmless), while orphan
//! detection asks "is this app installed" (a false negative there means offering to
//! delete an installed app's data, which is not). The second is why [`Identity`]
//! distinguishes "no bundle here" from "a bundle we could not name".

use std::path::Path;

/// What reading a bundle's identifier produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identity {
    Named(String),
    /// There is no `Contents/Info.plist`, so this is not a bundle we were meant to
    /// identify. Not a failure.
    NotABundle,
    /// An `Info.plist` that could not be parsed, or one with no `CFBundleIdentifier`.
    ///
    /// The distinction from `NotABundle` is the whole point: an app we cannot name is
    /// an app whose leftover data looks abandoned. Orphan detection counts these and
    /// switches itself off when there are too many.
    Unnamed,
}

/// The `CFBundleIdentifier` of the bundle at `dir`.
pub fn identify(dir: &Path) -> Identity {
    let info = dir.join("Contents/Info.plist");
    if !info.is_file() {
        return Identity::NotABundle;
    }
    // `::plist` rather than `plist`: this module shares its name with the crate.
    match ::plist::Value::from_file(&info) {
        Ok(value) => value
            .as_dictionary()
            .and_then(|d| d.get("CFBundleIdentifier"))
            .and_then(::plist::Value::as_string)
            // A blank identifier is no identifier. Treating `""` as a name would let
            // it match nothing and count as a success.
            .filter(|id| !id.is_empty())
            .map_or(Identity::Unnamed, |id| Identity::Named(id.to_owned())),
        Err(_) => Identity::Unnamed,
    }
}

/// The `Label` of a launchd job file, which is by convention the bundle id of whatever
/// installed it.
///
/// A job file is how software arranges to run without being open, so a label is
/// evidence the owning software is still set up on this machine even if its `.app` is
/// somewhere we did not look.
pub fn launchd_label(plist: &Path) -> Option<String> {
    let value = ::plist::Value::from_file(plist).ok()?;
    value
        .as_dictionary()?
        .get("Label")?
        .as_string()
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
}
