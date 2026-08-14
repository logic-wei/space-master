//! What is installed, which is the set orphan detection subtracts from.
//!
//! Every mistake in this file has the same shape: an app we fail to notice becomes an
//! app whose preferences, caches and containers look abandoned. That is the one failure
//! mode of the whole feature that costs the user something they cannot get back, so the
//! bias throughout is towards claiming *more* is installed:
//!
//!   - a bundle whose `Info.plist` we cannot read still counts, and enough of those switch
//!     the feature off entirely (see [`Installed::reliable`]);
//!   - apps are looked for well outside `/Applications`, including a bounded sweep of the
//!     home directory, because installers put them anywhere;
//!   - launchd labels and running processes are folded in, because software can arrange
//!     to be present without an `.app` anywhere we look;
//!   - identifiers match by segment prefix, so `com.foo.Bar` being installed protects
//!     `com.foo.Bar.Helper` without us having to have found the helper.
//!
//! Note what is *not* here: `/System/Library/CoreServices` and the rest of the OS. Those
//! ids are all `com.apple.*`, which the scoring layer refuses outright, and walking the
//! system for identifiers we would ignore anyway buys nothing.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use jwalk::WalkDirGeneric;

use crate::fsutil::bundle::{identify, launchd_label, Identity};

/// How deep below an app directory to keep looking for bundles.
///
/// Helpers nest in no fixed place — this machine has them under `Contents/Frameworks`,
/// `Contents/Resources`, `Contents/SharedSupport`, `Contents/MacOS`, `Contents/Helpers`
/// and directly under `Contents` — so there is no subpath list to whitelist. Six levels
/// reaches a helper inside a helper, which is as far as anything real goes, and costs
/// about a second and a half across `/Applications` and `/System/Applications`.
const MAX_DEPTH: usize = 6;

/// Where `.app` bundles live. `~/Applications` is resolved against `$HOME` by the
/// caller.
///
/// `/Library` and `/opt` are here because a surprising amount of installed software keeps
/// its bundle nowhere near `/Applications`: Microsoft AutoUpdate under
/// `/Library/Application Support/Microsoft/MAU2.0`, the Java updater under
/// `/Library/Internet Plug-Ins`, McAfee's menulet under `/Library/Application Support`,
/// Homebrew's own copies under `/opt/homebrew/Cellar`. Each of those writes preferences
/// and caches into `~/Library`, so leaving them out means offering to delete the data of
/// software that is installed and actively updating itself. Both cost under half a second.
const APP_DIRS: [&str; 4] = ["/Applications", "/System/Applications", "/Library", "/opt"];

/// How deep to sweep `$HOME` for apps that live outside any conventional directory.
///
/// Four levels reaches `~/Qt/Qt Creator.app` and `~/Qt/Tools/CMake/CMake.app`, which is
/// how the Qt installer lays itself out. Before this sweep existed, Qt Creator's
/// preferences were the clearest false positive on this machine: the app is installed and
/// used, and its settings looked abandoned.
const HOME_SWEEP_DEPTH: usize = 4;

/// Directory names the `$HOME` sweep refuses to descend into.
///
/// `Library` because every `.app` under it belongs to something found elsewhere, and
/// because `~/Library/Developer/CoreSimulator` holds a copy of every app ever installed
/// on a simulator — folding those in would protect the leftovers of throwaway builds,
/// which is a good part of what this feature is for.
///
/// `.Trash` because an app in the Trash is being *un*installed. Counting it as installed
/// would break the most obvious route to this page: drag the app to the Trash, then come
/// looking for what it left behind.
const HOME_SWEEP_SKIP: [&str; 2] = ["Library", ".Trash"];

/// Directories holding launchd job files, in the order they are read. The system ones
/// are included because a login item installed for all users is still evidence the
/// software is set up here.
const LAUNCHD_DIRS: [&str; 2] = ["/Library/LaunchAgents", "/Library/LaunchDaemons"];

#[derive(Debug, Default)]
pub struct Installed {
    ids: HashSet<String>,
    /// Labels of launchd job files, kept apart from `ids` so the scoring layer can say
    /// *why* something is protected. A label is weaker evidence than a bundle: it
    /// names software that arranged to run, not necessarily software still present.
    labels: HashSet<String>,
    named: usize,
    unnamed: usize,
}

