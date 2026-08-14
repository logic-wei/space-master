//! Phase 9's first acceptance step: print the installed set orphan detection will
//! subtract from, so it can be hand-checked against what is actually on this machine.
//! Nothing here deletes anything, and nothing here is scored yet.
//!
//!   cargo test --test review_installed -- --ignored --nocapture
//!
//! What to look for, in order of how much it would cost to get wrong:
//!
//!   - every app you know is installed appears, embedded helpers included;
//!   - `reliable()` is true, and the unnamed count is small enough to explain;
//!   - the launchd labels look like software that is set up here, not noise.

use std::path::PathBuf;

use space_master_lib::safety::running_apps::RunningApps;
use space_master_lib::scan::orphans::installed::Installed;

#[test]
#[ignore = "reads the real /Applications and process list; run with --ignored --nocapture"]
fn the_real_installed_set_is_reviewable() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let running = RunningApps::detect();
    let installed = Installed::detect(&home, &running);

    let mut ids: Vec<&String> = installed.ids().iter().collect();
    ids.sort();
    println!("\n=== bundle ids ({}) ===", ids.len());
    for id in &ids {
        println!("{id}");
    }

    let mut labels: Vec<&String> = installed.labels().iter().collect();
    labels.sort();
    println!("\n=== launchd labels ({}) ===", labels.len());
    for label in &labels {
        println!("{label}");
    }

    let total = installed.named() + installed.unnamed();
    println!("\nbundles seen   = {total}");
    println!("named          = {}", installed.named());
    println!("unnamed        = {}", installed.unnamed());
    println!(
        "failure rate   = {:.2}%  (threshold 2%)",
        installed.unnamed() as f64 * 100.0 / total as f64
    );
    println!("reliable       = {}", installed.reliable());
    println!("\ncompare: ls -d /Applications/*.app ~/Applications/*.app /System/Applications/*.app | wc -l");

    // The bar is deliberately low. This asserts the walk ran at all — a real number is
    // in the hundreds — and leaves the actual review to the printout above, because
    // "did we find the app you care about" is not a thing an assertion can ask.
    assert!(
        installed.reliable(),
        "enumeration unreliable: {} of {total} bundles could not be named",
        installed.unnamed()
    );
    assert!(
        ids.len() >= 100,
        "only {} ids found; a Mac has more than that",
        ids.len()
    );

    // Running processes must be folded in, or an app that is open but installed
    // somewhere we do not walk would look uninstalled.
    for id in running.bundle_ids() {
        assert!(installed.covers(id), "running app {id} is not in the set");
    }
}
