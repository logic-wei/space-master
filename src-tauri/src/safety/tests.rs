//! Guard test suite.
//!
//! Every test runs against a throwaway home directory, so a bug here cannot
//! touch the real one. The fake home is created under `target/` rather than
//! `/tmp`: `DENY_ABSOLUTE` covers `/private` and `/var`, which is correct in
//! production but would refuse every path in a `TempDir`-based home.
//!
//! The real home directory is only exercised by the `#[ignore]`d tests at the
//! bottom, which assert refusals and never write anything.

use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::deny;
use super::guard::{vet, vet_all, DeleteMode, GuardCtx, RuleId};
use super::roots::{self, Containment};
use super::running_apps::RunningApps;
use crate::catalog::quick;
use crate::model::item::ItemScope;
use crate::scan::orphans::candidates;

struct Fake {
    _dir: TempDir,
    home: PathBuf,
    ctx: GuardCtx,
}

impl Fake {
    fn at(&self, rel: &str) -> PathBuf {
        self.home.join(rel)
    }

    fn refuse(&self, rel: &str, mode: DeleteMode) -> RuleId {
        let path = self.at(rel);
        match vet(&path, mode, &self.ctx) {
            Ok(_) => panic!("{} was accepted but must be refused", path.display()),
            Err(r) => r.rule,
        }
    }

    fn accept(&self, rel: &str, mode: DeleteMode) -> PathBuf {
        let path = self.at(rel);
        match vet(&path, mode, &self.ctx) {
            Ok(t) => t.path().to_path_buf(),
            Err(r) => panic!("{} was refused by {:?}", path.display(), r.rule),
        }
    }
}

/// Directories and files present in every fake home. A mix of legitimately
/// cleanable paths, protected paths, and near-misses.
const DIRS: &[&str] = &[
    "Library/Caches/pip",
    "Library/Caches/Homebrew",
    "Library/Caches/com.apple.dt.Xcode",
    "Library/Caches/realdir",
    "Library/Caches-backup/x",
    "Library/Logs/SomeApp",
    "Library/Developer/Xcode/DerivedData/App-abcdef",
    "Library/Developer/Xcode/UserData/KeyBindings",
    "Library/Containers/com.example.app/Data",
    "Library/Containers/com.apple.Safari/Data",
    "Library/Application Support/CrashReporter",
    "Library/Application Support/MobileSync/Backup",
    "Library/Keychains",
    "Library/CloudStorage/OneDrive",
    "Library/Mobile Documents/com~apple~CloudDocs",
    "Documents/taxes",
    "Desktop",
    ".npm/_cacache/content-v2",
    ".cargo/registry/cache",
    ".ssh",
    ".config/gh",
    ".rustup/toolchains/stable",
    ".Trash/olditem",
    "workspace/proj/node_modules/left-pad",
    "workspace/proj/src",
];

const FILES: &[&str] = &[
    "Library/Logs/SomeApp/run.log",
    "Library/Preferences/com.example.app.plist",
    "Documents/taxes/2025.pdf",
];

fn fake() -> Fake {
    // `env!` rather than a runtime temp dir: see the module comment.
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-homes");
    fs::create_dir_all(&base).expect("create test home base");
    let dir = TempDir::new_in(&base).expect("create fake home");
    let home = dir.path().canonicalize().expect("canonicalize fake home");

    for rel in DIRS {
        fs::create_dir_all(home.join(rel)).expect("create fixture dir");
    }
    for rel in FILES {
        let path = home.join(rel);
        fs::create_dir_all(path.parent().unwrap()).expect("create fixture file parent");
        fs::write(&path, b"x").expect("create fixture file");
    }

    let ctx = GuardCtx::for_test(&home);
    Fake {
        _dir: dir,
        home,
        ctx,
    }
}

// ---------------------------------------------------------------------------
// T01-T04: paths that must be accepted, or the app does nothing useful.
// ---------------------------------------------------------------------------

#[test]
fn t01_named_cache_subdirectory_is_accepted() {
    let f = fake();
    assert_eq!(
        f.accept("Library/Caches/pip", DeleteMode::Trash),
        f.at("Library/Caches/pip")
    );
}

#[test]
fn t02_hidden_dotfile_cache_is_accepted() {
    let f = fake();
    f.accept(".npm/_cacache", DeleteMode::Trash);
    f.accept(".cargo/registry", DeleteMode::Trash);
}

