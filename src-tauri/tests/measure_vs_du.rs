//! Verification for the measurement engine.
//!
//! `du` is the reference implementation: it sums `st_blocks` and counts a
//! hard-linked inode once, which is exactly what we do. Agreeing with it is
//! therefore a real check, not a tautology — every way of getting this wrong
//! (counting `st_size`, skipping hidden files, double-counting links) makes the
//! two disagree.
//!
//! The synthetic-tree tests run by default. The comparison against the real
//! `~/Library/Caches` is `#[ignore]`d: it is slow, and the OS writes there while
//! we read, so it can only ever be a tolerance check.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use space_master_lib::fsutil::walk::{measure, IssueKind, MeasureCtx, Measurement};

fn ctx() -> MeasureCtx {
    MeasureCtx {
        pool: Arc::new(rayon::ThreadPoolBuilder::new().build().unwrap()),
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

fn run(root: &Path, ctx: &MeasureCtx) -> Measurement {
    measure(root, ctx, &mut |_| {})
}

/// `du -sk`, in bytes. Uses `-A`? No: plain `du` reports allocated blocks, which
/// is the number we want.
fn du_bytes(root: &Path) -> u64 {
    let out = Command::new("du")
        .arg("-sk")
        .arg(root)
        .output()
        .expect("run du");
    let text = String::from_utf8_lossy(&out.stdout);
    let kb: u64 = text
        .split_whitespace()
        .next()
        .expect("du output")
        .parse()
        .expect("du kilobytes");
    kb * 1024
}

/// Test trees live under `target/` rather than `/tmp` so paths stay on the same
/// volume as the build directory and survive `du` invocations unchanged.
fn scratch() -> tempfile::TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-trees");
    fs::create_dir_all(&base).expect("create scratch base");
    tempfile::TempDir::new_in(&base).expect("create scratch dir")
}

fn write_kb(path: &PathBuf, kb: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![b'x'; kb * 1024]).unwrap();
}

/// A tree exercising everything that makes measurement subtle.
fn build_tree(root: &Path) {
    write_kb(&root.join("plain.bin"), 64);
    write_kb(&root.join("nested/deep/inner.bin"), 128);

    // Hidden entries. `jwalk` would skip these with its default settings, and so
    // would we — silently, with no error and a plausible-looking total.
    write_kb(&root.join(".hidden-file.bin"), 32);
    write_kb(&root.join(".hidden-dir/payload.bin"), 96);

    // Hard links: three names, one inode, counted once.
    let target = root.join("linked/original.bin");
    write_kb(&target, 256);
    fs::create_dir_all(root.join("linked")).unwrap();
    fs::hard_link(&target, root.join("linked/alias-a.bin")).unwrap();
    fs::hard_link(&target, root.join("linked/alias-b.bin")).unwrap();

    // A sparse file: 8 MB logical, almost no blocks. Counting `st_size` here
    // would overstate the tree by an order of magnitude.
    let sparse = root.join("sparse.bin");
    let f = fs::File::create(&sparse).unwrap();
    f.set_len(8 * 1024 * 1024).unwrap();
    drop(f);

    // A symlink pointing outside the tree entirely.
    std::os::unix::fs::symlink("/System/Library", root.join("escape-link")).unwrap();
}

#[test]
fn measurement_matches_du_on_a_synthetic_tree() {
    let scratch = scratch();
    let root = scratch.path().canonicalize().unwrap();
    build_tree(&root);

    let m = run(&root, &ctx());
    let du = du_bytes(&root);

    // Both sides count the same blocks, so this should be exact. A tolerance is
    // allowed only for `du`'s kilobyte rounding.
    let diff = m.bytes.abs_diff(du);
    assert!(
        diff <= 1024,
        "measured {} bytes, du reports {} bytes (diff {})",
        m.bytes,
        du,
        diff
    );
    assert!(!m.cancelled);
}

#[test]
fn hidden_entries_are_counted() {
    let scratch = scratch();
    let root = scratch.path().canonicalize().unwrap();
    write_kb(&root.join("visible.bin"), 64);
    let visible_only = run(&root, &ctx()).bytes;

    write_kb(&root.join(".npm/_cacache/payload.bin"), 128);
    let with_hidden = run(&root, &ctx()).bytes;

    assert!(
        with_hidden >= visible_only + 128 * 1024,
        "hidden tree contributed only {} bytes",
        with_hidden - visible_only
    );
}

