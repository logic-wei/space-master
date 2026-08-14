//! Append-only record of what this app deleted.
//!
//! Finder's "Put Back" is the recovery path for Trash-mode deletes — verified on
//! macOS 26.4, contrary to the `trash` crate's own documentation. So this is an
//! audit log, not an undo stack: its job is to answer "what did SpaceMaster delete,
//! and when", including after a crash halfway through a batch.
//!
//! That last part is why every record is flushed before the next deletion begins. A
//! buffered ledger would lose exactly the records that matter most — the ones written
//! just before things went wrong.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::safety::guard::DeleteMode;

pub const LEDGER_FILE: &str = "ledger.jsonl";

/// One line of the ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Record {
    BatchStart {
        batch: String,
        at_ms: u64,
        mode: DeleteMode,
        planned: usize,
    },
    Removed {
        batch: String,
        at_ms: u64,
        item_id: String,
        path: PathBuf,
        bytes: u64,
    },
    Failed {
        batch: String,
        at_ms: u64,
        path: PathBuf,
        /// The OS error, untranslated. Diagnostics, not UI copy.
        detail: String,
    },
    BatchEnd {
        batch: String,
        at_ms: u64,
        removed: usize,
        failed: usize,
        bytes: u64,
    },
}

/// A batch whose `BatchEnd` never arrived: the app stopped mid-delete.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnfinishedBatch {
    pub batch: String,
    pub at_ms: u64,
    pub mode: DeleteMode,
    /// How many deletions we know completed. The true figure can be one higher: the
    /// process may have died between the deletion and its record.
    pub removed: usize,
    pub bytes: u64,
}

/// One batch, reassembled from the lines that mention it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub batch: String,
    pub at_ms: u64,
    pub mode: DeleteMode,
    /// How many entries the batch set out to delete. Higher than `removed.len()` plus
    /// `failed.len()` only when the app stopped partway.
    pub planned: usize,
    pub removed: Vec<RemovedRecord>,
    pub failed: Vec<FailedRecord>,
    pub bytes: u64,
    /// False when the closing record never arrived.
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedRecord {
    pub item_id: String,
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedRecord {
    pub path: PathBuf,
    /// The OS error verbatim. Diagnostics, not UI copy — and the one string in this
    /// app that reaches the screen untranslated, because inventing wording for it
    /// would mean guessing what macOS meant.
    pub detail: String,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Distinguishes batches that start within the same millisecond.
static SEQ: AtomicU64 = AtomicU64::new(0);

pub struct Ledger {
    file: File,
    batch: String,
}

impl Ledger {
    /// Opens the ledger in `dir` and writes the opening record for a new batch.
    pub fn begin(dir: &Path, mode: DeleteMode, planned: usize) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(LEDGER_FILE))?;
        let at_ms = now_ms();
        let batch = format!("{at_ms}-{}", SEQ.fetch_add(1, Ordering::Relaxed));
        let mut ledger = Self { file, batch };
        ledger.write(&Record::BatchStart {
            batch: ledger.batch.clone(),
            at_ms,
            mode,
            planned,
        })?;
        Ok(ledger)
    }

    pub fn batch(&self) -> &str {
        &self.batch
    }

    pub fn removed(&mut self, item_id: &str, path: &Path, bytes: u64) -> std::io::Result<()> {
        self.write(&Record::Removed {
            batch: self.batch.clone(),
            at_ms: now_ms(),
            item_id: item_id.to_string(),
            path: path.to_path_buf(),
            bytes,
        })
    }

    pub fn failed(&mut self, path: &Path, detail: &str) -> std::io::Result<()> {
        self.write(&Record::Failed {
            batch: self.batch.clone(),
            at_ms: now_ms(),
            path: path.to_path_buf(),
            detail: detail.to_string(),
        })
    }

    pub fn end(&mut self, removed: usize, failed: usize, bytes: u64) -> std::io::Result<()> {
        self.write(&Record::BatchEnd {
            batch: self.batch.clone(),
            at_ms: now_ms(),
            removed,
            failed,
            bytes,
        })
    }

    fn write(&mut self, record: &Record) -> std::io::Result<()> {
        let mut line = serde_json::to_vec(record).map_err(std::io::Error::other)?;
        line.push(b'\n');
        self.file.write_all(&line)?;
        // Before the next deletion, not after the batch: see the module docs.
        self.file.flush()
    }
}

