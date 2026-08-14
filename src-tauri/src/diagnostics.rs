//! Diagnostics for a build that has nowhere to print.
//!
//! A bundled `.app` is launched with no terminal attached, so a panic message goes
//! nowhere at all: the window simply stops doing anything. The delete loop already
//! survives a panicking entry — each one is wrapped in `catch_unwind` — but all it can
//! record is that a panic happened, which is not enough to fix one.

use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const PANIC_FILE: &str = "panic.log";

/// Appends every panic to `panic.log`, then lets the previous hook run.
///
/// The file sits next to the ledger, which is the one directory this app refuses to
/// delete (Guard rule R11). `~/Library/Logs` would be the conventional place and is
/// also a one-click clean target — a log the app deletes is a log that is empty when
/// it is needed.
pub fn install_panic_log(dir: &Path) {
    let dir = dir.to_path_buf();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // `PanicHookInfo`'s own formatting already carries the message and the source
        // location, which is the whole of what there is to know here.
        let _ = append(&dir, &info.to_string());
        previous(info);
    }));
}

/// Separate from the hook so it can be tested without installing one: a panic hook is
/// process-wide state, and a test that swaps it out affects every other test.
fn append(dir: &Path, line: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(PANIC_FILE))?;
    // Epoch milliseconds, matching the ledger's stamps: the point of this file is to be
    // lined up against the batch that was running at the time.
    let at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    // One line per panic, newlines in the message flattened, so the file stays readable
    // as a list even after several.
    writeln!(file, "{at_ms} {}", line.replace('\n', " | "))?;
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panics_are_appended_rather_than_overwriting_each_other() {
        let dir = tempfile::tempdir().unwrap();
        append(dir.path(), "panicked at src/a.rs:1:2:\nfirst").unwrap();
        append(dir.path(), "panicked at src/b.rs:3:4:\nsecond").unwrap();

        let written = std::fs::read_to_string(dir.path().join(PANIC_FILE)).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("first"), "{}", lines[0]);
        assert!(lines[1].contains("second"), "{}", lines[1]);
    }

    #[test]
    fn a_missing_directory_is_created_rather_than_losing_the_panic() {
        // The data directory is created when the ledger is first opened, which on a
        // fresh install has not happened by the time something can panic.
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("not-yet");
        append(&nested, "panicked at src/a.rs:1:2: boom").unwrap();
        assert!(nested.join(PANIC_FILE).exists());
    }
}
