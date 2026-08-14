//! Phase 7's acceptance step: read the plan the professional mode would act on,
//! entry by entry. Ignored by default because it measures the real home directory;
//! nothing here deletes anything.
//!
//!   cargo test --test review_dev_plan -- --ignored --nocapture

use space_master_lib::commands::clean::build_plan;
use space_master_lib::model::item::Target;
use space_master_lib::safety::guard::{DeleteMode, GuardCtx};
use space_master_lib::scan::{dev_caches, session::Scanner};

#[test]
#[ignore = "measures the real home directory; run with --ignored --nocapture"]
fn the_real_dev_plan_is_reviewable() {
    let home = std::env::home_dir()
        .expect("$HOME")
        .canonicalize()
        .expect("home canonicalizes");
    let scanner = Scanner::new().expect("scanner");
    let handle = scanner.begin();

    let scanned = dev_caches::scan(&handle, &home, &mut |_| {});
    let resolved: Vec<(String, Target)> = scanned
        .iter()
        .flat_map(|s| s.targets.iter().map(|t| (s.item.id.clone(), t.clone())))
        .collect();

    let ctx = GuardCtx::detect(None).expect("guard context");
    // The mode the page sends. Trash, not Permanent — see the assertion at the end.
    let plan = build_plan(1, handle.generation, &resolved, DeleteMode::Trash, &ctx);

    println!("\n=== scan ===");
    for s in &scanned {
        println!(
            "{:<28} {:>13} bytes  {:>7} files  last_used {:?}  {} issues",
            s.item.id,
            s.item.bytes,
            s.item.files,
            s.item.last_used_ms,
            s.item.issues.len()
        );
    }

    println!("\n=== accepted ({}) ===", plan.accepted.len());
    for e in &plan.accepted {
        println!(
            "{:<28} {:>13} bytes  {}",
            e.item_id,
            e.bytes,
            e.path.display()
        );
    }

    println!("\n=== rejected ({}) ===", plan.rejected.len());
    for r in &plan.rejected {
        println!("{:?} {} {:?}", r.rule, r.path.display(), r.detail);
    }

    println!("\n=== advisories ===");
    for a in dev_caches::advisories(&home) {
        println!("{:<12} {}  ->  {}", a.id, a.path.display(), a.command);
    }

    println!("\nestimated_bytes = {}", plan.estimated_bytes);

    for e in &plan.accepted {
        assert!(
            e.path.starts_with(&home),
            "{} escaped $HOME",
            e.path.display()
        );
    }

    // The whole reason Tier B is recoverable by construction. If R15 ever stopped
    // covering these paths, the professional mode would silently gain the power to
    // delete a rebuilt-in-an-afternoon cache outright.
    let permanent = build_plan(2, handle.generation, &resolved, DeleteMode::Permanent, &ctx);
    assert!(
        permanent.accepted.is_empty(),
        "R15 let {} Tier B paths through as permanent deletions",
        permanent.accepted.len()
    );
}