#[test]
fn t03_regular_file_is_accepted() {
    let f = fake();
    f.accept("Library/Logs/SomeApp/run.log", DeleteMode::Trash);
}

#[test]
fn t04_derived_data_entry_is_accepted() {
    let f = fake();
    f.accept(
        "Library/Developer/Xcode/DerivedData/App-abcdef",
        DeleteMode::Trash,
    );
}

// ---------------------------------------------------------------------------
// T05-T08: shape and scope.
// ---------------------------------------------------------------------------

#[test]
fn t05_dot_dot_traversal_is_refused() {
    let f = fake();
    assert_eq!(
        f.refuse("Library/Caches/../Documents/taxes", DeleteMode::Trash),
        RuleId::NonNormalComponent
    );
}

#[test]
fn t06_symlink_target_is_refused_not_followed() {
    let f = fake();
    let link = f.at("Library/Caches/link-to-docs");
    std::os::unix::fs::symlink(f.at("Documents"), &link).unwrap();
    assert_eq!(
        f.refuse("Library/Caches/link-to-docs", DeleteMode::Trash),
        RuleId::Symlink
    );
    // The link itself survives, and so does its target.
    assert!(fs::symlink_metadata(&link).is_ok());
    assert!(f.at("Documents").is_dir());
}

#[test]
fn t07_sibling_with_root_name_as_byte_prefix_is_refused() {
    // `Library/Caches-backup` shares a byte prefix with the `Library/Caches`
    // root but not a component prefix. This is the test that would fail if
    // containment were ever rewritten as string comparison.
    let f = fake();
    assert_eq!(
        f.refuse("Library/Caches-backup/x", DeleteMode::Trash),
        RuleId::OutsideRoots
    );
}

#[test]
fn t08_home_and_filesystem_roots_are_refused() {
    let f = fake();
    for p in [f.home.as_path(), Path::new("/"), Path::new("/Users")] {
        let rule = vet(p, DeleteMode::Trash, &f.ctx).unwrap_err().rule;
        assert_eq!(rule, RuleId::TooShallow, "{}", p.display());
    }
}

#[test]
fn t08b_relative_and_nul_paths_are_refused() {
    let f = fake();
    let rel = Path::new("Library/Caches/pip");
    assert_eq!(
        vet(rel, DeleteMode::Trash, &f.ctx).unwrap_err().rule,
        RuleId::NotAbsolute
    );

    let nul = PathBuf::from(std::ffi::OsStr::from_bytes(b"/Users/x/Library/Caches/a\0b"));
    assert_eq!(
        vet(&nul, DeleteMode::Trash, &f.ctx).unwrap_err().rule,
        RuleId::NulByte
    );
}

// ---------------------------------------------------------------------------
// T09-T14: the deny list, entry by entry.
// ---------------------------------------------------------------------------

#[test]
fn t09_documents_is_refused() {
    let f = fake();
    assert_eq!(
        f.refuse("Documents/taxes", DeleteMode::Trash),
        RuleId::Protected
    );
    assert_eq!(
        f.refuse("Documents/taxes/2025.pdf", DeleteMode::Trash),
        RuleId::Protected
    );
}

#[test]
fn t10_keychains_is_refused() {
    let f = fake();
    assert_eq!(
        f.refuse("Library/Keychains", DeleteMode::Trash),
        RuleId::Protected
    );
}

#[test]
fn t11_cloud_storage_is_refused() {
    // Deleting here removes the server-side copy too, so it must never be
    // reachable regardless of how much space it appears to free.
    let f = fake();
    assert_eq!(
        f.refuse("Library/CloudStorage/OneDrive", DeleteMode::Trash),
        RuleId::Protected
    );
}

#[test]
fn t12_mobile_documents_is_refused() {
    let f = fake();
    assert_eq!(
        f.refuse(
            "Library/Mobile Documents/com~apple~CloudDocs",
            DeleteMode::Trash
        ),
        RuleId::Protected
    );
}

#[test]
fn t13_credential_directories_are_refused() {
    let f = fake();
    assert_eq!(f.refuse(".ssh", DeleteMode::Trash), RuleId::Protected);
    assert_eq!(f.refuse(".config/gh", DeleteMode::Trash), RuleId::Protected);
}

