//! Phase 9's third acceptance step: run the whole orphan pipeline over the real machine
//! and print what it decided, so the high-confidence bucket can be checked row by row.
//! Nothing here deletes anything.
//!
//!   cargo test --test review_orphans -- --ignored --nocapture
//!
//! What to look for:
//!
//!   - every row under `likely` really is software you no longer have. This bucket is
//!     ticked by default, so the target is zero false positives — the `open -R` commands
//!     printed at the end are there to check them one at a time.
//!   - no `com.apple.*` and nothing currently running appears outside `protected`.
//!   - the evidence chips on each row are things you could verify yourself. If a row's
//!     evidence does not justify its bucket, the weights are wrong, not the row.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use space_master_lib::fsutil::walk::MeasureCtx;
use space_master_lib::safety::running_apps::RunningApps;
use space_master_lib::scan::orphans::candidates::{self, Candidate};
use space_master_lib::scan::orphans::installed::Installed;
use space_master_lib::scan::orphans::measure::footprint;
use space_master_lib::scan::orphans::score::{self, Assessment, Bucket};

#[test]
#[ignore = "reads the real ~/Library; run with --ignored --nocapture"]
fn the_real_orphan_verdicts_are_reviewable() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let running = RunningApps::detect();
    let installed = Installed::detect(&home, &running);
    assert!(installed.reliable(), "installed enumeration is unreliable");

    let vendors = score::VendorIndex::build(&installed);
    let ctx = MeasureCtx {
        pool: Arc::new(rayon::ThreadPoolBuilder::new().build().expect("rayon pool")),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    // The app's own data directory. Hardcoded here rather than asked of Tauri because
    // this harness has no app handle; the real caller passes what Tauri reports.
    let own = home.join("Library/Application Support/dev.local.spacemaster");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64;

    let mut verdicts: Vec<(Candidate, Assessment, u64)> = Vec::new();
    for candidate in candidates::collect(&home) {
        if let Some(veto) = score::veto(&candidate, &installed, &running, Some(&own)) {
            verdicts.push((candidate, score::protected(veto), 0));
            continue;
        }
        let f = footprint(&candidate, &ctx);
        let assessment = score::assess(&candidate, f.bytes, f.holds_database, now, &vendors);
        verdicts.push((candidate, assessment, f.bytes));
    }

    let mut by_bucket: BTreeMap<&str, Vec<&(Candidate, Assessment, u64)>> = BTreeMap::new();
    for v in &verdicts {
        by_bucket.entry(label(v.1.bucket)).or_default().push(v);
    }

    // Protected first and abbreviated: it is the largest group by far and the question
    // about it ("is anything here actually deletable?") is answered by the reason, not by
    // the row. Counted per reason so a veto that never fires is visible.
    let protected: Vec<_> = verdicts.iter().filter(|v| v.1.veto.is_some()).collect();
    let mut by_reason: BTreeMap<String, usize> = BTreeMap::new();
    for v in &protected {
        *by_reason
            .entry(format!("{:?}", v.1.veto.expect("veto")))
            .or_default() += 1;
    }
    println!("\n=== protected ({}) ===", protected.len());
    for (reason, count) in &by_reason {
        println!("{count:>5}  {reason}");
    }

    for bucket in [Bucket::Likely, Bucket::Possible, Bucket::Unclear] {
        let rows = by_bucket.remove(label(bucket)).unwrap_or_default();
        println!("\n=== {} ({}) ===", label(bucket), rows.len());
        for (candidate, assessment, bytes) in rows {
            print(candidate, assessment, *bytes);
        }
    }

    // `Keep` without a veto: measured, and the evidence argued against. Worth reading —
    // these are the rows the UI shows with a reason but no checkbox.
    let kept: Vec<_> = verdicts
        .iter()
        .filter(|v| v.1.bucket == Bucket::Keep && v.1.veto.is_none())
        .collect();
    println!("\n=== keep, on the evidence ({}) ===", kept.len());
    for (candidate, assessment, bytes) in kept {
        print(candidate, assessment, *bytes);
    }

    let likely: Vec<_> = verdicts
        .iter()
        .filter(|v| v.1.bucket == Bucket::Likely)
        .collect();
    println!(
        "\n=== the {} rows that would be ticked by default ===",
        likely.len()
    );
    for (candidate, _, _) in &likely {
        println!("open -R '{}'", candidate.hits[0].path.display());
    }

    let reclaimable: u64 = likely.iter().map(|v| v.2).sum();
    println!(
        "\nsummary: {} candidates, {} protected, {} ticked by default, {:.2} GiB",
        verdicts.len(),
        protected.len(),
        likely.len(),
        reclaimable as f64 / (1024.0 * 1024.0 * 1024.0),
    );

    // Nothing is asserted about the bucket sizes — the review above is the check. These
    // two only catch a pipeline that silently collapsed.
    assert!(!protected.is_empty(), "nothing was protected");
    for (candidate, assessment, _) in &verdicts {
        if candidate.id.starts_with("com.apple.") {
            assert_eq!(
                assessment.bucket,
                Bucket::Keep,
                "{} is Apple's and must not be selectable",
                candidate.id
            );
        }
    }
}

fn label(bucket: Bucket) -> &'static str {
    match bucket {
        Bucket::Likely => "likely",
        Bucket::Possible => "possible",
        Bucket::Unclear => "unclear",
        Bucket::Keep => "keep",
    }
}

fn print(candidate: &Candidate, assessment: &Assessment, bytes: u64) {
    let places: Vec<String> = candidate
        .hits
        .iter()
        .map(|h| format!("{:?}", h.location))
        .collect();
    let evidence: Vec<String> = assessment
        .evidence
        .iter()
        .map(|e| format!("{e:?}"))
        .collect();
    println!(
        "{:>3}  {:<9}  {:<46} {}",
        assessment.score,
        human(bytes),
        candidate.id,
        places.join(", ")
    );
    println!("{:>3}  {:<9}  {}", "", "", evidence.join(" · "));
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "K", "M", "G"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1}{}", UNITS[unit])
}
