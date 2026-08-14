//! Process-wide state behind the commands.
//!
//! The interesting part is [`ItemIndex`]. It is what makes "the frontend cannot
//! name a path" true: `preview_clean` receives ids, and the only place an id can
//! resolve to a path is a scan this process performed. An id that is not in the
//! current index resolves to nothing at all.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::model::error::{AppError, AppResult};
use crate::model::item::{ScannedItem, Target};
use crate::safety::guard::DeleteMode;
use crate::scan::session::Scanner;

/// Paths from the most recent scan, keyed by item id.
#[derive(Default)]
pub struct ItemIndex {
    generation: u64,
    items: HashMap<String, Vec<Target>>,
}

impl ItemIndex {
    pub fn replace(&mut self, generation: u64, scanned: &[ScannedItem]) {
        self.generation = generation;
        self.items = scanned
            .iter()
            .map(|s| (s.item.id.clone(), s.targets.clone()))
            .collect();
    }

    /// Resolves ids to paths, refusing the whole batch if the report they came from
    /// is not the current one. A partial resolution would delete some of what the
    /// user selected and silently drop the rest.
    pub fn resolve(&self, generation: u64, ids: &[String]) -> AppResult<Vec<(String, Target)>> {
        if generation != self.generation {
            return Err(AppError::StaleScan);
        }
        let mut out = Vec::new();
        for id in ids {
            let targets = self.items.get(id).ok_or(AppError::StaleScan)?;
            out.extend(targets.iter().map(|t| (id.clone(), t.clone())));
        }
        Ok(out)
    }
}

/// Paths from the most recent orphan scan, for revealing a row in Finder.
///
/// Separate from [`ItemIndex`], which holds only what may be deleted. Showing a row is
/// not deleting it, and the rows most worth looking at are the protected ones the index
/// deliberately does not carry.
#[derive(Default)]
pub struct RevealIndex {
    generation: u64,
    items: HashMap<String, Vec<PathBuf>>,
}

impl RevealIndex {
    pub fn replace(&mut self, generation: u64, items: HashMap<String, Vec<PathBuf>>) {
        self.generation = generation;
        self.items = items;
    }

    /// The path at `place` for `id`, or an error. Same generation check as `resolve`: an
    /// id from a superseded report may now mean a different directory.
    pub fn path(&self, generation: u64, id: &str, place: usize) -> AppResult<PathBuf> {
        if generation != self.generation {
            return Err(AppError::StaleScan);
        }
        self.items
            .get(id)
            .and_then(|paths| paths.get(place))
            .cloned()
            .ok_or(AppError::StaleScan)
    }
}

/// The paths a previewed plan covers, held so `execute_clean` needs only a token.
pub struct StoredPlan {
    pub generation: u64,
    pub mode: DeleteMode,
    pub entries: Vec<(String, Target)>,
}

/// At most one plan is alive at a time, because at most one is on screen. A second
/// preview supersedes the first rather than accumulating executable plans.
#[derive(Default)]
pub struct Plans {
    next_token: u64,
    current: Option<(u64, StoredPlan)>,
}

impl Plans {
    /// Allocates the token for a plan about to be built, so it can be embedded in the
    /// plan the frontend receives.
    pub fn reserve(&mut self) -> u64 {
        self.next_token += 1;
        self.next_token
    }

    pub fn store(&mut self, token: u64, plan: StoredPlan) {
        self.current = Some((token, plan));
    }

    /// Consumes the plan. One-shot: a double-click on Clean must not delete twice, and
    /// the second attempt should say so rather than quietly find nothing to do.
    pub fn take(&mut self, token: u64) -> AppResult<StoredPlan> {
        match self.current.take() {
            Some((held, plan)) if held == token => Ok(plan),
            // Put it back: a stale token must not discard the plan the user is looking at.
            Some(other) => {
                self.current = Some(other);
                Err(AppError::StalePlan)
            }
            None => Err(AppError::StalePlan),
        }
    }
}

