use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tauri::State;

use crate::model::error::{AppError, AppResult};
use crate::model::item::Target;
use crate::model::outcome::CleanOutcome;
use crate::model::plan::{CleanPlan, PlanEntry};
use crate::remove::ledger::{self, UnfinishedBatch};
use crate::remove::{self, Job};
use crate::safety::guard::{vet_all, DeleteMode, GuardCtx};
use crate::state::{AppState, StoredPlan};

/// Turns a selection into a vetted, reviewable plan. Nothing is deleted.
///
/// `item_ids` refer to the scan identified by `generation`; no parameter names a
/// path.
#[tauri::command]
pub async fn preview_clean(
    state: State<'_, AppState>,
    generation: u64,
    item_ids: Vec<String>,
    mode: DeleteMode,
) -> AppResult<CleanPlan> {
    let resolved = state
        .index
        .lock()
        .expect("item index lock")
        .resolve(generation, &item_ids)?;

    // Detected per preview rather than cached: R10 asks whether an app is running
    // *now*, and the user may well have quit one after seeing it flagged.
    let ctx = GuardCtx::detect(state.app_data_dir.clone())?;

    // Reserved before the plan is built so the token can be embedded in it, and
    // without holding the lock across the Guard's filesystem work.
    let token = state.plans.lock().expect("plans lock").reserve();
    let plan = build_plan(token, generation, &resolved, mode, &ctx);

    // Only what the Guard accepted is executable. A rejected entry must not become
    // deletable just because it was in the selection.
    let entries = plan
        .accepted
        .iter()
        .map(|e| {
            (
                e.item_id.clone(),
                Target {
                    path: e.path.clone(),
                    bytes: e.bytes,
                },
            )
        })
        .collect();
    state.plans.lock().expect("plans lock").store(
        token,
        StoredPlan {
            generation,
            mode,
            entries,
        },
    );

    Ok(plan)
}

/// Carries out a previously previewed plan. The token is the only parameter: the paths
/// come from what this process vetted, not from anything the frontend can say.
///
/// The plan is vetted a *second* time here. Between approving a plan and running it an
/// app can launch, a directory can be swapped for a symlink, or a path can vanish —
/// and the check that matters is the one closest to the syscall.
#[tauri::command]
pub async fn execute_clean(state: State<'_, AppState>, token: u64) -> AppResult<CleanOutcome> {
    let plan = state.plans.lock().expect("plans lock").take(token)?;
    let ledger_dir = state.ledger_dir()?.clone();
    let ctx = GuardCtx::detect(state.app_data_dir.clone())?;
    let mode = plan.mode;

    let (jobs, rejected) = {
        let paths: Vec<PathBuf> = plan.entries.iter().map(|(_, t)| t.path.clone()).collect();
        let meta: HashMap<&Path, (&str, u64)> = plan
            .entries
            .iter()
            .map(|(id, t)| (t.path.as_path(), (id.as_str(), t.bytes)))
            .collect();
        let (targets, rejected) = vet_all(&paths, mode, &ctx);
        let jobs: Vec<Job> = targets
            .into_iter()
            .map(|target| {
                let (item_id, bytes) = meta
                    .get(target.path())
                    .expect("vetted path came from the stored plan");
                Job {
                    item_id: (*item_id).to_string(),
                    bytes: *bytes,
                    target,
                }
            })
            .collect();
        (jobs, rejected)
    };

    // Trashing is a rename per entry, but a batch of them is still syscalls that must
    // not run on a tokio worker.
    tauri::async_runtime::spawn_blocking(move || remove::run(&jobs, mode, rejected, &ledger_dir))
        .await
        .map_err(|e| AppError::Scan(e.to_string()))?
}

/// Batches whose closing record never arrived, i.e. the app stopped mid-delete.
#[tauri::command]
pub fn unfinished_batches(state: State<'_, AppState>) -> AppResult<Vec<UnfinishedBatch>> {
    Ok(ledger::unfinished(state.ledger_dir()?)?)
}

/// Vets every resolved path and pairs the survivors back up with the item and size
/// they came from.
///
/// Separate from the command so the Guard's presence in this path is covered by a
/// test: a plan builder that forgot to call [`vet_all`] would still produce a
/// perfectly plausible-looking plan.
pub fn build_plan(
    token: u64,
    generation: u64,
    resolved: &[(String, Target)],
    mode: DeleteMode,
    ctx: &GuardCtx,
) -> CleanPlan {
    let paths: Vec<PathBuf> = resolved.iter().map(|(_, t)| t.path.clone()).collect();
    let meta: HashMap<&Path, (&str, u64)> = resolved
        .iter()
        .map(|(id, t)| (t.path.as_path(), (id.as_str(), t.bytes)))
        .collect();

    let (targets, rejected) = vet_all(&paths, mode, ctx);

    let accepted: Vec<PlanEntry> = targets
        .iter()
        .map(|t| {
            let (item_id, bytes) = meta
                .get(t.path())
                // `vet_all` only ever returns paths it was given.
                .expect("vetted path came from the resolved set");
            PlanEntry {
                item_id: (*item_id).to_string(),
                path: t.path().to_path_buf(),
                bytes: *bytes,
                is_dir: t.is_dir(),
            }
        })
        .collect();

    CleanPlan {
        token,
        generation,
        mode,
        estimated_bytes: accepted.iter().map(|e| e.bytes).sum(),
        accepted,
        rejected,
    }
}
