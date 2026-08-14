//! Tier B: Xcode's caches, which are directories full of rows rather than rows.
//!
//! Unlike Tier A and the developer caches, there is nothing here we can name ahead
//! of time: the rows are an iOS build number, a project name plus a hash, an archive
//! named after the minute it was cut. So the catalog names the *directories* and the
//! scan lists their children, one row each.
//!
//! Listing children rather than totalling each directory is the point of the page.
//! `iOS DeviceSupport` is routinely the largest single thing on a developer's disk,
//! and the copy matching the device currently plugged in is the one worth keeping —
//! a single 34 GB row would make that an all-or-nothing choice.
//!
//! Everything here goes to the Trash. Guard rule R15 enforces it rather than trusting
//! this file: none of these paths is under a Tier A entry, so `Permanent` is refused.

/// A directory whose children become rows.
pub struct XcodeGroup {
    /// Stable id. Prefixes the id of every row found here, and names the wording all
    /// of them share (`notes.<id>`).
    pub id: &'static str,
    /// Path relative to `$HOME`.
    pub rel: &'static str,
    /// How many directory levels below `rel` the rows sit.
    pub depth: usize,
}

pub const XCODE_GROUPS: &[XcodeGroup] = &[
    // Symbols copied off each device Xcode has debugged, keyed by OS version and
    // build. Re-extracted from the device on next connect, which is slow but needs
    // nothing we cannot get back.
    XcodeGroup {
        id: "deviceSupport",
        rel: "Library/Developer/Xcode/iOS DeviceSupport",
        depth: 1,
    },
    // Build output, indexes and module caches, one directory per project. Deleting a
    // project's directory costs a clean build, not any source.
    XcodeGroup {
        id: "derivedData",
        rel: "Library/Developer/Xcode/DerivedData",
        depth: 1,
    },
    // Archives are grouped by the date they were cut, so the bundles sit one level
    // deeper. Listed per bundle on purpose: the archive of a build that shipped is
    // the only copy of its dSYMs, and keeping that one while dropping the five test
    // builds from the same afternoon is the decision worth offering.
    XcodeGroup {
        id: "archives",
        rel: "Library/Developer/Xcode/Archives",
        depth: 2,
    },
];