pub struct AppState {
    pub scanner: Arc<Scanner>,
    pub home: PathBuf,
    /// Passed to the Guard for R11 so we cannot delete our own data directory, and
    /// used as the ledger's home.
    pub app_data_dir: Option<PathBuf>,
    pub index: Arc<Mutex<ItemIndex>>,
    pub reveal: Arc<Mutex<RevealIndex>>,
    pub plans: Arc<Mutex<Plans>>,
}

impl AppState {
    pub fn new(home: PathBuf, app_data_dir: Option<PathBuf>) -> AppResult<Self> {
        Ok(Self {
            scanner: Arc::new(Scanner::new()?),
            home,
            app_data_dir,
            index: Arc::new(Mutex::new(ItemIndex::default())),
            reveal: Arc::new(Mutex::new(RevealIndex::default())),
            plans: Arc::new(Mutex::new(Plans::default())),
        })
    }

    /// Where the ledger lives. Deleting without being able to record it is refused, so
    /// a missing data directory is an error rather than a silent downgrade.
    pub fn ledger_dir(&self) -> AppResult<&PathBuf> {
        self.app_data_dir
            .as_ref()
            .ok_or_else(|| AppError::InvalidPath("appDataDir".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::item::{ItemScope, ScanItem};

    fn scanned(id: &str, paths: &[&str], bytes: u64) -> ScannedItem {
        ScannedItem {
            item: ScanItem {
                id: id.to_string(),
                path: PathBuf::from(paths[0]),
                bytes,
                files: 1,
                last_used_ms: None,
                scope: ItemScope::SelfDir,
                note: None,
                issues: Vec::new(),
            },
            targets: paths
                .iter()
                .map(|p| Target {
                    path: PathBuf::from(p),
                    bytes,
                })
                .collect(),
        }
    }

    fn index() -> ItemIndex {
        let mut index = ItemIndex::default();
        index.replace(
            3,
            &[
                scanned("pipCache", &["/home/u/Library/Caches/pip"], 100),
                scanned("trash", &["/home/u/.Trash/a", "/home/u/.Trash/b"], 50),
            ],
        );
        index
    }

    #[test]
    fn an_item_resolves_to_every_path_it_covers() {
        let resolved = index().resolve(3, &["trash".to_string()]).unwrap();
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().all(|(id, _)| id == "trash"));
    }

    #[test]
    fn a_stale_generation_resolves_to_nothing() {
        // The window this closes: the user scans, a rescan bumps the generation, and
        // the still-visible old report is acted on. Its ids may now point elsewhere.
        assert!(index().resolve(2, &["pipCache".to_string()]).is_err());
    }

    fn stored(mode: DeleteMode) -> StoredPlan {
        StoredPlan {
            generation: 3,
            mode,
            entries: vec![(
                "pipCache".to_string(),
                Target {
                    path: PathBuf::from("/home/u/Library/Caches/pip"),
                    bytes: 100,
                },
            )],
        }
    }

    #[test]
    fn a_plan_can_only_be_executed_once() {
        // What this closes: a double-click on Clean, or a retry after a slow response.
        let mut plans = Plans::default();
        let token = plans.reserve();
        plans.store(token, stored(DeleteMode::Trash));

        assert!(plans.take(token).is_ok());
        assert!(plans.take(token).is_err());
    }

    #[test]
    fn a_stale_token_does_not_discard_the_plan_on_screen() {
        // The old token must fail *without* consuming the current plan, or a stray
        // retry from a superseded preview would disarm the button the user is looking at.
        let mut plans = Plans::default();
        let first = plans.reserve();
        plans.store(first, stored(DeleteMode::Trash));
        let second = plans.reserve();
        plans.store(second, stored(DeleteMode::Trash));

        assert!(plans.take(first).is_err());
        assert!(plans.take(second).is_ok());
    }

    #[test]
    fn an_unknown_id_fails_the_whole_batch() {
        // Resolving the rest would delete part of the selection and drop the remainder
        // without saying so.
        let err = index().resolve(3, &["pipCache".to_string(), "nope".to_string()]);
        assert!(err.is_err());
    }
}
