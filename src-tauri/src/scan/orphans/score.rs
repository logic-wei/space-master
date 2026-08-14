//! Deciding how confident we are that a candidate's owner is gone.
//!
//! Two stages, in this order for a reason. First a veto: a candidate whose owner is
//! plainly present is dropped out of consideration entirely, without being measured. Then,
//! for whatever is left, a score built from evidence.
//!
//! The score is never shown. A number invites the user to read precision into it that is
//! not there — the weights below are judgement, not measurement. What the UI shows is the
//! evidence itself ("not used in over a year", "found in 3 places"), which the user can
//! check, and a bucket that decides whether the row is ticked by default.
//!
//! ## What the buckets are for
//!
//! Only [`Bucket::Likely`] is ticked by default. [`Bucket::Possible`] requires the user to
//! expand the row before it can be ticked at all, which is a deliberate obstacle to
//! selecting everything without reading it. [`Bucket::Keep`] cannot be ticked, and is
//! still displayed with its reason — "we looked at this and decided against it" is worth
//! more to a user's trust than silence.
//!
//! ## Signals from the plan that are not implemented, and why
//!
//!   - *LaunchServices orphan records (+10)*. The only route to them is parsing
//!     `lsregister -dump`, an undocumented format. If it changed we would lose the signal
//!     silently, and a silent loss here means a false positive.
//!   - *Referenced by a login item (−20)*. Launchd job files are already a hard veto and
//!     cover nearly the same ground.
//!   - *A CLI of the same name exists (−10)*. Too weak to justify: the last segment of a
//!     bundle id matching some binary on `$PATH` is mostly coincidence.

use std::collections::HashSet;
use std::path::Path;

use crate::safety::running_apps::RunningApps;
use crate::scan::orphans::candidates::{Candidate, Location};
use crate::scan::orphans::installed::Installed;

/// Prefixes owned by macOS or its bundled software, which are never offered.
///
/// `com.apple.` is the obvious one. The rest were all found on this machine sitting
/// outside it: Shortcuts writes under `is.workflow.`, the WWDC app under
/// `developer.apple.`, and `org.cups.PrintingPrefs` holds the printing subsystem's
/// settings — an id with no app behind it at all, which is exactly the shape that scores
/// highest if nothing stops it.
const SYSTEM_PREFIXES: [&str; 4] = [
    "com.apple.",
    "is.workflow.",
    "developer.apple.",
    "org.cups.",
];

/// Why a candidate is not on offer. Wording lives under `orphans.protected.*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Veto {
    /// Belongs to macOS or to software Apple ships.
    System,
    /// This app's own data. Deleting it mid-run would take the ledger with it.
    OwnData,
    /// The id, or something it descends from, names an installed app.
    Installed,
    /// An installed app's id descends from this one — a shared container named after the
    /// family rather than the app. `com.microsoft.rdc` is the group container of the
    /// installed `com.microsoft.rdc.macos`.
    InstalledFamily,
    /// A launchd job carries this label, so something arranged to run under this name.
    LaunchdJob,
    /// A process is running from this bundle right now.
    Running,
}

/// A fact about a candidate, shown to the user as a chip. Wording lives under
/// `orphans.evidence.*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Evidence {
    UnusedOver180d,
    UnusedOver1y,
    UnusedOver2y,
    /// Written to within the last 30 days. The strongest single argument against.
    RecentActivity,
    /// Nothing could be stat'd, so age is unknown — which counts for nothing, in either
    /// direction.
    AgeUnknown,
    /// Found in more than one of the locations we look at.
    ManyLocations,
    /// Found in exactly one, and that one is Preferences: a few kilobytes of settings,
    /// which is the cheapest thing to keep and the most annoying thing to lose.
    OnlyPreferences,
    /// Three or more segments, the shape a real bundle id has.
    StandardId,
    /// Two segments. Often not a bundle id at all but a directory that happens to have a
    /// dot in it.
    ShortId,
    /// Some installed app comes from what looks like the same vendor.
    SameVendor,
    /// Holds a database outside a cache directory, so it is plausibly the only copy of
    /// something.
    HoldsDatabase,
    Large,
    Tiny,
}