impl Installed {
    /// Enumerates apps, launchd jobs and running processes.
    pub fn detect(home: &Path, running: &crate::safety::running_apps::RunningApps) -> Self {
        let mut out = Self::default();

        let dirs = APP_DIRS
            .iter()
            .map(PathBuf::from)
            .chain(std::iter::once(home.join("Applications")));
        for dir in dirs {
            out.absorb(bundles_under(&dir));
        }
        out.absorb(home_sweep(home));

        let launchd = LAUNCHD_DIRS
            .iter()
            .map(PathBuf::from)
            .chain(std::iter::once(home.join("Library/LaunchAgents")));
        for dir in launchd {
            out.absorb_labels(&dir);
        }

        // A process whose app we never found is still an installed app — it is running.
        out.ids.extend(running.bundle_ids().iter().cloned());

        out
    }

    /// Whether the enumeration is trustworthy enough to draw conclusions from.
    ///
    /// Below the threshold a handful of unreadable bundles only risks a few rows being
    /// offered that should not be; above it, something systematic is wrong — a macOS
    /// release changing `Info.plist`, or the app running without permission to read
    /// `/Applications` — and every installed app looks uninstalled. There is no partial
    /// answer worth giving in that case, so the caller shows nothing.
    ///
    /// The denominator is bundles that *have* an `Info.plist`, which is the population the
    /// question is about. Directories merely named `.app` are excluded, and there are more
    /// of them than one expects: `/opt/homebrew/share/qt/mkspecs` alone holds five called
    /// `Info.plist.app`. Counting those as failures made the rate a measure of how much
    /// Homebrew is installed rather than of whether plist reading works.
    pub fn reliable(&self) -> bool {
        let total = self.named + self.unnamed;
        // No apps at all is not a 0% failure rate, it is a failed enumeration. A Mac
        // with an empty `/Applications` does not exist.
        total > 0 && self.unnamed * 50 <= total
    }

    /// Whether `id` is installed, or is a segment-level extension of something that is.
    ///
    /// `com.foo.Bar` installed covers `com.foo.Bar.Helper` but not `com.foo.Barn`,
    /// which is why this walks dot boundaries instead of calling `starts_with`.
    pub fn covers(&self, id: &str) -> bool {
        if self.ids.contains(id) {
            return true;
        }
        id.char_indices()
            .filter(|(_, c)| *c == '.')
            .any(|(at, _)| self.ids.contains(&id[..at]))
    }

    /// Whether some installed id descends from `id` — the opposite direction to
    /// [`Installed::covers`], and a separate signal because it means something different.
    ///
    /// `covers` only looks downward on purpose: finding `com.foo.Bar.Helper` says nothing
    /// about whether `com.foo.Bar` is still here, and that is exactly the leftover case.
    /// Looking upward answers a different question — is this a shared container named after
    /// an app *family*? `~/Library/Group Containers/com.microsoft.rdc` is used by the
    /// installed `com.microsoft.rdc.macos`, and nothing points from the container back to
    /// the app that opens it.
    ///
    /// Kept apart from `covers` so the UI can tell the user which of the two it is.
    pub fn family_of(&self, id: &str) -> bool {
        let prefix = format!("{id}.");
        self.ids.iter().any(|known| known.starts_with(&prefix))
    }

    /// Whether a launchd job file names `id`, exactly. Not prefix-matched: the label of
    /// a job is a specific claim, and widening it here would double up with `covers`.
    pub fn has_launchd_job(&self, id: &str) -> bool {
        self.labels.contains(id)
    }

    pub fn named(&self) -> usize {
        self.named
    }

    pub fn unnamed(&self) -> usize {
        self.unnamed
    }

    pub fn ids(&self) -> &HashSet<String> {
        &self.ids
    }

    pub fn labels(&self) -> &HashSet<String> {
        &self.labels
    }

    /// Records the identity of every bundle in `paths`.
    fn absorb(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            match identify(&path) {
                Identity::Named(id) => {
                    self.named += 1;
                    self.ids.insert(id);
                }
                // Counted, not ignored. This is the number `reliable` is about: there is a
                // bundle here and we failed to name it, so its data is about to look
                // abandoned.
                Identity::Unnamed => self.unnamed += 1,
                // No `Info.plist`, so there is no identifier to have missed and nothing
                // under `~/Library` that could be misjudged because of it. A Finder alias
                // and a Qt mkspec template both land here.
                Identity::NotABundle => {}
            }
        }
    }

    fn absorb_labels(&mut self, dir: &Path) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "plist") {
                if let Some(label) = launchd_label(&path) {
                    self.labels.insert(label);
                }
            }
        }
    }
}

