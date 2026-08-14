//! Verifies that the Guard is actually on the preview path.
//!
//! The Guard's own rules are covered in `safety/tests.rs`. What is checked here is
//! the wiring: a [`build_plan`] that forgot to call `vet_all` would still return a
//! perfectly plausible plan, with the refused paths sitting in `accepted`, and
//! nothing else in the codebase would notice.
//!
//! Fake homes live under `target/` rather than `/tmp` for the same reason as the
//! Guard suite: `DENY_ABSOLUTE` covers `/private` and `/var`.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::clean::build_plan;
use crate::model::item::Target;
use crate::safety::guard::{DeleteMode, GuardCtx, RuleId};

const GENERATION: u64 = 7;
const TOKEN: u64 = 1;

struct Fake {
    _dir: TempDir,
    home: PathBuf,
    ctx: GuardCtx,
}

impl Fake {
    fn new() -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-homes");
        fs::create_dir_all(&base).expect("create test home base");
        let dir = TempDir::new_in(&base).expect("create fake home");
        let home = dir.path().canonicalize().expect("canonicalize fake home");
        Self {
            ctx: GuardCtx::for_test(&home),
            _dir: dir,
            home,
        }
    }

    /// Creates `rel` as a directory holding one file, and returns it the way a scan
    /// would hand it to the plan builder.
    fn target(&self, id: &str, rel: &str, bytes: u64) -> (String, Target) {
        let path = self.home.join(rel);
        fs::create_dir_all(&path).expect("create target dir");
        fs::write(path.join("payload"), b"x").expect("create payload");
        (id.to_string(), Target { path, bytes })
    }
}

#[test]
fn a_clean_selection_is_accepted_whole() {
    let f = Fake::new();
    let resolved = vec![
        f.target("pipCache", "Library/Caches/pip", 4096),
        f.target("npmCacache", ".npm/_cacache", 8192),
    ];

    let plan = build_plan(TOKEN, GENERATION, &resolved, DeleteMode::Permanent, &f.ctx);

    assert_eq!(plan.generation, GENERATION);
    assert_eq!(plan.accepted.len(), 2);
    assert!(plan.rejected.is_empty(), "{:?}", plan.rejected);
    assert_eq!(plan.estimated_bytes, 4096 + 8192);
    assert!(plan.accepted.iter().all(|e| e.is_dir));
}

#[test]
fn a_protected_path_is_rejected_not_accepted() {
    // The case worth having a permanent test for: something that must never be
    // touched reaches the plan builder anyway. It has to come back in `rejected`.
    let f = Fake::new();
    let resolved = vec![
        f.target("pipCache", "Library/Caches/pip", 4096),
        f.target("bogus", "Documents", 999_999),
    ];

    let plan = build_plan(TOKEN, GENERATION, &resolved, DeleteMode::Trash, &f.ctx);

    assert_eq!(plan.accepted.len(), 1);
    assert_eq!(plan.accepted[0].item_id, "pipCache");
    assert_eq!(plan.rejected.len(), 1);
    assert_eq!(plan.rejected[0].rule, RuleId::Protected);
    // A refused path must not count towards what the user is promised.
    assert_eq!(plan.estimated_bytes, 4096);
}

#[test]
fn permanent_mode_is_refused_outside_the_quick_catalog() {
    // `.cargo/registry` is an allowed root, so this passes every rule except R15.
    // Refusing it is what keeps permanent deletion bounded by a compile-time table.
    let f = Fake::new();
    let resolved = vec![f.target("cargoRegistry", ".cargo/registry", 4096)];

    let permanent = build_plan(TOKEN, GENERATION, &resolved, DeleteMode::Permanent, &f.ctx);
    assert_eq!(permanent.rejected.len(), 1);
    assert_eq!(permanent.rejected[0].rule, RuleId::PermanentNotAllowed);
    assert_eq!(permanent.estimated_bytes, 0);

    let trashed = build_plan(TOKEN, GENERATION, &resolved, DeleteMode::Trash, &f.ctx);
    assert_eq!(trashed.accepted.len(), 1);
}

#[test]
fn overlapping_selections_reject_both_sides() {
    let f = Fake::new();
    let resolved = vec![
        f.target("a", "Library/Caches/pip", 4096),
        f.target("b", "Library/Caches/pip/http", 2048),
    ];

    let plan = build_plan(TOKEN, GENERATION, &resolved, DeleteMode::Trash, &f.ctx);

    assert!(plan.accepted.is_empty(), "{:?}", plan.accepted);
    assert_eq!(plan.rejected.len(), 2);
    assert!(plan.rejected.iter().all(|r| r.rule == RuleId::Overlapping));
}

#[test]
fn each_accepted_entry_keeps_its_item_and_size() {
    // A `Children`-scoped item contributes several paths under one id. Losing that
    // association would make the per-row totals in the UI wrong while the grand
    // total still added up.
    let f = Fake::new();
    let resolved: Vec<(String, Target)> = [1024u64, 2048, 4096]
        .into_iter()
        .enumerate()
        .map(|(i, bytes)| f.target("trash", &format!(".Trash/item-{i}"), bytes))
        .collect();

    let plan = build_plan(TOKEN, GENERATION, &resolved, DeleteMode::Permanent, &f.ctx);

    assert_eq!(plan.accepted.len(), 3);
    assert!(plan.accepted.iter().all(|e| e.item_id == "trash"));
    assert_eq!(plan.estimated_bytes, 1024 + 2048 + 4096);
    for entry in &plan.accepted {
        let expected = resolved
            .iter()
            .find(|(_, t)| t.path == entry.path)
            .expect("entry path was in the input")
            .1
            .bytes;
        assert_eq!(
            entry.bytes,
            expected,
            "{} lost its size",
            entry.path.display()
        );
    }
}