#[test]
fn hard_links_are_counted_once() {
    let scratch = scratch();
    let root = scratch.path().canonicalize().unwrap();
    let original = root.join("original.bin");
    write_kb(&original, 512);
    let single = run(&root, &ctx()).bytes;

    for i in 0..4 {
        fs::hard_link(&original, root.join(format!("alias-{i}.bin"))).unwrap();
    }
    let with_aliases = run(&root, &ctx());

    assert_eq!(with_aliases.bytes, single, "aliases inflated the total");
    assert_eq!(with_aliases.files, 5, "aliases should still be listed");
    assert_eq!(with_aliases.bytes, du_bytes(&root));
}

#[test]
fn sparse_file_is_measured_by_blocks_not_length() {
    let scratch = scratch();
    let root = scratch.path().canonicalize().unwrap();
    let sparse = root.join("sparse.bin");
    let f = fs::File::create(&sparse).unwrap();
    f.set_len(64 * 1024 * 1024).unwrap();
    drop(f);

    let m = run(&root, &ctx());
    assert!(
        m.bytes < 1024 * 1024,
        "a 64 MB sparse file was measured as {} bytes",
        m.bytes
    );
}

#[test]
fn symlink_is_reported_and_contributes_nothing() {
    let scratch = scratch();
    let root = scratch.path().canonicalize().unwrap();
    write_kb(&root.join("real.bin"), 64);
    let before = run(&root, &ctx()).bytes;

    // Pointing at /Volumes: following this would leave the tree, leave the
    // volume, and in the worst case wander into a network mount.
    std::os::unix::fs::symlink("/Volumes", root.join("volumes-link")).unwrap();
    let after = run(&root, &ctx());

    assert_eq!(after.bytes, before, "symlink changed the total");
    assert!(
        after
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::SymlinkSkipped && i.path.ends_with("volumes-link")),
        "symlink was skipped without being reported: {:?}",
        after.issues
    );
}

#[test]
fn symlinked_root_is_refused_rather_than_followed() {
    let scratch = scratch();
    let root = scratch.path().canonicalize().unwrap();
    write_kb(&root.join("payload.bin"), 64);
    let link = root.join("self-link");
    std::os::unix::fs::symlink(&root, &link).unwrap();

    let m = run(&link, &ctx());
    assert_eq!(m.bytes, 0);
    assert_eq!(m.issues.len(), 1);
    assert_eq!(m.issues[0].kind, IssueKind::SymlinkSkipped);
}

#[test]
fn cancellation_stops_the_walk_and_marks_the_result() {
    let scratch = scratch();
    let root = scratch.path().canonicalize().unwrap();
    for i in 0..40 {
        write_kb(&root.join(format!("dir-{i}/payload.bin")), 32);
    }

    let ctx = ctx();
    let cancel = Arc::clone(&ctx.cancel);
    // Cancel from inside the progress callback, after the first few entries.
    let mut seen = 0u64;
    let m = measure(&root, &ctx, &mut |_| {
        seen += 1;
        if seen == 3 {
            cancel.store(true, Ordering::Relaxed);
        }
    });

    assert!(m.cancelled, "cancelled scan was not flagged");
    let full = run(&root, &{
        let mut c = self::ctx();
        c.cancel = Arc::new(AtomicBool::new(false));
        c
    });
    assert!(
        m.bytes < full.bytes,
        "cancelled scan reported the full total ({} vs {})",
        m.bytes,
        full.bytes
    );
}

// ---------------------------------------------------------------------------
// Real directories. Opt-in via `cargo test -- --ignored`.
// ---------------------------------------------------------------------------

fn compare_against_du(rel: &str, tolerance: f64) {
    let root = std::env::home_dir().unwrap().join(rel);
    if !root.is_dir() {
        eprintln!("skipping {rel}: not present on this machine");
        return;
    }

    let m = run(&root, &ctx());
    let du = du_bytes(&root);
    let drift = (m.bytes as f64 - du as f64).abs() / du as f64;

    eprintln!(
        "{rel}: measured {:.2} GB, du {:.2} GB, drift {:.2}%, {} files, {} issues",
        m.bytes as f64 / 1e9,
        du as f64 / 1e9,
        drift * 100.0,
        m.files,
        m.issues.len()
    );
    assert!(
        drift <= tolerance,
        "{rel} drifted {:.2}% from du (limit {:.0}%)",
        drift * 100.0,
        tolerance * 100.0
    );
}

#[test]
#[ignore = "measures the real home directory; run with --ignored"]
fn library_caches_matches_du() {
    // 3% rather than exact: the OS writes into Caches continuously, and `du` and
    // this walk see the tree at slightly different moments.
    compare_against_du("Library/Caches", 0.03);
}

#[test]
#[ignore = "measures the real home directory; run with --ignored"]
fn developer_caches_match_du() {
    compare_against_du(".npm/_cacache", 0.03);
    compare_against_du("Library/Developer/Xcode/DerivedData", 0.03);
    compare_against_du("Library/Developer/Xcode/iOS DeviceSupport", 0.03);
}