#[test]
fn t14_toolchains_and_device_backups_are_refused() {
    let f = fake();
    assert_eq!(
        f.refuse(".rustup/toolchains/stable", DeleteMode::Trash),
        RuleId::Protected
    );
    assert_eq!(
        f.refuse(
            "Library/Application Support/MobileSync/Backup",
            DeleteMode::Trash
        ),
        RuleId::Protected
    );
    // UserData sits inside the Library/Developer root, so only the deny list
    // stops it.
    assert_eq!(
        f.refuse(
            "Library/Developer/Xcode/UserData/KeyBindings",
            DeleteMode::Trash
        ),
        RuleId::Protected
    );
}

// ---------------------------------------------------------------------------
// T15: ancestors of protected paths.
// ---------------------------------------------------------------------------

#[test]
fn t15_ancestor_of_a_protected_path_is_refused() {
    let f = fake();
    assert_eq!(
        f.refuse("Library", DeleteMode::Trash),
        RuleId::WouldTakeProtected
    );
}

#[test]
fn t15b_shared_container_roots_are_children_only() {
    // The root exists and is in scope, but deleting it wholesale would take
    // unrelated apps' data.
    let f = fake();
    assert_eq!(
        f.refuse("Library/Caches", DeleteMode::Trash),
        RuleId::RootNotDeletable
    );
    assert_eq!(
        f.refuse(".Trash", DeleteMode::Trash),
        RuleId::RootNotDeletable
    );
}

// ---------------------------------------------------------------------------
// T16-T17: permanent deletion is confined to the Tier A catalog.
// ---------------------------------------------------------------------------

#[test]
fn t16_permanent_outside_quick_catalog_is_refused() {
    let f = fake();
    // Accepted for Trash, refused for Permanent: the difference is R15 alone.
    f.accept(".cargo/registry", DeleteMode::Trash);
    assert_eq!(
        f.refuse(".cargo/registry", DeleteMode::Permanent),
        RuleId::PermanentNotAllowed
    );
    assert_eq!(
        f.refuse(
            "Library/Developer/Xcode/DerivedData/App-abcdef",
            DeleteMode::Permanent
        ),
        RuleId::PermanentNotAllowed
    );
}

#[test]
fn t17_permanent_inside_quick_catalog_is_accepted() {
    let f = fake();
    f.accept(".npm/_cacache", DeleteMode::Permanent);
    f.accept("Library/Caches/pip", DeleteMode::Permanent);
    f.accept("Library/Caches/Homebrew", DeleteMode::Permanent);
    // Children of a `Children`-scoped entry qualify; the entry itself does not.
    f.accept(".Trash/olditem", DeleteMode::Permanent);
}

#[test]
fn t17b_every_quick_catalog_entry_is_reachable() {
    // A Tier A entry that Guard would refuse is a dead row: one-click clean would
    // silently skip it forever. Catch that at test time instead.
    let f = fake();
    for entry in quick::QUICK_ENTRIES {
        let path = f.at(entry.rel);
        let expect_self_deletable = entry.scope == ItemScope::SelfDir;
        let containment = roots::classify(&path, &f.home);
        assert_ne!(
            containment,
            Containment::Outside,
            "{} is not under any root",
            entry.rel
        );
        if expect_self_deletable {
            assert_ne!(
                containment,
                Containment::IsChildrenOnlyRoot,
                "{} is a children-only root but declares SelfDir scope",
                entry.rel
            );
        }
        assert!(
            deny::containing_entry(&path, &f.home).is_none(),
            "{} is on the deny list",
            entry.rel
        );
        assert!(
            deny::descendant_entry(&path, &f.home).is_none(),
            "{} would take a protected path with it",
            entry.rel
        );
    }
}

#[test]
fn t17d_every_orphan_location_is_trashable_and_never_permanent() {
    // Orphan detection can only offer what the Guard would accept, and a location missing
    // from `ROOTS` would make every leftover found there unofferable forever with nothing
    // else in the codebase saying so.
    //
    // The other half is the guarantee the orphans page relies on rather than enforces:
    // leftovers are recoverable because R15 refuses permanent deletion outside the Tier A
    // catalog, whatever mode the frontend asks for.
    let f = fake();
    for location in candidates::LOCATIONS {
        let rel = format!("Library/{}/com.example.gone", location.dir());
        fs::create_dir_all(f.at(&rel)).expect("create orphan directory");

        f.accept(&rel, DeleteMode::Trash);

        // `~/Library/Logs` is a Tier A quick-clean entry, so R15 does permit permanent
        // deletion beneath it. That is a decision about the one-click page and does not
        // reach the orphans page, which only ever asks for `Trash`.
        if location == candidates::Location::Logs {
            continue;
        }
        assert_eq!(
            f.refuse(&rel, DeleteMode::Permanent),
            RuleId::PermanentNotAllowed,
            "{rel} could be deleted permanently"
        );
    }
}

