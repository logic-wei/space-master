//! Turning directory listings under `~/Library` into named candidates.
//!
//! The one rule that matters here: **a name we cannot read as a bundle identifier is
//! dropped, not kept.** `~/Library/Application Support` holds `Code`, `calibre`,
//! `AliLang` and dozens more like them, and `Application Scripts` is 848 entries of
//! which most are bare UUIDs. None of those can be compared against the installed set,
//! so every one of them would look abandoned forever. "Cannot attribute" must never
//! become "orphaned", so they never reach the scoring layer at all.
//!
//! The cost is real — VS Code's `Application Support/Code` is invisible to this feature
//! even after you uninstall VS Code — and it is the right trade. The alternative is a
//! page that offers to delete the data of software that is plainly installed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where a candidate turned up. Wording lives in the frontend under `orphans.where.*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Location {
    Caches,
    Preferences,
    ApplicationSupport,
    Containers,
    GroupContainers,
    SavedState,
    Logs,
    WebKit,
    HttpStorages,
    ApplicationScripts,
}

impl Location {
    /// Path relative to `~/Library`.
    pub fn dir(self) -> &'static str {
        match self {
            Location::Caches => "Caches",
            Location::Preferences => "Preferences",
            Location::ApplicationSupport => "Application Support",
            Location::Containers => "Containers",
            Location::GroupContainers => "Group Containers",
            Location::SavedState => "Saved Application State",
            Location::Logs => "Logs",
            Location::WebKit => "WebKit",
            Location::HttpStorages => "HTTPStorages",
            Location::ApplicationScripts => "Application Scripts",
        }
    }

    /// Whether this is a directory whose contents software is built to be able to lose.
    ///
    /// Decides how much a database found inside is worth as an argument against deleting.
    /// A `.sqlite` under `Caches` is a cache by construction — the app rebuilds it. The
    /// same file under `Application Support` or a `Container` may be the only copy of
    /// something the user typed.
    pub fn is_disposable(self) -> bool {
        matches!(
            self,
            Location::Caches
                | Location::Logs
                | Location::WebKit
                | Location::HttpStorages
                | Location::SavedState
        )
    }
}

pub const LOCATIONS: [Location; 10] = [
    Location::Caches,
    Location::Preferences,
    Location::ApplicationSupport,
    Location::Containers,
    Location::GroupContainers,
    Location::SavedState,
    Location::Logs,
    Location::WebKit,
    Location::HttpStorages,
    Location::ApplicationScripts,
];

/// Suffixes that are part of the *filename* rather than part of the identifier.
///
/// Deliberately closed and short. The tempting generalisation — strip whatever follows
/// the last dot — is wrong: `.extension`, `.helper`, `.widget`, `.diagnostic` and
/// `.controls` all appear here as genuine trailing segments of real bundle ids (94
/// entries end in `.extension` on this machine), and `xyz.chatboxapp.app` is an app, not
/// a stray `.app` bundle.
const FILE_SUFFIXES: [&str; 3] = [".plist", ".savedState", ".binarycookies"];

/// One place a candidate's data was found.
#[derive(Debug, Clone)]
pub struct Hit {
    pub location: Location,
    pub path: PathBuf,
    /// The entry name as it is on disk, before the TeamID prefix and file suffix came
    /// off. Shown in the UI so the row can be recognised in Finder.
    pub raw_name: String,
    /// Seconds since the epoch, or `None` if the entry could not be stat'd. This is the
    /// entry's own mtime, not a walk of everything below it: for a cache directory the
    /// mtime moves whenever a file is added or removed, which is the signal we want, and
    /// it costs one `lstat` instead of a subtree.
    pub modified: Option<i64>,
}

/// Everything found for one identifier.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    pub hits: Vec<Hit>,
}

impl Candidate {
    /// The most recent mtime across every location. `None` only if nothing could be
    /// stat'd, which the scoring layer must treat as "unknown", never as "old".
    pub fn last_modified(&self) -> Option<i64> {
        self.hits.iter().filter_map(|h| h.modified).max()
    }
}

