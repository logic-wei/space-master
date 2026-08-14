//! Snapshot of what is currently running, so Guard can refuse to delete data
//! belonging to a live process.
//!
//! Deleting a running app's container does not usually crash it — it silently
//! loses state and rewrites a fresh container on quit, which looks exactly like
//! data corruption to the user.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::fsutil::bundle::Identity;

#[derive(Debug, Default)]
pub struct RunningApps {
    bundle_ids: HashSet<String>,
    exe_paths: Vec<PathBuf>,
}

impl RunningApps {
    /// Empty snapshot. Used by tests, and as the fallback when process
    /// enumeration is unavailable — R10 then contributes nothing, and the
    /// remaining rules still apply.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn detect() -> Self {
        use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};

        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_processes(ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet)),
        );

        let mut bundle_ids = HashSet::new();
        let mut exe_paths = Vec::new();
        // Many processes share one bundle (helpers, XPC services), so resolving
        // each .app once keeps this to a few dozen plist reads.
        let mut resolved: HashMap<PathBuf, Option<String>> = HashMap::new();

        for proc in sys.processes().values() {
            let Some(exe) = proc.exe() else { continue };
            exe_paths.push(exe.to_path_buf());

            let Some(bundle) = enclosing_app_bundle(exe) else {
                continue;
            };
            let id = resolved
                .entry(bundle.clone())
                .or_insert_with(|| read_bundle_id(&bundle));
            if let Some(id) = id {
                bundle_ids.insert(id.clone());
            }
        }

        Self {
            bundle_ids,
            exe_paths,
        }
    }

    #[cfg(test)]
    pub fn with_bundle_ids<I: IntoIterator<Item = String>>(ids: I) -> Self {
        Self {
            bundle_ids: ids.into_iter().collect(),
            exe_paths: Vec::new(),
        }
    }

    pub fn owns_bundle(&self, id: &str) -> bool {
        self.bundle_ids.contains(id)
    }

    /// Every bundle a live process belongs to. Orphan detection folds these into its
    /// installed set: software that is running is installed, whether or not its `.app`
    /// turned up anywhere we looked.
    pub fn bundle_ids(&self) -> &HashSet<String> {
        &self.bundle_ids
    }

    /// The executable of a live process located inside `dir`, if any. Catches
    /// the case where an app runs from a directory we were about to remove.
    pub fn exe_inside(&self, dir: &Path) -> Option<&Path> {
        self.exe_paths
            .iter()
            .find(|p| p.starts_with(dir))
            .map(PathBuf::as_path)
    }
}

/// The outermost `.app` directory containing `exe`. Outermost rather than
/// nearest, because a helper at `Foo.app/Contents/.../Helper.app/...` belongs to
/// `Foo.app` for our purposes.
fn enclosing_app_bundle(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .filter(|a| a.extension().is_some_and(|e| e == "app"))
        .last()
        .map(Path::to_path_buf)
}

fn read_bundle_id(bundle: &Path) -> Option<String> {
    match crate::fsutil::bundle::identify(bundle) {
        Identity::Named(id) => Some(id),
        // A bundle we cannot name contributes nothing to R10 and is not worth
        // reporting: the rule can only ever refuse more than it must.
        Identity::NotABundle | Identity::Unnamed => None,
    }
}