// ---------------------------------------------------------------------------
// T18: batch-level overlap.
// ---------------------------------------------------------------------------

#[test]
fn t18_overlapping_batch_entries_reject_both_sides() {
    let f = fake();
    let parent = f.at("Library/Caches/pip");
    let child = f.at("Library/Caches/pip/http");
    fs::create_dir_all(&child).unwrap();
    let other = f.at(".npm/_cacache");

    let (accepted, rejected) = vet_all(
        &[parent.clone(), child.clone(), other.clone()],
        DeleteMode::Trash,
        &f.ctx,
    );

    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].path(), other);
    assert_eq!(rejected.len(), 2);
    assert!(rejected.iter().all(|r| r.rule == RuleId::Overlapping));
}

#[test]
fn t18b_exact_duplicates_collapse() {
    let f = fake();
    let p = f.at("Library/Caches/pip");
    let (accepted, rejected) = vet_all(&[p.clone(), p.clone()], DeleteMode::Trash, &f.ctx);
    assert_eq!(accepted.len(), 1);
    assert!(rejected.is_empty());
}

// ---------------------------------------------------------------------------
// T19-T20: volume and running-process checks.
// ---------------------------------------------------------------------------

#[test]
fn t19_path_on_another_volume_is_refused() {
    let mut f = fake();
    f.ctx.set_home_dev(u64::MAX);
    assert_eq!(
        f.refuse("Library/Caches/pip", DeleteMode::Trash),
        RuleId::OtherVolume
    );
}

#[test]
fn t20_container_of_a_running_app_is_refused() {
    let mut f = fake();
    f.ctx.set_running(RunningApps::with_bundle_ids(
        ["com.example.app".to_string()],
    ));
    assert_eq!(
        f.refuse("Library/Containers/com.example.app/Data", DeleteMode::Trash),
        RuleId::AppRunning
    );
    // A container whose app is not running stays cleanable.
    let g = fake();
    g.accept("Library/Containers/com.example.app/Data", DeleteMode::Trash);
}

#[test]
fn t20b_apple_owned_containers_are_refused() {
    let f = fake();
    assert_eq!(
        f.refuse(
            "Library/Containers/com.apple.Safari/Data",
            DeleteMode::Trash
        ),
        RuleId::SystemBundle
    );
}

// ---------------------------------------------------------------------------
// Remaining rules.
// ---------------------------------------------------------------------------

#[test]
fn missing_path_is_refused() {
    let f = fake();
    assert_eq!(
        f.refuse("Library/Caches/never-existed", DeleteMode::Trash),
        RuleId::Missing
    );
}

#[test]
fn symlinked_parent_component_is_refused() {
    // R4 only inspects the final component. A link anywhere in the parent chain
    // is caught by comparing the canonical path against the literal one.
    let f = fake();
    let link = f.at("Library/Caches/aliaslink");
    std::os::unix::fs::symlink(f.at("Library/Caches/realdir"), &link).unwrap();
    fs::create_dir_all(f.at("Library/Caches/realdir/inner")).unwrap();

    let through_link = link.join("inner");
    let rule = vet(&through_link, DeleteMode::Trash, &f.ctx)
        .unwrap_err()
        .rule;
    assert_eq!(rule, RuleId::PathAliased);
}

#[test]
fn fifo_is_refused() {
    // A fifo stands in for the whole "not a file or directory" class. A unix
    // socket would be the more obvious fixture, but `sockaddr_un` caps paths at
    // 104 bytes and the fake home is already longer than that.
    let f = fake();
    let fifo = f.at("Library/Caches/realdir/pipe");
    let c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);

    let rule = vet(&fifo, DeleteMode::Trash, &f.ctx).unwrap_err().rule;
    assert_eq!(rule, RuleId::NotFileOrDir);
}

