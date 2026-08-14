//! Phase 9's second acceptance step: print the raw candidates, before any scoring, next
//! to what the installed set says about each one. Nothing here deletes anything.
//!
//!   cargo test --test review_candidates -- --ignored --nocapture
//!
//! What to look for:
//!
//!   - in the `installed` block, every row really is software you have. A wrong entry
//!     there is harmless — it only means we protect something deletable.
//!   - in the `unclaimed` block, every row really is software you do *not* have. A wrong
//!     entry there is the failure this whole feature has to avoid, so read it closely,
//!     and check the name parsing: `TEAMID.groups.com.x.y` must appear as `com.x.y`.
//!   - in the `dropped` block, nothing that looks like a bundle id. Those names were
//!     judged unattributable and will never be offered for deletion.

use std::collections::BTreeSet;
use std::path::PathBuf;

use space_master_lib::safety::running_apps::RunningApps;
use space_master_lib::scan::orphans::candidates::{self, Candidate};
use space_master_lib::scan::orphans::installed::Installed;

#[test]
#[ignore = "reads the real ~/Library; run with --ignored --nocapture"]
fn the_real_candidate_list_is_reviewable() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let installed = Installed::detect(&home, &RunningApps::detect());
    assert!(installed.reliable(), "installed enumeration is unreliable");

    let found = candidates::collect(&home);

    // The prefixes the scoring layer will hard-veto. `is.workflow.` and `developer.apple.`
    // are Apple's too — Shortcuts and the WWDC app — and neither starts with `com.apple.`.
    let apple_owned = |id: &str| {
        ["com.apple.", "is.workflow.", "developer.apple."]
            .iter()
            .any(|p| id.starts_with(p))
    };
    let (apple, rest): (Vec<_>, Vec<_>) = found.into_iter().partition(|c| apple_owned(&c.id));
    let (claimed, unclaimed): (Vec<_>, Vec<_>) =
        rest.into_iter().partition(|c| installed.covers(&c.id));

    println!("\n=== installed, so protected ({}) ===", claimed.len());
    for c in &claimed {
        print(c);
    }

    println!(
        "\n=== unclaimed by any installed app ({}) ===",
        unclaimed.len()
    );
    for c in &unclaimed {
        let launchd = if installed.has_launchd_job(&c.id) {
            "  [launchd job]"
        } else {
            ""
        };
        print(c);
        if !launchd.is_empty() {
            println!("{launchd}");
        }
    }

    println!("\n=== com.apple.*, never offered ({}) ===", apple.len());

    // The names that did not parse as identifiers. Printed as a set because the same
    // name shows up in several locations, and the point is to eyeball the vocabulary.
    let mut dropped = BTreeSet::new();
    let library = home.join("Library");
    for dir in [
        "Caches",
        "Preferences",
        "Application Support",
        "Containers",
        "Group Containers",
        "Saved Application State",
        "Logs",
        "WebKit",
        "HTTPStorages",
        "Application Scripts",
    ] {
        let Ok(read) = std::fs::read_dir(library.join(dir)) else {
            continue;
        };
        for entry in read.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if candidates::parse_id(&name).is_none() {
                dropped.insert(name);
            }
        }
    }
    println!("\n=== dropped as unattributable ({}) ===", dropped.len());
    for name in &dropped {
        println!("{name}");
    }

    println!(
        "\nsummary: {} protected, {} unclaimed, {} apple, {} dropped",
        claimed.len(),
        unclaimed.len(),
        apple.len(),
        dropped.len()
    );

    // Nothing is asserted about the unclaimed count — the review above is the check.
    // This only catches a collection that silently found nothing.
    assert!(!claimed.is_empty(), "no candidate matched an installed app");
}

fn print(c: &Candidate) {
    let places: Vec<String> = c.hits.iter().map(|h| format!("{:?}", h.location)).collect();
    println!("{:<52} {}", c.id, places.join(", "));
    for hit in &c.hits {
        if hit.raw_name != c.id {
            println!("{:<52}   raw: {}", "", hit.raw_name);
        }
    }
}
