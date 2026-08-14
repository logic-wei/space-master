//! The loop every catalog scan runs: measure each row's paths, stream progress,
//! hand back the paths a clean would act on.
//!
//! Resolution happens before this, measurement inside it. The split exists because
//! resolving a row differs per catalog — a Tier A entry may expand to its children,
//! a `~/.cache` row is discovered by listing — while the measuring, throttling and
//! cancellation are identical and are the parts that are easy to get subtly wrong.

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use crate::fsutil::walk::{measure, MeasureCtx, ScanIssue};
use crate::model::item::{ItemScope, ScanItem, ScannedItem, Target};
use crate::model::report::ScanEvent;
use crate::scan::progress::Throttle;
use crate::scan::session::ScanHandle;

/// One row to measure, already resolved to the paths it covers.
pub struct Spec {
    /// Stable id, also the i18n key suffix when the catalog has wording for it.
    pub id: String,
    /// Shown to the user. For a `Children`-scoped row this is the directory itself,
    /// not the children that would be deleted.
    pub path: PathBuf,
    pub scope: ItemScope,
    pub targets: Vec<PathBuf>,
    /// Wording shared with the rest of the rows this one was discovered alongside.
    /// See [`ScanItem::note`].
    pub note: Option<&'static str>,
    /// Anything that went wrong while resolving `targets`, carried onto the row.
    /// Without it, a directory we could not list is indistinguishable from an empty
    /// one.
    pub issues: Vec<ScanIssue>,
}

impl Spec {
    /// A row that is exactly one directory, deleted whole.
    pub fn whole(id: String, path: PathBuf) -> Self {
        Self {
            id,
            targets: vec![path.clone()],
            path,
            scope: ItemScope::SelfDir,
            note: None,
            issues: Vec::new(),
        }
    }

    pub fn with_note(mut self, note: &'static str) -> Self {
        self.note = Some(note);
        self
    }
}

pub fn measure_all(
    handle: &ScanHandle,
    group: &'static str,
    specs: Vec<Spec>,
    ctx: &MeasureCtx,
    emit: &mut dyn FnMut(ScanEvent),
) -> Vec<ScannedItem> {
    let mut throttle = Throttle::default();
    let mut scanned = Vec::new();
    let mut total = 0u64;

    for spec in specs {
        if handle.cancel.load(Ordering::Relaxed) {
            break;
        }
        let id = spec.id.clone();
        let mut item = ScanItem {
            id: spec.id,
            path: spec.path,
            bytes: 0,
            files: 0,
            last_used_ms: None,
            scope: spec.scope,
            note: spec.note,
            issues: spec.issues,
        };
        let mut measured = Vec::with_capacity(spec.targets.len());

        for target in spec.targets {
            let base = total;
            let m = measure(&target, ctx, &mut |partial| {
                let running = base + partial.bytes;
                if throttle.admit(running) {
                    emit(ScanEvent::Progress {
                        generation: handle.generation,
                        group,
                        item_id: id.clone(),
                        bytes: running,
                    });
                }
            });
            total += m.bytes;
            item.bytes += m.bytes;
            item.files += m.files;
            if let Some(used) = m.last_used_ms {
                item.last_used_ms = Some(item.last_used_ms.map_or(used, |seen| seen.max(used)));
            }
            item.issues.extend(m.issues);
            measured.push(Target {
                path: target,
                bytes: m.bytes,
            });
        }

        // The row is about to be rendered with final numbers, so let the next
        // progress tick through immediately rather than waiting out the interval.
        throttle.reset();
        emit(ScanEvent::ItemDone {
            generation: handle.generation,
            group,
            item: item.clone(),
        });
        scanned.push(ScannedItem {
            item,
            targets: measured,
        });
    }

    scanned
}