impl Evidence {
    fn weight(self) -> i32 {
        match self {
            Evidence::UnusedOver180d => 20,
            Evidence::UnusedOver1y => 32,
            Evidence::UnusedOver2y => 45,
            Evidence::RecentActivity => -50,
            Evidence::AgeUnknown => 0,
            Evidence::ManyLocations => 0, // Scaled by count; added separately.
            Evidence::OnlyPreferences => -8,
            Evidence::StandardId => 15,
            Evidence::ShortId => -18,
            // Strong enough that no row is ticked by default while software from the same
            // vendor is installed, short of every other signal being maximal. On this
            // machine every vendor match but one turned out to be data belonging to
            // installed software that could not be attributed exactly — Sony's
            // `com.sony.EULA-PP-Checker` alongside an installed Imaging Edge, Qt's
            // `com.qtproject.*` alongside `org.qt-project.*`. The plan's -25 left the Sony
            // row in the default selection.
            //
            // Not a veto, though: `com.microsoft.teams` is genuinely gone while Office is
            // installed, and that shape is common.
            Evidence::SameVendor => -35,
            Evidence::HoldsDatabase => -15,
            Evidence::Large => 10,
            Evidence::Tiny => -12,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Bucket {
    /// Ticked by default.
    Likely,
    /// Selectable only after the row is expanded.
    Possible,
    /// Shown collapsed, under a heading that says this needs the user's judgement.
    Unclear,
    /// Not selectable.
    Keep,
}

#[derive(Debug, Clone)]
pub struct Assessment {
    pub bucket: Bucket,
    pub veto: Option<Veto>,
    pub evidence: Vec<Evidence>,
    /// Kept for the review harness and the logs, never sent to the frontend.
    pub score: i32,
}

/// Whether the candidate's owner is present, and how we know.
///
/// Runs before anything is measured: most candidates on a normal machine are vetoed here,
/// and walking a container tree to find that out afterwards would be wasted work.
pub fn veto(
    candidate: &Candidate,
    installed: &Installed,
    running: &RunningApps,
    app_data_dir: Option<&Path>,
) -> Option<Veto> {
    if SYSTEM_PREFIXES.iter().any(|p| candidate.id.starts_with(p)) {
        return Some(Veto::System);
    }
    if let Some(own) = app_data_dir {
        if candidate.hits.iter().any(|h| h.path.starts_with(own)) {
            return Some(Veto::OwnData);
        }
    }
    if running.owns_bundle(&candidate.id) {
        return Some(Veto::Running);
    }
    if installed.covers(&candidate.id) {
        return Some(Veto::Installed);
    }
    if installed.family_of(&candidate.id) {
        return Some(Veto::InstalledFamily);
    }
    if installed.has_launchd_job(&candidate.id) {
        return Some(Veto::LaunchdJob);
    }
    None
}

/// What is known about a candidate that survived [`veto`].
///
/// `now` is seconds since the epoch, passed in rather than read so the thresholds can be
/// tested. `bytes` and `holds_database` come from measuring the candidate's paths.
pub fn assess(
    candidate: &Candidate,
    bytes: u64,
    holds_database: bool,
    now: i64,
    vendors: &VendorIndex,
) -> Assessment {
    let mut evidence = Vec::new();

    match candidate.last_modified() {
        None => evidence.push(Evidence::AgeUnknown),
        Some(at) => {
            let days = (now - at).max(0) / 86_400;
            // Only one age band applies. Ordered newest-first so the most specific claim
            // about a recently touched candidate wins.
            if days <= 30 {
                evidence.push(Evidence::RecentActivity);
            } else if days > 730 {
                evidence.push(Evidence::UnusedOver2y);
            } else if days > 365 {
                evidence.push(Evidence::UnusedOver1y);
            } else if days > 180 {
                evidence.push(Evidence::UnusedOver180d);
            }
        }
    }

    let places = distinct_locations(candidate);
    if places > 1 {
        evidence.push(Evidence::ManyLocations);
    } else if candidate
        .hits
        .iter()
        .all(|h| h.location == Location::Preferences)
    {
        evidence.push(Evidence::OnlyPreferences);
    }

    let segments = candidate.id.split('.').count();
    evidence.push(if segments >= 3 {
        Evidence::StandardId
    } else {
        Evidence::ShortId
    });

    if vendors.knows(&candidate.id) {
        evidence.push(Evidence::SameVendor);
    }
    if holds_database {
        evidence.push(Evidence::HoldsDatabase);
    }
    if bytes > 100 * 1024 * 1024 {
        evidence.push(Evidence::Large);
    } else if bytes < 1024 * 1024 {
        evidence.push(Evidence::Tiny);
    }

    // 50 is "no idea either way". Every weight moves away from it.
    let mut score: i32 = 50;
    for e in &evidence {
        score += e.weight();
    }
    // +8 per additional location, capped: five places is not twice the evidence of three.
    score += (8 * (places as i32 - 1)).clamp(0, 24);
    let score = score.clamp(0, 100);

    Assessment {
        bucket: bucket_for(score),
        veto: None,
        evidence,
        score,
    }
}

fn bucket_for(score: i32) -> Bucket {
    match score {
        78..=100 => Bucket::Likely,
        55..=77 => Bucket::Possible,
        30..=54 => Bucket::Unclear,
        _ => Bucket::Keep,
    }
}

/// The assessment for a vetoed candidate: nothing measured, nothing selectable.
pub fn protected(veto: Veto) -> Assessment {
    Assessment {
        bucket: Bucket::Keep,
        veto: Some(veto),
        evidence: Vec::new(),
        score: 0,
    }
}

/// Counts locations rather than hits. A candidate can appear twice in one directory —
/// `com.amazon.Lassen` and `group.com.amazon.Lassen` both normalise to the same id — and
/// that is one place, not two.
fn distinct_locations(candidate: &Candidate) -> usize {
    let mut seen: Vec<Location> = Vec::new();
    for hit in &candidate.hits {
        if !seen.contains(&hit.location) {
            seen.push(hit.location);
        }
    }
    seen.len()
}

/// The vendors this machine has software from, for recognising a candidate as a sibling of
/// something installed.
///
/// Compares the second segment with punctuation removed and case folded, which is not
/// fussiness: Qt Creator's bundle id is `org.qt-project.qtcreator` while the preferences it
/// writes are `com.qtproject.QtCreator`. Nothing about enumerating apps can connect those
/// two — Qt derives its preferences domain from a `QSettings` organization name, not from
/// the bundle — so without this the settings of an installed, in-use application look
/// abandoned. Both reduce to `qtproject`.
#[derive(Debug, Default)]
pub struct VendorIndex {
    tokens: HashSet<String>,
}

impl VendorIndex {
    pub fn build(installed: &Installed) -> Self {
        Self {
            tokens: installed.ids().iter().filter_map(|id| vendor(id)).collect(),
        }
    }

    fn knows(&self, id: &str) -> bool {
        vendor(id).is_some_and(|v| self.tokens.contains(&v))
    }
}

/// The vendor segment of `id`, normalised. `None` for an id with no vendor segment.
///
/// Deliberately not the first segment: that is the TLD, and `com` matching `com` would
/// make every id a sibling of every other.
fn vendor(id: &str) -> Option<String> {
    let segment = id.split('.').nth(1)?;
    let token: String = segment
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    (!token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_800_000_000;

    fn candidate(id: &str, places: &[Location], age_days: i64) -> Candidate {
        Candidate {
            id: id.to_string(),
            hits: places
                .iter()
                .map(|&location| crate::scan::orphans::candidates::Hit {
                    location,
                    path: PathBuf::from("/tmp").join(id),
                    raw_name: id.to_string(),
                    modified: Some(NOW - age_days * DAY),
                })
                .collect(),
        }
    }

    fn assess_plain(c: &Candidate) -> Assessment {
        assess(c, 10 * 1024 * 1024, false, NOW, &VendorIndex::default())
    }

    #[test]
    fn only_one_age_band_applies() {
        for (days, expected) in [
            (10, Evidence::RecentActivity),
            (200, Evidence::UnusedOver180d),
            (400, Evidence::UnusedOver1y),
            (900, Evidence::UnusedOver2y),
        ] {
            let c = candidate("com.foo.Bar", &[Location::Caches], days);
            let ages: Vec<Evidence> = assess_plain(&c)
                .evidence
                .into_iter()
                .filter(|e| e.weight().abs() >= 20 || *e == Evidence::AgeUnknown)
                .collect();
            assert_eq!(ages, vec![expected], "at {days} days");
        }
    }

    #[test]
    fn an_age_between_31_and_180_days_says_nothing() {
        let c = candidate("com.foo.Bar", &[Location::Caches], 90);
        let e = assess_plain(&c).evidence;
        assert!(!e.contains(&Evidence::RecentActivity));
        assert!(!e.contains(&Evidence::UnusedOver180d));
    }

    #[test]
    fn an_unknown_age_is_not_an_old_age() {
        let mut c = candidate("com.foo.Bar", &[Location::Caches], 900);
        c.hits[0].modified = None;
        let a = assess_plain(&c);
        assert!(a.evidence.contains(&Evidence::AgeUnknown));
        assert!(!a.evidence.contains(&Evidence::UnusedOver2y));
    }

    #[test]
    fn recent_activity_alone_keeps_a_row_out_of_the_default_selection() {
        // Everything else about this candidate argues for deletion: old-style id, four
        // places, large. One write in the last month is enough to stop it.
        let c = candidate(
            "com.foo.Bar",
            &[
                Location::Caches,
                Location::Preferences,
                Location::Containers,
                Location::Logs,
            ],
            5,
        );
        let a = assess(&c, 500 * 1024 * 1024, false, NOW, &VendorIndex::default());
        assert_ne!(a.bucket, Bucket::Likely, "score was {}", a.score);
    }

    #[test]
    fn a_long_unused_multi_location_app_is_offered_by_default() {
        let c = candidate(
            "com.example.hello",
            &[Location::Containers, Location::ApplicationScripts],
            800,
        );
        let a = assess(&c, 50 * 1024 * 1024, false, NOW, &VendorIndex::default());
        assert_eq!(a.bucket, Bucket::Likely, "score was {}", a.score);
    }

    #[test]
    fn a_lone_preferences_file_counts_against_but_does_not_veto() {
        // Being wrong about a settings file is invisible until the user reinstalls and
        // finds their configuration gone, so it argues against. It does not decide: a
        // plist for software that has been uninstalled for two years is what this feature
        // is looking for, and refusing to offer it would leave the commonest shape of
        // leftover permanently unreachable.
        let c = candidate("com.foo.Bar", &[Location::Preferences], 900);
        let a = assess(&c, 4 * 1024, false, NOW, &VendorIndex::default());
        assert!(a.evidence.contains(&Evidence::OnlyPreferences));

        let elsewhere = candidate("com.foo.Bar", &[Location::ApplicationSupport], 900);
        let b = assess(&elsewhere, 4 * 1024, false, NOW, &VendorIndex::default());
        assert!(a.score < b.score);
    }

    #[test]
    fn one_doubt_is_enough_to_untick_a_lone_preferences_file() {
        let c = candidate("com.foo.Bar", &[Location::Preferences], 900);
        let mut vendors = VendorIndex::default();
        vendors.tokens.insert("foo".to_string());
        let same_vendor = assess(&c, 4 * 1024, false, NOW, &vendors);
        assert_ne!(same_vendor.bucket, Bucket::Likely);

        let holds_data = assess(&c, 4 * 1024, true, NOW, &VendorIndex::default());
        assert_ne!(holds_data.bucket, Bucket::Likely);

        let short_id = candidate("foo.Bar", &[Location::Preferences], 900);
        let a = assess(&short_id, 4 * 1024, false, NOW, &VendorIndex::default());
        assert_ne!(a.bucket, Bucket::Likely);
    }

    #[test]
    fn repeated_locations_count_once() {
        let mut c = candidate("com.foo.Bar", &[Location::ApplicationScripts], 200);
        c.hits.push(c.hits[0].clone());
        let a = assess_plain(&c);
        assert!(
            !a.evidence.contains(&Evidence::ManyLocations),
            "one directory twice is one place"
        );
    }

    #[test]
    fn extra_locations_stop_helping_after_four() {
        let many = [
            Location::Caches,
            Location::Preferences,
            Location::Containers,
            Location::Logs,
            Location::WebKit,
            Location::HttpStorages,
        ];
        let four = assess_plain(&candidate("com.foo.Bar", &many[..4], 200)).score;
        let six = assess_plain(&candidate("com.foo.Bar", &many, 200)).score;
        assert_eq!(four, six);
    }

    #[test]
    fn a_vendor_whose_software_is_installed_counts_against() {
        let mut vendors = VendorIndex::default();
        vendors.tokens.insert("qtproject".to_string());
        let c = candidate("com.qtproject.QtCreator", &[Location::Preferences], 900);

        let a = assess(&c, 4 * 1024, false, NOW, &vendors);
        assert!(a.evidence.contains(&Evidence::SameVendor));
        assert_ne!(a.bucket, Bucket::Likely, "score was {}", a.score);
    }

    #[test]
    fn a_vendor_match_keeps_a_row_out_of_the_default_selection() {
        // The row that made this a rule: Sony's Imaging Edge is installed, and
        // `com.sony.EULA-PP-Checker` — a preferences domain written by one of its
        // frameworks, with no bundle of its own to find — was ticked by default.
        let mut vendors = VendorIndex::default();
        vendors.tokens.insert("sony".to_string());
        let c = candidate(
            "com.sony.EULA-PP-Checker",
            &[
                Location::Caches,
                Location::Preferences,
                Location::WebKit,
                Location::HttpStorages,
            ],
            200,
        );
        let a = assess(&c, 1_300_000, false, NOW, &vendors);
        assert_ne!(a.bucket, Bucket::Likely, "score was {}", a.score);

        // Still not a veto. Two years untouched, in four places, and large is enough
        // evidence to outweigh a shared vendor.
        let long_gone = candidate(
            "com.sony.EULA-PP-Checker",
            &c.hits.iter().map(|h| h.location).collect::<Vec<_>>(),
            900,
        );
        let b = assess(&long_gone, 500 * 1024 * 1024, false, NOW, &vendors);
        assert_eq!(b.bucket, Bucket::Likely, "score was {}", b.score);
    }

    #[test]
    fn punctuation_and_case_do_not_hide_a_vendor() {
        // The real pair from this machine: Qt Creator is installed as
        // `org.qt-project.qtcreator` and writes `com.qtproject.QtCreator`.
        assert_eq!(
            vendor("org.qt-project.qtcreator"),
            vendor("com.qtproject.X")
        );
        assert_eq!(
            vendor("com.The-Qt-Company.x").as_deref(),
            Some("theqtcompany")
        );
    }

    #[test]
    fn the_tld_is_not_a_vendor() {
        // Otherwise every `com.*` id is a sibling of every other, and the signal fires
        // on everything.
        let mut vendors = VendorIndex::default();
        vendors.tokens.insert("com".to_string());
        assert!(!vendors.knows("com.foo.Bar"));
        assert_eq!(vendor("com"), None);
    }

    #[test]
    fn the_bucket_boundaries_are_where_the_plan_put_them() {
        assert_eq!(bucket_for(78), Bucket::Likely);
        assert_eq!(bucket_for(77), Bucket::Possible);
        assert_eq!(bucket_for(55), Bucket::Possible);
        assert_eq!(bucket_for(54), Bucket::Unclear);
        assert_eq!(bucket_for(30), Bucket::Unclear);
        assert_eq!(bucket_for(29), Bucket::Keep);
        assert_eq!(bucket_for(0), Bucket::Keep);
    }

    #[test]
    fn a_score_cannot_leave_its_range() {
        let piled_on = candidate(
            "com.foo.Bar",
            &[
                Location::Caches,
                Location::Preferences,
                Location::Containers,
                Location::Logs,
            ],
            5000,
        );
        let a = assess(&piled_on, u64::MAX, false, NOW, &VendorIndex::default());
        assert!((0..=100).contains(&a.score), "score was {}", a.score);
    }

    #[test]
    fn a_protected_row_is_not_selectable_and_says_why() {
        let a = protected(Veto::LaunchdJob);
        assert_eq!(a.bucket, Bucket::Keep);
        assert_eq!(a.veto, Some(Veto::LaunchdJob));
        assert!(a.evidence.is_empty());
    }
}