#[test]
fn our_own_data_directory_is_refused() {
    let mut f = fake();
    let own = f.at("Library/Application Support/dev.local.spacemaster");
    fs::create_dir_all(&own).unwrap();
    f.ctx.set_app_data_dir(own.clone());
    let rule = vet(&own, DeleteMode::Trash, &f.ctx).unwrap_err().rule;
    assert_eq!(rule, RuleId::OwnAppData);
}

// ---------------------------------------------------------------------------
// Property test: the invariant, rather than an enumeration of cases.
// ---------------------------------------------------------------------------

/// Component names chosen to collide with roots, deny entries, and near-misses,
/// so random joins land on interesting paths far more often than random strings
/// would.
const SEGMENTS: &[&str] = &[
    "Library",
    "Caches",
    "Caches-backup",
    "Documents",
    "Keychains",
    "CloudStorage",
    "Developer",
    "Xcode",
    "UserData",
    "DerivedData",
    "Containers",
    "com.apple.Safari",
    "com.example.app",
    ".npm",
    "_cacache",
    ".cargo",
    "registry",
    ".ssh",
    ".Trash",
    "pip",
    "Homebrew",
    "realdir",
    "..",
    ".",
    "olditem",
    "x",
];

proptest::proptest! {
    #[test]
    fn vetted_paths_always_satisfy_the_invariant(
        segments in proptest::collection::vec(
            proptest::sample::select(SEGMENTS), 0..6),
        permanent in proptest::bool::ANY,
    ) {
        let f = fake();
        let mode = if permanent { DeleteMode::Permanent } else { DeleteMode::Trash };
        let mut path = f.home.clone();
        for s in &segments {
            path.push(s);
        }

        if let Ok(target) = vet(&path, mode, &f.ctx) {
            let p = target.path();
            proptest::prop_assert_ne!(
                roots::classify(p, &f.home), Containment::Outside);
            proptest::prop_assert!(deny::containing_entry(p, &f.home).is_none());
            proptest::prop_assert!(deny::descendant_entry(p, &f.home).is_none());
            proptest::prop_assert_eq!(p.canonicalize().unwrap(), p);
            proptest::prop_assert!(p.starts_with(&f.home));
            if permanent {
                proptest::prop_assert!(quick::covers(p, &f.home));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Real home directory. Read-only, and opt-in via `cargo test -- --ignored`.
// ---------------------------------------------------------------------------

/// Paths under the real `$HOME` that must be refused. Nothing here is created,
/// read, or modified — only passed to `vet`.
const REAL_HOME_SENSITIVE: &[&str] = &[
    "Documents",
    "Desktop",
    "Downloads",
    "Pictures",
    "Movies",
    "Music",
    "Library",
    "Library/Keychains",
    "Library/Mobile Documents",
    "Library/CloudStorage",
    "Library/Mail",
    "Library/Messages",
    "Library/Safari",
    "Library/Photos",
    "Library/Preferences",
    "Library/Application Support",
    "Library/Application Support/MobileSync",
    "Library/Application Support/AddressBook",
    "Library/Developer",
    "Library/Developer/Xcode/UserData",
    "Library/Caches",
    ".ssh",
    ".gnupg",
    ".aws",
    ".config",
    ".rustup/toolchains",
];

#[test]
#[ignore = "reads the real home directory; run with --ignored"]
fn real_home_sensitive_paths_are_all_refused() {
    let ctx = GuardCtx::detect(None).expect("detect real home");
    let home = ctx.home().to_path_buf();

    for rel in REAL_HOME_SENSITIVE {
        let path = home.join(rel);
        for mode in [DeleteMode::Trash, DeleteMode::Permanent] {
            let result = vet(&path, mode, &ctx);
            assert!(
                result.is_err(),
                "{} was accepted for {:?}",
                path.display(),
                mode
            );
        }
    }
}

#[test]
#[ignore = "reads the real home directory; run with --ignored"]
fn real_filesystem_roots_are_all_refused() {
    let ctx = GuardCtx::detect(None).expect("detect real home");
    for p in [
        "/",
        "/Users",
        "/System",
        "/System/Library",
        "/Library",
        "/usr",
        "/etc",
        "/var",
        "/Applications",
        "/Volumes",
    ] {
        assert!(
            vet(Path::new(p), DeleteMode::Trash, &ctx).is_err(),
            "{p} was accepted"
        );
    }
    assert!(vet(ctx.home(), DeleteMode::Trash, &ctx).is_err());
}