/// Every `.app` path under `dir`, bundles inside bundles included.
///
/// Symlinks are followed here, unlike everywhere else in this app. Elsewhere not
/// following them is a safety property — we refuse to delete through a link. Here the
/// question is only what exists, and an app symlinked into `/Applications` (which is how
/// several installers and `~/Applications` itself work) is installed.
fn bundles_under(dir: &Path) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return Vec::new();
    }
    WalkDirGeneric::<((), ())>::new(dir)
        .skip_hidden(false)
        .follow_links(true)
        .sort(false)
        .max_depth(MAX_DEPTH)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().as_encoded_bytes().ends_with(b".app"))
        .map(|e| e.path())
        .collect()
}

/// Apps sitting loose in the home directory, wherever their installer chose to put them.
///
/// Symlinks are *not* followed here, unlike [`bundles_under`]. A link in the home
/// directory points either at something already walked or at somewhere outside it, and one
/// pointing at `/` would drag four levels of the whole filesystem into a sweep that is
/// meant to take half a second.
fn home_sweep(home: &Path) -> Vec<PathBuf> {
    WalkDirGeneric::<((), ())>::new(home)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .max_depth(HOME_SWEEP_DEPTH)
        .process_read_dir(|_, _, _, children| {
            for child in children.iter_mut().filter_map(|c| c.as_mut().ok()) {
                if HOME_SWEEP_SKIP.iter().any(|s| child.file_name() == *s) {
                    child.read_children = None;
                }
            }
        })
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().as_encoded_bytes().ends_with(b".app"))
        .map(|e| e.path())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(ids: &[&str]) -> Installed {
        Installed {
            ids: ids.iter().map(|s| (*s).to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn an_installed_id_covers_its_own_helpers() {
        let installed = with(&["com.foo.Bar"]);
        assert!(installed.covers("com.foo.Bar"));
        assert!(installed.covers("com.foo.Bar.Helper"));
        assert!(installed.covers("com.foo.Bar.Helper.Renderer"));
    }

    #[test]
    fn a_shared_text_prefix_is_not_a_shared_id() {
        // The reason this compares segments: `Barn` is a different app from `Bar`, and
        // treating it as covered would protect a genuine orphan — a lesser mistake than
        // the reverse, but still a wrong answer.
        let installed = with(&["com.foo.Bar"]);
        assert!(!installed.covers("com.foo.Barn"));
        assert!(!installed.covers("com.foo.Barn.Helper"));
        assert!(!installed.covers("com.foo.Ba"));
    }

    #[test]
    fn a_helper_does_not_cover_its_parent() {
        // Only downwards. Finding `com.foo.Bar.Helper` says nothing about whether the
        // app that owned it is still here — that is exactly the leftover case.
        let installed = with(&["com.foo.Bar.Helper"]);
        assert!(!installed.covers("com.foo.Bar"));
        assert!(!installed.covers("com.foo"));
    }

    #[test]
    fn nothing_is_covered_by_an_empty_set() {
        let installed = with(&[]);
        assert!(!installed.covers("com.foo.Bar"));
        assert!(!installed.covers(""));
    }

    #[test]
    fn a_family_is_recognised_upwards_only() {
        // The real case: the group container is `com.microsoft.rdc`, the app is
        // `com.microsoft.rdc.macos`. `covers` cannot see it, and must not start to.
        let installed = with(&["com.microsoft.rdc.macos"]);
        assert!(installed.family_of("com.microsoft.rdc"));
        assert!(!installed.covers("com.microsoft.rdc"));

        // Not the other direction, or every helper's parent would claim to be a family.
        assert!(!installed.family_of("com.microsoft.rdc.macos.Helper"));
        // And not a shared text prefix: `rdcx` is a different name.
        assert!(!installed.family_of("com.microsoft.rdcx"));
    }

    #[test]
    fn an_enumeration_that_found_nothing_is_not_reliable() {
        assert!(!Installed::default().reliable());
    }

    #[test]
    fn the_failure_rate_decides_reliability() {
        let at_two_percent = Installed {
            named: 98,
            unnamed: 2,
            ..Default::default()
        };
        assert!(at_two_percent.reliable());

        let over = Installed {
            named: 96,
            unnamed: 4,
            ..Default::default()
        };
        assert!(
            !over.reliable(),
            "4 of 100 unreadable must disable the page"
        );
    }
}
