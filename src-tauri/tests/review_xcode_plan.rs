//! Phase 8's acceptance step: read the plan the Xcode page would act on, row by row,
//! and check it against `du`. Ignored by default because it measures the real home
//! directory; nothing here deletes anything.
//!
//!   cargo test --test review_xcode_plan -- --ignored --nocapture

use space_master_lib::commands::clean::build_plan;
use space_master_lib::model::item::Target;
use space_master_lib::safety::guard::{DeleteMode, GuardCtx};
use space_master_lib::scan::{session::Scanner, xcode};

#[test]
#[ignore = "measures the real home directory; run with --ignored --nocapture"]
fn the_real_xcode_plan_is_reviewable() {
    let home = std::env::home_dir()
        .expect("$HOME")
        .canonicalize()
        .expect("home canonicalizes");
    let scanner = Scanner::new().expect("scanner");
    let handle = scanner.begin();

    let scanned = xcode::scan(&handle, &home, &mut |_| {});
    let resolved: Vec<(String, Target)> = scanned
        .iter()
        .flat_map(|s| s.targets.iter().map(|t| (s.item.id.clone(), t.clone())))
        .collect();

    let ctx = GuardCtx::detect(None).expect("guard context");
    let plan = build_plan(1, handle.generation, &resolved, DeleteMode::Trash, &ctx);

    println!("\n=== scan ===");
    for s in &scanned {
        println!(
            "{:<64} {:>13} bytes  {:>7} files  note {:?}",
            s.item.id, s.item.bytes, s.item.files, s.item.note
        );
    }

    // Per-group totals, which are what `du -sk` on each catalog directory reports.
    println!("\n=== group totals ===");
    for group in space_master_lib::catalog::xcode::XCODE_GROUPS {
        let bytes: u64 = scanned
            .iter()
            .filter(|s| s.item.note == Some(group.id))
            .map(|s| s.item.bytes)
            .sum();
        let rows = scanned
            .iter()
            .filter(|s| s.item.note == Some(group.id))
            .count();
        println!(
            "{:<14} {:>13} bytes  {:>3} rows  {}",
            group.id,
            bytes,
            rows,
            home.join(group.rel).display()
        );
    }

    println!("\n=== accepted ({}) ===", plan.accepted.len());
    for e in &plan.accepted {
        println!("{:>13} bytes  {}", e.bytes, e.path.display());
    }

    println!("\n=== rejected ({}) ===", plan.rejected.len());
    for r in &plan.rejected {
        println!("{:?} {} {:?}", r.rule, r.path.display(), r.detail);
    }

    println!("\nestimated_bytes = {}", plan.estimated_bytes);

    for e in &plan.accepted {
        assert!(
            e.path.starts_with(&home),
            "{} escaped $HOME",
            e.path.display()
        );
    }

    // Every row must carry wording, or the UI shows a build number and a size with
    // nothing saying whether it holds symbols or the dSYMs of a shipped build.
    for s in &scanned {
        assert!(s.item.note.is_some(), "{} has no note", s.item.id);
    }

    // Xcode's caches are Tier B: recoverable by construction, enforced by R15 rather
    // than by the page happening to send `Trash`.
    let permanent = build_plan(2, handle.generation, &resolved, DeleteMode::Permanent, &ctx);
    assert!(
        permanent.accepted.is_empty(),
        "R15 let {} Xcode paths through as permanent deletions",
        permanent.accepted.len()
    );
}
