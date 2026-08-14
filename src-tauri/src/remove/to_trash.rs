//! Moving a vetted target to `~/.Trash`.

use trash::macos::{DeleteMethod, TrashContextExtMacos};
use trash::TrashContext;

use crate::safety::guard::{DeleteMode, SafeTarget};

/// The `trash` crate defaults to [`DeleteMethod::Finder`], which drives `osascript`.
/// That needs Apple Events permission, and an unsigned app loses the grant on every
/// rebuild — so it would break constantly during development and silently depend on a
/// TCC prompt in release. `NsFileManager` needs no extra permission, and Finder's "Put
/// Back" still works with it (verified on macOS 26.4, contrary to the crate's docs).
pub fn context() -> TrashContext {
    let mut ctx = TrashContext::default();
    ctx.set_delete_method(DeleteMethod::NsFileManager);
    ctx
}

pub fn send(ctx: &TrashContext, target: &SafeTarget) -> Result<(), trash::Error> {
    debug_assert!(
        matches!(target.mode(), DeleteMode::Trash),
        "to_trash received a target vetted for permanent deletion"
    );
    ctx.delete(target.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default is `Finder`, so this asserts against a regression that would look
    /// harmless in a diff and only fail on a machine without Apple Events permission.
    #[test]
    fn the_context_uses_nsfilemanager_not_finder() {
        // `DeleteMethod` does not implement `PartialEq`.
        assert!(matches!(
            context().delete_method(),
            DeleteMethod::NsFileManager
        ));
    }
}
