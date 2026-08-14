//! The single gate every deletion passes through.
//!
//! `SafeTarget` has private fields and no public constructor, so [`vet`] is the
//! only way to obtain one. Everything in `remove/` accepts `&SafeTarget` and
//! nothing else, which makes "delete a path that was never checked"
//! unrepresentable rather than merely discouraged.
//!
//! Paths travel *outward* only. A `Rejection` carries the path it refused so the
//! UI can show what was skipped and why — that is what makes the tool auditable.
//! No command accepts a path as input; the frontend sends item ids and a plan
//! token, so it cannot name a target of its own.

use std::collections::HashSet;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::deny;
use super::roots::{self, Containment};
use super::running_apps::RunningApps;
use crate::catalog::quick;
use crate::model::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeleteMode {
    /// Recoverable: the item lands in `~/.Trash`.
    Trash,
    /// Unrecoverable. Restricted by R15 to the Tier A catalog.
    Permanent,
}

/// Why a candidate was refused. A stable code, not a sentence: the frontend maps
/// it to localized wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleId {
    /// R16 — path contains a NUL byte and cannot reach a syscall intact.
    NulByte,
    /// R1 — path is relative.
    NotAbsolute,
    /// R2 — path contains `..` or another non-literal component.
    NonNormalComponent,
    /// R7 — too close to the top of the filesystem, or contains `$HOME`.
    TooShallow,
    /// R6 — not inside any allowed root.
    OutsideRoots,
    /// R6 — is an allowed root, but that root only permits deleting children.
    RootNotDeletable,
    /// R8 — matches a deny entry.
    Protected,
    /// R9 — would take a deny entry with it.
    WouldTakeProtected,
    /// R8 — belongs to an OS bundle identifier.
    SystemBundle,
    /// R11 — is this app's own data.
    OwnAppData,
    /// R3 — does not exist.
    Missing,
    /// R4 — is a symlink. We neither follow it nor delete the link.
    Symlink,
    /// R14 — socket, fifo, or device node.
    NotFileOrDir,
    /// R12 — lives on a different volume than `$HOME`.
    OtherVolume,
    /// R5 — resolves to a different path than it names.
    PathAliased,
    /// R10 — belongs to a running application.
    AppRunning,
    /// R15 — permanent deletion requested for a path outside the Tier A catalog.
    PermanentNotAllowed,
    /// R13 — another candidate in the same batch is an ancestor or descendant.
    Overlapping,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rejection {
    pub path: PathBuf,
    pub rule: RuleId,
    /// Machine-readable context — the conflicting path, or a bundle id. Never
    /// prose.
    pub detail: Option<String>,
}

impl Rejection {
    fn new(path: &Path, rule: RuleId) -> Self {
        Self {
            path: path.to_path_buf(),
            rule,
            detail: None,
        }
    }

    fn with(path: &Path, rule: RuleId, detail: impl ToString) -> Self {
        Self {
            path: path.to_path_buf(),
            rule,
            detail: Some(detail.to_string()),
        }
    }
}

/// A path that passed every rule. Fields are private on purpose: see the module
/// docs.
#[derive(Debug, Clone)]
pub struct SafeTarget {
    path: PathBuf,
    mode: DeleteMode,
    is_dir: bool,
}

impl SafeTarget {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn mode(&self) -> DeleteMode {
        self.mode
    }

    pub fn is_dir(&self) -> bool {
        self.is_dir
    }
}

pub struct GuardCtx {
    home: PathBuf,
    home_dev: u64,
    app_data_dir: Option<PathBuf>,
    running: RunningApps,
}

