//! Central home for paths and intervals that would otherwise be scattered
//! as magic literals across the codebase.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Per-workspace config/cache directory (`<workspace>/.rhymr/`), created
/// lazily by callers that write into it.
pub fn workspace_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".rhymr")
}

/// JSON snapshot of the user's Apple Notes, kept in the workspace's
/// `.rhymr/` so the "Apple Notes" tree shows something instantly on launch
/// while a fresh read runs in the background. `None` when there's no
/// workspace loaded yet.
pub fn apple_notes_cache(workspace_root: Option<&Path>) -> Option<PathBuf> {
    Some(workspace_dir(workspace_root?).join("apple-notes.json"))
}

/// How often the "Apple Notes" tree re-reads from the Notes app.
pub const APPLE_NOTES_REFRESH: Duration = Duration::from_secs(300);
