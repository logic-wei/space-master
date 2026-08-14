//! Phase 5's acceptance step: put a real directory in the Trash through the real code
//! path — Guard, `remove::run`, ledger — and leave it there so Finder's "Put Back" can
//! be tried by hand.
//!
//! Ignored by default because it writes to the real home directory. It creates its own
//! victim under `~/Library/Caches`, so no catalog entry is involved and nothing the
//! user owns is at risk.
//!
//!   cargo test --test trash_round_trip -- --ignored --nocapture

use std::fs;
use std::path::PathBuf;

use space_master_lib::remove::{self, ledger};
use space_master_lib::safety::guard::{vet, DeleteMode, GuardCtx};

/// Named so that a leftover directory is obviously ours if the test aborts midway.
const VICTIM: &str = "zz-spacemaster-trash-probe";

#[test]
#[ignore = "writes to the real ~/Library/Caches and ~/.Trash; run with --ignored --nocapture"]
fn a_real_directory_reaches_the_trash_and_is_recorded() {
    let home = std::env::home_dir()
        .expect("$HOME")
        .canonicalize()
        .expect("home canonicalizes");
    let victim = home.join("Library/Caches").join(VICTIM);

    // A directory with nested content, so this exercises a recursive move rather than
    // a single-file rename.
    fs::create_dir_all(victim.join("nested")).expect("create victim");
    fs::write(victim.join("top.txt"), b"top").expect("write top");
    fs::write(victim.join("nested/inner.txt"), vec![b'x'; 4096]).expect("write inner");

    let ctx = GuardCtx::detect(None).expect("guard context");
    let target = match vet(&victim, DeleteMode::Trash, &ctx) {
        Ok(target) => target,
        Err(rejection) => panic!(
            "the Guard refused our own probe directory: {:?} {:?}",
            rejection.rule, rejection.detail
        ),
    };

    // A temp ledger, so a probe run does not land in the app's real history.
    let ledger_home = tempfile::tempdir().expect("temp ledger dir");
    let ledger_dir = ledger_home.path();

    let jobs = vec![remove::Job {
        item_id: "probe".to_string(),
        bytes: 4096,
        target,
    }];
    let outcome =
        remove::run(&jobs, DeleteMode::Trash, Vec::new(), ledger_dir).expect("run the removal");

    println!("\nbatch      {}", outcome.batch);
    println!("removed    {}", outcome.removed.len());
    println!("failed     {}", outcome.failed.len());
    for f in &outcome.failed {
        println!("  {:?} {} — {}", f.kind, f.path.display(), f.detail);
    }

    assert!(outcome.failed.is_empty(), "the trash move failed");
    assert_eq!(outcome.removed.len(), 1);
    assert!(!victim.exists(), "{} is still there", victim.display());

    // The point of the ledger: a batch that completed must be closed, or the startup
    // check would report every successful clean as an interrupted one.
    let unfinished = ledger::unfinished(ledger_dir).expect("read ledger");
    assert!(
        unfinished.is_empty(),
        "a completed batch was left open: {unfinished:?}"
    );

    let records = fs::read_to_string(ledger_dir.join(ledger::LEDGER_FILE)).expect("ledger file");
    assert!(
        records.contains(VICTIM),
        "the ledger has no record naming the path it deleted"
    );
    print!("\n=== ledger ===\n{records}");

    // Left in the Trash on purpose. Nothing here can verify Finder's "Put Back", so the
    // item has to survive the test for a human to try it.
    report_trash_contents(&home);
    println!(
        "\nNow check ~/.Trash for `{VICTIM}` and try Finder -> Put Back.\nIt should return to {}",
        victim.display()
    );
}

/// Best-effort look inside the Trash. Reading `~/.Trash` needs Full Disk Access, so
/// failure here says nothing about whether the move worked — which is why no assertion
/// depends on it. Printed only to save a trip to Finder when the permission is granted.
fn report_trash_contents(home: &std::path::Path) {
    match fs::read_dir(home.join(".Trash")) {
        Ok(read) => {
            let found: Vec<PathBuf> = read
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.to_string_lossy().contains(VICTIM))
                .collect();
            println!("\n~/.Trash entries matching the probe: {found:?}");
        }
        Err(e) => println!("\n(cannot list ~/.Trash: {e}. Full Disk Access is needed.)"),
    }
}