/// Every batch the ledger holds, newest first.
///
/// The only reader of the file, so there is one answer to "what did this app delete"
/// rather than two that can disagree. A malformed line is skipped rather than aborting
/// the read: the ledger is append-only and flushed per record, so the only way to get
/// one is a partial write at the moment of a crash — which is precisely when we still
/// want the rest.
///
/// Counts are recomputed from the individual records rather than taken from the closing
/// one. They agree unless a record was lost, and when they disagree the records are the
/// ones that name actual paths.
pub fn history(dir: &Path) -> std::io::Result<Vec<BatchSummary>> {
    let file = match File::open(dir.join(LEDGER_FILE)) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let mut batches: Vec<BatchSummary> = Vec::new();
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { continue };
        let Ok(record) = serde_json::from_str::<Record>(&line) else {
            continue;
        };
        // A record whose opening line was lost is dropped: there is nothing to say
        // about which batch it belonged to.
        match record {
            Record::BatchStart {
                batch,
                at_ms,
                mode,
                planned,
            } => batches.push(BatchSummary {
                batch,
                at_ms,
                mode,
                planned,
                removed: Vec::new(),
                failed: Vec::new(),
                bytes: 0,
                finished: false,
            }),
            Record::Removed {
                batch,
                item_id,
                path,
                bytes,
                ..
            } => {
                if let Some(b) = batches.iter_mut().find(|b| b.batch == batch) {
                    b.bytes += bytes;
                    b.removed.push(RemovedRecord {
                        item_id,
                        path,
                        bytes,
                    });
                }
            }
            Record::Failed {
                batch,
                path,
                detail,
                ..
            } => {
                if let Some(b) = batches.iter_mut().find(|b| b.batch == batch) {
                    b.failed.push(FailedRecord { path, detail });
                }
            }
            Record::BatchEnd { batch, .. } => {
                if let Some(b) = batches.iter_mut().find(|b| b.batch == batch) {
                    b.finished = true;
                }
            }
        }
    }

    // Recent first: the batch a user came here to check is the last one they ran.
    batches.reverse();
    Ok(batches)
}

/// Batches that were started but never closed, i.e. the app stopped mid-delete.
pub fn unfinished(dir: &Path) -> std::io::Result<Vec<UnfinishedBatch>> {
    Ok(history(dir)?
        .into_iter()
        .filter(|b| !b.finished)
        .map(|b| UnfinishedBatch {
            batch: b.batch,
            at_ms: b.at_ms,
            mode: b.mode,
            removed: b.removed.len(),
            bytes: b.bytes,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_batch_is_not_reported_as_unfinished() {
        let dir = tempfile::tempdir().unwrap();
        let mut l = Ledger::begin(dir.path(), DeleteMode::Trash, 2).unwrap();
        l.removed("pipCache", Path::new("/tmp/a"), 10).unwrap();
        l.removed("pipCache", Path::new("/tmp/b"), 20).unwrap();
        l.end(2, 0, 30).unwrap();

        assert!(unfinished(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn a_batch_that_stopped_midway_is_reported_with_what_completed() {
        // Simulates the crash case: records exist, `end` never ran.
        let dir = tempfile::tempdir().unwrap();
        let mut l = Ledger::begin(dir.path(), DeleteMode::Trash, 3).unwrap();
        l.removed("appLogs", Path::new("/tmp/a"), 100).unwrap();
        drop(l);

        let open = unfinished(dir.path()).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].removed, 1);
        assert_eq!(open[0].bytes, 100);
    }

    #[test]
    fn a_missing_ledger_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(unfinished(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn a_truncated_final_line_does_not_discard_the_records_before_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut l = Ledger::begin(dir.path(), DeleteMode::Trash, 2).unwrap();
        l.removed("pipCache", Path::new("/tmp/a"), 10).unwrap();
        drop(l);
        // A half-written record, as a crash mid-`write_all` would leave behind.
        let mut f = OpenOptions::new()
            .append(true)
            .open(dir.path().join(LEDGER_FILE))
            .unwrap();
        f.write_all(br#"{"kind":"removed","batch":"#).unwrap();
        drop(f);

        let open = unfinished(dir.path()).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].removed, 1);
    }

    #[test]
    fn history_reads_back_what_a_batch_deleted_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut older = Ledger::begin(dir.path(), DeleteMode::Permanent, 1).unwrap();
        older.removed("pipCache", Path::new("/tmp/a"), 10).unwrap();
        older.end(1, 0, 10).unwrap();

        let mut newer = Ledger::begin(dir.path(), DeleteMode::Trash, 2).unwrap();
        newer.removed("appLogs", Path::new("/tmp/b"), 20).unwrap();
        newer.failed(Path::new("/tmp/c"), "no permission").unwrap();
        newer.end(1, 1, 20).unwrap();

        let all = history(dir.path()).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].batch, newer.batch());
        assert_eq!(all[0].mode, DeleteMode::Trash);
        assert_eq!(all[0].planned, 2);
        assert_eq!(all[0].removed.len(), 1);
        assert_eq!(all[0].removed[0].path, Path::new("/tmp/b"));
        assert_eq!(all[0].failed.len(), 1);
        assert_eq!(all[0].bytes, 20);
        assert!(all[0].finished);
        assert_eq!(all[1].mode, DeleteMode::Permanent);
    }

    #[test]
    fn two_batches_started_in_the_same_millisecond_get_distinct_ids() {
        let dir = tempfile::tempdir().unwrap();
        let a = Ledger::begin(dir.path(), DeleteMode::Trash, 1).unwrap();
        let b = Ledger::begin(dir.path(), DeleteMode::Trash, 1).unwrap();
        assert_ne!(a.batch(), b.batch());
    }
}
