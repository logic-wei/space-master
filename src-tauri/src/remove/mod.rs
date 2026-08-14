//! Deletion. Every function here takes `&SafeTarget`, and only
//! [`crate::safety::guard::vet`] can produce one.

pub mod ledger;
pub mod permanent;
pub mod to_trash;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use crate::fsutil::volume::volume_info;
use crate::model::error::AppResult;
use crate::model::outcome::{CleanOutcome, FailureEntry, FailureKind, RemovedEntry};
use crate::safety::guard::{DeleteMode, Rejection, SafeTarget};
use crate::safety::privacy;

/// One vetted target plus the bookkeeping the ledger and the outcome need.
pub struct Job {
    pub item_id: String,
    pub bytes: u64,
    pub target: SafeTarget,
}

fn classify_trash(err: &trash::Error) -> FailureKind {
    match err {
        // EPERM and EACCES. On macOS this is almost always a TCC refusal rather than
        // file permissions — `~/.Trash` needs Full Disk Access.
        trash::Error::Os { code, .. } if *code == libc::EPERM || *code == libc::EACCES => {
            FailureKind::PermissionDenied
        }
        trash::Error::CouldNotAccess { .. } | trash::Error::CanonicalizePath { .. } => {
            FailureKind::Inaccessible
        }
        // NSFileManager reports a TCC refusal as a plain NSError, which the crate passes
        // through as `Unknown` with a sentence in it. Matching that sentence would be
        // matching localized prose, so the question is answered from the other side: if
        // we do not hold Full Disk Access, a refusal to move an app container is what
        // that looks like, and telling the user which switch to flick beats telling them
        // macOS reported an error.
        trash::Error::Unknown { .. } if !privacy::full_disk_access() => {
            FailureKind::PermissionDenied
        }
        _ => FailureKind::Failed,
    }
}

fn classify_io(err: &std::io::Error) -> FailureKind {
    match err.kind() {
        std::io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        std::io::ErrorKind::NotFound => FailureKind::Inaccessible,
        _ => FailureKind::Failed,
    }
}

/// Blocks a user can consume on the volume holding `probe`, or `None` if it cannot be
/// read. Only the difference between two of these is ever used.
fn available_on(probe: &Path) -> Option<u64> {
    volume_info(probe).ok().map(|v| v.available_bytes)
}