/// Every attributable candidate under `~/Library`, grouped by identifier.
///
/// Sorted by identifier, and sizes are not measured here: most of these are about to be
/// vetoed for being installed, and walking a container tree to find that out afterwards
/// would be wasted work. Whoever survives scoring gets measured.
pub fn collect(home: &Path) -> Vec<Candidate> {
    let library = home.join("Library");
    let mut by_id: BTreeMap<String, Vec<Hit>> = BTreeMap::new();

    for location in LOCATIONS {
        let dir = library.join(location.dir());
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.filter_map(Result::ok) {
            let raw_name = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = parse_id(&raw_name) else {
                continue;
            };
            if !holds_app_data(&entry, &raw_name) {
                continue;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            by_id.entry(id).or_default().push(Hit {
                location,
                path: entry.path(),
                raw_name,
                modified,
            });
        }
    }

    by_id
        .into_iter()
        .map(|(id, hits)| Candidate { id, hits })
        .collect()
}

/// Whether the entry is the kind of thing an app keeps its data in.
///
/// A directory always is. A plain file only counts when its name ended in a suffix we
/// recognise, which is the difference between `com.foo.Bar.plist` and the loose
/// `DiscRecording.log`, `PhotosSearch.aapbz` and `default.store-wal` sitting in these same
/// directories. Those parse as two-segment identifiers and belong to nothing; listing them
/// would pad the page with rows no one can judge.
///
/// A symlink is neither, and falls out here — which is also what the Guard would decide
/// about it later.
fn holds_app_data(entry: &std::fs::DirEntry, raw_name: &str) -> bool {
    match entry.file_type() {
        Ok(t) if t.is_dir() => true,
        Ok(t) if t.is_file() => FILE_SUFFIXES.iter().any(|s| raw_name.ends_with(s)),
        _ => false,
    }
}

/// The bundle identifier a directory or file name refers to, if it refers to one.
///
/// Strips in the order the names are actually built: the file suffix last, so it comes
/// off first, then the TeamID prefix, then the group marker.
/// `243LU875E5.groups.com.apple.podcasts` needs all three.
pub fn parse_id(raw: &str) -> Option<String> {
    let mut name = raw;
    for suffix in FILE_SUFFIXES {
        if let Some(stem) = name.strip_suffix(suffix) {
            name = stem;
            break;
        }
    }
    name = strip_team_id(name);
    name = strip_group_marker(name);

    is_bundle_id(name).then(|| name.to_owned())
}

/// Removes the marker that says "this is a shared container rather than an app's own".
///
/// Not cosmetic. `~/Library/Group Containers` is 121 entries of which ninety-odd are
/// `group.com.apple.*`, and without this they read as ids whose first segment is `group`
/// — matching no installed app, and so looking abandoned. Normalising them to the owning
/// app's id is what lets the `com.apple.*` veto and the installed-set lookup see them for
/// what they are.
const GROUP_MARKERS: [&str; 3] = ["systemgroup.", "groups.", "group."];

fn strip_group_marker(name: &str) -> &str {
    for marker in GROUP_MARKERS {
        if let Some(rest) = name.strip_prefix(marker) {
            return rest;
        }
    }
    name
}

/// Removes a leading Apple Developer Team ID, which is exactly ten uppercase
/// alphanumerics.
///
/// The digit requirement is what keeps this from eating a real leading segment: a
/// reverse-DNS name starts with something like `com` or `org`, and no ten-character
/// all-caps TLD with a digit in it exists. Without it, an id whose first segment happened
/// to be ten capital letters would silently lose it and stop matching anything installed.
fn strip_team_id(name: &str) -> &str {
    let Some((head, rest)) = name.split_once('.') else {
        return name;
    };
    let is_team_id = head.len() == 10
        && head
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && head.bytes().any(|b| b.is_ascii_digit());
    if is_team_id {
        rest
    } else {
        name
    }
}

/// Whether `name` reads as a reverse-DNS identifier we can compare against installed
/// apps.
///
/// Two or more segments, none empty, and the first one all-alphabetic. The last condition
/// is doing most of the work: it rejects `AAProfilePicture_9C3821B4-…-8FED65C900F6.png`
/// and `12043870399`, which would otherwise pass as two-segment ids. Later segments are
/// left alone because real ones contain spaces and hyphens — `com.the-qt-company.Qt Apps`
/// is a preferences domain on this machine.
fn is_bundle_id(name: &str) -> bool {
    let mut segments = name.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    if first.is_empty() || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    let mut count = 0;
    for segment in segments {
        if segment.is_empty() {
            return false;
        }
        count += 1;
    }
    count >= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_identifier_is_itself() {
        assert_eq!(parse_id("com.foo.Bar").as_deref(), Some("com.foo.Bar"));
    }

    #[test]
    fn a_file_suffix_comes_off() {
        assert_eq!(
            parse_id("com.foo.Bar.plist").as_deref(),
            Some("com.foo.Bar")
        );
        assert_eq!(
            parse_id("com.foo.Bar.savedState").as_deref(),
            Some("com.foo.Bar")
        );
        assert_eq!(
            parse_id("examine.mac.binarycookies").as_deref(),
            Some("examine.mac")
        );
    }

    #[test]
    fn an_identifier_segment_that_looks_like_a_suffix_stays() {
        // 94 entries on this machine end in `.extension`, and `xyz.chatboxapp.app` is a
        // real app. Stripping whatever follows the last dot would rename all of them.
        for id in [
            "com.foo.Bar.extension",
            "com.foo.Bar.helper",
            "com.foo.Bar.widget",
            "xyz.chatboxapp.app",
        ] {
            assert_eq!(parse_id(id).as_deref(), Some(id));
        }
    }

    #[test]
    fn a_team_id_prefix_comes_off() {
        assert_eq!(
            parse_id("5ZSL2CJU2T.com.dingtalk.mac.tblive").as_deref(),
            Some("com.dingtalk.mac.tblive")
        );
        assert_eq!(
            parse_id("5ZSL2CJU2T.com.dingtalk.meeting.plist").as_deref(),
            Some("com.dingtalk.meeting")
        );
        assert_eq!(
            parse_id("5ZSL2CJU2T.com.dingtalk.meeting.binarycookies").as_deref(),
            Some("com.dingtalk.meeting")
        );
    }

    #[test]
    fn a_group_marker_comes_off_with_or_without_a_team_id() {
        assert_eq!(
            parse_id("243LU875E5.groups.com.apple.podcasts").as_deref(),
            Some("com.apple.podcasts")
        );
        // Ninety-odd of these exist on this machine. Left alone they read as ids whose
        // first segment is `group`, match nothing installed, and look abandoned.
        assert_eq!(
            parse_id("group.com.apple.notes").as_deref(),
            Some("com.apple.notes")
        );
        assert_eq!(
            parse_id("systemgroup.com.apple.icloud.searchpartyd.sharedsettings").as_deref(),
            Some("com.apple.icloud.searchpartyd.sharedsettings")
        );
        assert_eq!(
            parse_id("group.com.microsoft.shared").as_deref(),
            Some("com.microsoft.shared")
        );
        assert_eq!(
            parse_id("UBF8T346G9.Office").as_deref(),
            // One segment left after the TeamID, so there is nothing to compare against
            // an installed app and the entry is dropped rather than half-attributed.
            None
        );
    }

    #[test]
    fn a_leading_segment_is_only_a_team_id_if_it_has_a_digit() {
        // Ten capitals with no digit is a name, not a TeamID. Eating it would leave an
        // id that matches nothing installed — which is exactly how a live app's data
        // gets offered up.
        assert_eq!(
            parse_id("ABCDEFGHIJ.com.foo.Bar").as_deref(),
            Some("ABCDEFGHIJ.com.foo.Bar")
        );
        // Nine characters is not a TeamID, so nothing is stripped — and what remains
        // starts with a segment that is not a plausible TLD either, so the name is
        // unattributable and gets dropped. Both halves of that are the safe answer.
        assert_eq!(parse_id("5ZSL2CJU2.com.foo.Bar"), None);
    }

    #[test]
    fn a_name_that_is_not_an_identifier_is_dropped() {
        // Every one of these is on this machine. None can be checked against the
        // installed set, so none may be offered for deletion.
        for name in [
            "Code",
            "calibre",
            "AliLang",
            "微信开发者工具",
            "148001E3-4AE6-4F24-88D8-830E4DA25FA1",
            "12043870399.plist",
            "examine_mac",
            "AAProfilePicture_9C3821B4-3C8D-457C-8432-8FED65C900F6.png",
            "calibre-ebook.com",
            "",
            ".",
            "com.",
            ".com.foo",
        ] {
            assert_eq!(parse_id(name), None, "{name} should not parse as an id");
        }
    }

    #[test]
    fn spaces_and_hyphens_survive_in_later_segments() {
        for id in ["com.the-qt-company.Qt Apps", "com.seafile.TeamFile Client"] {
            assert_eq!(parse_id(id).as_deref(), Some(id));
        }
    }

    #[test]
    fn the_newest_hit_decides_the_age() {
        let hit = |modified| Hit {
            location: Location::Caches,
            path: PathBuf::from("/tmp/x"),
            raw_name: "x".to_string(),
            modified,
        };
        let candidate = Candidate {
            id: "com.foo.Bar".to_string(),
            hits: vec![hit(Some(100)), hit(None), hit(Some(300))],
        };
        assert_eq!(candidate.last_modified(), Some(300));

        let unknown = Candidate {
            id: "com.foo.Bar".to_string(),
            hits: vec![hit(None)],
        };
        assert_eq!(unknown.last_modified(), None, "unknown is not old");
    }
}