impl GuardCtx {
    pub fn detect(app_data_dir: Option<PathBuf>) -> AppResult<Self> {
        let home = std::env::home_dir()
            .ok_or_else(|| AppError::InvalidPath("$HOME".to_string()))?
            .canonicalize()?;
        Ok(Self {
            home_dev: std::fs::symlink_metadata(&home)?.dev(),
            home,
            // May not exist yet on first launch, so a failed canonicalize is not
            // fatal — the literal path still blocks R11.
            app_data_dir: app_data_dir.map(|p| p.canonicalize().unwrap_or(p)),
            running: RunningApps::detect(),
        })
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    #[cfg(test)]
    pub fn for_test(home: &Path) -> Self {
        let home = home
            .canonicalize()
            .expect("test home directory must exist and be canonical");
        Self {
            home_dev: std::fs::symlink_metadata(&home)
                .expect("stat test home")
                .dev(),
            home,
            app_data_dir: None,
            running: RunningApps::empty(),
        }
    }

    #[cfg(test)]
    pub fn set_running(&mut self, running: RunningApps) {
        self.running = running;
    }

    #[cfg(test)]
    pub fn set_app_data_dir(&mut self, dir: PathBuf) {
        self.app_data_dir = Some(dir);
    }

    /// Pretends `$HOME` lives on a different device. Mounting a second volume
    /// inside a unit test is not worth the flakiness; what can actually regress
    /// is the comparison being dropped, and this exercises that branch.
    #[cfg(test)]
    pub fn set_home_dev(&mut self, dev: u64) {
        self.home_dev = dev;
    }
}

fn depth(p: &Path) -> usize {
    p.components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count()
}

/// Rules that need no filesystem access: R16, R1, R2.
fn check_shape(candidate: &Path) -> Result<(), Rejection> {
    if candidate.as_os_str().as_bytes().contains(&0) {
        return Err(Rejection::new(candidate, RuleId::NulByte));
    }
    if !candidate.is_absolute() {
        return Err(Rejection::new(candidate, RuleId::NotAbsolute));
    }
    // Only RootDir and literal names are acceptable. This rejects `..` and `.`,
    // and it must run before any prefix comparison: `Caches/../Documents` is a
    // prefix match for `Caches` while pointing somewhere else entirely.
    if !candidate
        .components()
        .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
    {
        return Err(Rejection::new(candidate, RuleId::NonNormalComponent));
    }
    Ok(())
}

/// The only constructor for [`SafeTarget`].
pub fn vet(candidate: &Path, mode: DeleteMode, ctx: &GuardCtx) -> Result<SafeTarget, Rejection> {
    check_shape(candidate)?;

    // R7. `depth >= 3` puts the floor at a direct child of a standard home
    // directory; the ancestor test covers non-standard homes, where a shallow
    // path could still contain `$HOME`.
    if depth(candidate) < 3 || ctx.home.starts_with(candidate) {
        return Err(Rejection::new(candidate, RuleId::TooShallow));
    }

    // R8 and R9 run before R6 so that a path which is both out of scope *and*
    // protected reports the protection. "This is your Documents folder" is a more
    // useful thing to surface than "not in scope".
    if let Some(hit) = deny::containing_entry(candidate, &ctx.home) {
        return Err(Rejection::with(candidate, RuleId::Protected, hit.display()));
    }
    // R9 — the rule that stops `~/Library` from taking Keychains with it.
    if let Some(hit) = deny::descendant_entry(candidate, &ctx.home) {
        return Err(Rejection::with(
            candidate,
            RuleId::WouldTakeProtected,
            hit.display(),
        ));
    }
    if let Some(id) = deny::protected_bundle(candidate, &ctx.home) {
        return Err(Rejection::with(candidate, RuleId::SystemBundle, id));
    }

    // R6.
    match roots::classify(candidate, &ctx.home) {
        Containment::Below | Containment::IsWholeRoot => {}
        Containment::IsChildrenOnlyRoot => {
            return Err(Rejection::new(candidate, RuleId::RootNotDeletable));
        }
        Containment::Outside => return Err(Rejection::new(candidate, RuleId::OutsideRoots)),
    }

    // R11. Deleting our own store mid-run would drop the ledger we rely on to
    // report what happened.
    if let Some(own) = &ctx.app_data_dir {
        if candidate.starts_with(own) || own.starts_with(candidate) {
            return Err(Rejection::new(candidate, RuleId::OwnAppData));
        }
    }

    // R3.
    let md = std::fs::symlink_metadata(candidate)
        .map_err(|_| Rejection::new(candidate, RuleId::Missing))?;
    let ft = md.file_type();

    // R4. A symlink is refused rather than followed: deleting the link is
    // pointless and following it escapes every check above.
    if ft.is_symlink() {
        return Err(Rejection::new(candidate, RuleId::Symlink));
    }
    // R14.
    if !(ft.is_dir() || ft.is_file()) || ft.is_socket() || ft.is_fifo() {
        return Err(Rejection::new(candidate, RuleId::NotFileOrDir));
    }
    // R12. Catches external disks, network shares, Time Machine, and cloud
    // providers that mount as their own volume.
    if md.dev() != ctx.home_dev {
        return Err(Rejection::new(candidate, RuleId::OtherVolume));
    }
    // R5. The catch-all for symlinks anywhere in the *parent* chain: if any
    // component was a link, the resolved path differs from the one we vetted.
    let resolved = candidate
        .canonicalize()
        .map_err(|_| Rejection::new(candidate, RuleId::Missing))?;
    if resolved != candidate {
        return Err(Rejection::with(
            candidate,
            RuleId::PathAliased,
            resolved.display(),
        ));
    }

    // R10.
    if let Some(id) = deny::bundle_id_component(candidate, &ctx.home) {
        if ctx.running.owns_bundle(&id) {
            return Err(Rejection::with(candidate, RuleId::AppRunning, id));
        }
    }
    if ft.is_dir() {
        if let Some(exe) = ctx.running.exe_inside(candidate) {
            return Err(Rejection::with(
                candidate,
                RuleId::AppRunning,
                exe.display(),
            ));
        }
    }

    // R15. The last line of defence: permanent deletion is confined to a
    // compile-time table, so even a fully compromised candidate set can only
    // permanently destroy things listed in `catalog::quick`.
    if mode == DeleteMode::Permanent && !quick::covers(candidate, &ctx.home) {
        return Err(Rejection::new(candidate, RuleId::PermanentNotAllowed));
    }

    Ok(SafeTarget {
        path: candidate.to_path_buf(),
        mode,
        is_dir: ft.is_dir(),
    })
}

/// Vets a whole batch, additionally enforcing R13.
///
/// Exact duplicates collapse. An ancestor/descendant pair rejects *both* sides:
/// the overlap means the caller's picture of the filesystem is wrong, and
/// guessing which one it meant is how a partial delete becomes a surprise.
pub fn vet_all(
    candidates: &[PathBuf],
    mode: DeleteMode,
    ctx: &GuardCtx,
) -> (Vec<SafeTarget>, Vec<Rejection>) {
    let mut seen = HashSet::new();
    let unique: Vec<&PathBuf> = candidates
        .iter()
        .filter(|p| seen.insert(p.as_path()))
        .collect();

    let overlapping: HashSet<&Path> = unique
        .iter()
        .enumerate()
        .flat_map(|(i, a)| {
            unique[i + 1..].iter().filter_map(move |b| {
                (a.starts_with(b) || b.starts_with(a)).then_some((a.as_path(), b.as_path()))
            })
        })
        .flat_map(|(a, b)| [a, b])
        .collect();

    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for path in unique {
        if overlapping.contains(path.as_path()) {
            rejected.push(Rejection::new(path, RuleId::Overlapping));
            continue;
        }
        match vet(path, mode, ctx) {
            Ok(target) => accepted.push(target),
            Err(r) => rejected.push(r),
        }
    }
    (accepted, rejected)
}