/// Runs a batch, recording each result before starting the next deletion.
///
/// Opening the ledger comes first and is fatal: deleting things we cannot write down
/// would leave the user with no way to find out what happened. Each deletion is
/// wrapped in `catch_unwind` so one panicking entry costs that entry and not the
/// batch — which is why the release profile keeps `panic = "unwind"`.
pub fn run(
    jobs: &[Job],
    mode: DeleteMode,
    rejected: Vec<Rejection>,
    ledger_dir: &Path,
) -> AppResult<CleanOutcome> {
    let mut ledger = ledger::Ledger::begin(ledger_dir, mode, jobs.len())?;
    let trash = to_trash::context();

    // Taken after the ledger exists, so the directory the probe reads is present, and
    // only for permanent mode: moving to the Trash frees nothing, so the difference
    // would be pure noise from whatever else is writing to the disk.
    let before = matches!(mode, DeleteMode::Permanent)
        .then(|| available_on(ledger_dir))
        .flatten();

    let mut removed = Vec::new();
    let mut failed = Vec::new();
    let mut bytes = 0u64;

    for job in jobs {
        let path = job.target.path();
        let attempt = catch_unwind(AssertUnwindSafe(|| match mode {
            DeleteMode::Trash => {
                to_trash::send(&trash, &job.target).map_err(|e| (classify_trash(&e), e.to_string()))
            }
            DeleteMode::Permanent => {
                permanent::remove(&job.target).map_err(|e| (classify_io(&e), e.to_string()))
            }
        }));

        match attempt {
            Ok(Ok(())) => {
                ledger.removed(&job.item_id, path, job.bytes)?;
                bytes += job.bytes;
                removed.push(RemovedEntry {
                    item_id: job.item_id.clone(),
                    path: path.to_path_buf(),
                    bytes: job.bytes,
                });
            }
            Ok(Err((kind, detail))) => {
                ledger.failed(path, &detail)?;
                failed.push(FailureEntry {
                    item_id: job.item_id.clone(),
                    path: path.to_path_buf(),
                    kind,
                    detail,
                });
            }
            Err(_) => {
                ledger.failed(path, "panic")?;
                failed.push(FailureEntry {
                    item_id: job.item_id.clone(),
                    path: path.to_path_buf(),
                    kind: FailureKind::Failed,
                    detail: "panic".to_string(),
                });
            }
        }
    }

    ledger.end(removed.len(), failed.len(), bytes)?;

    // What the volume actually gave back, which is the only figure the user can check
    // against `df`. It is normally *lower* than `bytes`: on APFS a file sharing blocks
    // with a clone elsewhere releases only the blocks nothing else references. It can
    // also be higher, because other processes keep writing while we work.
    let freed = before
        .zip(available_on(ledger_dir))
        .map(|(before, after)| after.saturating_sub(before));

    Ok(CleanOutcome {
        batch: ledger.batch().to_string(),
        mode,
        removed,
        rejected,
        failed,
        bytes,
        freed_bytes: freed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::guard::{vet, GuardCtx};

    /// A fake home under `target/`, not in a temp dir: `/var` is on the Guard's absolute
    /// deny list, so anything under `$TMPDIR` is refused before the test can begin.
    fn fake_home() -> tempfile::TempDir {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-homes");
        std::fs::create_dir_all(&base).expect("create test home base");
        tempfile::TempDir::new_in(&base).expect("create fake home")
    }

    /// Builds a vetted target inside a fake home, then moves the file out from under it
    /// so the deletion is guaranteed to fail. Failing on purpose is what keeps these
    /// tests out of the real `~/.Trash`: a successful trash operation from a unit test
    /// would leave debris in the user's Trash on every `cargo test`.
    fn doomed_job(home: &Path, name: &str) -> Job {
        let dir = home.join("Library/Caches");
        std::fs::create_dir_all(&dir).expect("fake caches");
        let path = dir.join(name);
        std::fs::write(&path, b"x").expect("write victim");

        let ctx = GuardCtx::for_test(home);
        let target = vet(&path, DeleteMode::Trash, &ctx).expect("vet the victim");

        // After vetting, so the target is real but the path no longer resolves.
        std::fs::rename(&path, dir.join(format!("{name}-moved"))).expect("move it away");

        Job {
            item_id: name.to_string(),
            bytes: 1,
            target,
        }
    }

    /// A vetted permanent-delete target. The path sits under a Tier A entry because
    /// R15 refuses `Permanent` anywhere else — a test that picked an arbitrary path
    /// would be testing the Guard's refusal, not the deletion.
    fn quick_job(home: &Path) -> Job {
        let path = home.join("Library/Caches/pip");
        std::fs::create_dir_all(path.join("wheels")).expect("fake pip cache");
        std::fs::write(path.join("wheels/a.whl"), vec![b'w'; 8192]).expect("write wheel");

        let ctx = GuardCtx::for_test(home);
        let target = vet(&path, DeleteMode::Permanent, &ctx).expect("vet the cache");
        Job {
            item_id: "pipCache".to_string(),
            bytes: 8192,
            target,
        }
    }

    #[test]
    fn a_permanent_delete_takes_the_whole_tree_and_measures_what_was_freed() {
        let dir = fake_home();
        let home = dir.path().canonicalize().expect("canonicalize fake home");
        let ledger_dir = tempfile::tempdir().expect("ledger dir");
        let jobs = vec![quick_job(&home)];
        let path = jobs[0].target.path().to_path_buf();

        let outcome =
            run(&jobs, DeleteMode::Permanent, Vec::new(), ledger_dir.path()).expect("run");

        assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
        assert_eq!(outcome.removed.len(), 1);
        assert!(!path.exists(), "{} survived", path.display());
        // The value depends on the filesystem's mood; that it was measured at all is
        // what the outcome promises for a permanent clean.
        assert!(outcome.freed_bytes.is_some());
    }

    #[test]
    fn trash_mode_reports_no_measurement_of_freed_space() {
        // Moving to the Trash on the same volume releases nothing. A number here would
        // be whatever else was writing to the disk, presented as our result.
        let dir = fake_home();
        let home = dir.path().canonicalize().expect("canonicalize fake home");
        let ledger_dir = tempfile::tempdir().expect("ledger dir");
        let jobs = vec![doomed_job(&home, "probe")];

        let outcome = run(&jobs, DeleteMode::Trash, Vec::new(), ledger_dir.path()).expect("run");

        assert!(outcome.freed_bytes.is_none());
    }

    #[test]
    fn a_failed_entry_does_not_stop_the_ones_after_it() {
        // The batch is the unit the user asked for, not the entry. Aborting on the first
        // failure would silently skip everything else they selected.
        let dir = fake_home();
        let home = dir.path().canonicalize().expect("canonicalize fake home");
        let ledger_dir = tempfile::tempdir().expect("ledger dir");
        let jobs = vec![doomed_job(&home, "first"), doomed_job(&home, "second")];

        let outcome = run(&jobs, DeleteMode::Trash, Vec::new(), ledger_dir.path())
            .expect("the batch itself succeeds");

        assert_eq!(outcome.failed.len(), 2);
        assert!(outcome.removed.is_empty());
        assert_eq!(outcome.bytes, 0);
    }

    #[test]
    fn a_batch_that_only_failed_is_still_closed() {
        // Otherwise every clean where nothing could be deleted would come back as an
        // "interrupted clean" warning on the next launch.
        let dir = fake_home();
        let home = dir.path().canonicalize().expect("canonicalize fake home");
        let ledger_dir = tempfile::tempdir().expect("ledger dir");
        let jobs = vec![doomed_job(&home, "only")];

        run(&jobs, DeleteMode::Trash, Vec::new(), ledger_dir.path()).expect("run");

        assert!(ledger::unfinished(ledger_dir.path())
            .expect("read ledger")
            .is_empty());
    }

    #[test]
    fn refusals_from_the_preview_survive_into_the_outcome() {
        // The outcome is the only report the user sees. A path the Guard refused has to
        // appear there, or the clean looks like it covered the whole selection.
        let dir = fake_home();
        let ledger_dir = tempfile::tempdir().expect("ledger dir");
        let rejected = vec![Rejection {
            path: dir.path().join("Documents"),
            rule: crate::safety::guard::RuleId::Protected,
            detail: None,
        }];

        let outcome = run(&[], DeleteMode::Trash, rejected, ledger_dir.path()).expect("run");

        assert_eq!(outcome.rejected.len(), 1);
    }
}
