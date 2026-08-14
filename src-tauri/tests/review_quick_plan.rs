//! Phase 4's acceptance step: read the plan the app would act on, entry by entry,
//! against the Tier A catalog. Ignored by default because it measures the real home
//! directory; nothing here deletes anything.
//!
//!   cargo test --test review_quick_plan -- --ignored --nocapture

use space_master_lib::commands::clean::build_plan;
use space_master_lib::model::item::Target;
use space_master_lib::safety::guard::{DeleteMode, GuardCtx};
use space_master_lib::scan::{quick, session::Scanner};

#[test]
#[ignore = "measures the real home directory; run with --ignored --nocapture"]
fn the_real_quick_plan_is_reviewable() {
    let home = std::env::home_dir()
        .expect("$HOME")
        .canonicalize()
        .expect("home canonicalizes");
    let scanner = Scanner::new().expect("scanner");
    let handle = scanner.begin();

    let scanned = quick::scan(&handle, &home, &mut |_| {});
    let resolved: Vec<(String, Target)> = scanned
        .iter()
        .flat_map(|s| s.targets.iter().map(|t| (s.item.id.clone(), t.clone())))
        .collect();

    // The same mode the one-click page sends, so R15 is exercised for real.
    let ctx = GuardCtx::detect(None).expect("guard context");
    let plan = build_plan(1, handle.generation, &resolved, DeleteMode::Permanent, &ctx);

    println!("\n=== scan ===");
    for s in &scanned {
        println!(
            "{:<18} {:>12} bytes  {:>7} files  {:?}  {} targets  {} issues",
            s.item.id,
            s.item.bytes,
            s.item.files,
            s.item.scope,
            s.targets.len(),
            s.item.issues.len()
        );
        for issue in &s.item.issues {
            println!("      issue {:?} {}", issue.kind, issue.path.display());
        }
    }

    println!("\n=== accepted ({}) ===", plan.accepted.len());
    for e in &plan.accepted {
        println!(
            "{:<18} {:>12} bytes  {}  {}",
            e.item_id,
            e.bytes,
            if e.is_dir { "dir " } else { "file" },
            e.path.display()
        );
    }

    println!("\n=== rejected ({}) ===", plan.rejected.len());
    for r in &plan.rejected {
        println!("{:?} {} {:?}", r.rule, r.path.display(), r.detail);
    }

    println!("\nestimated_bytes = {}", plan.estimated_bytes);

    // Every path the plan would touch has to sit under the home this test resolved.
    for e in &plan.accepted {
        assert!(
            e.path.starts_with(&home),
            "{} escaped $HOME",
            e.path.display()
        );
    }
}
