//! Central home for paths and intervals that would otherwise be scattered
//! as magic literals across the codebase.

use crate::setting::NotesCacheScope;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `<config dir>/rhymr/` — the same directory `setting::settings_file`
/// uses. `None` when the OS reports no config dir.
fn user_config_dir() -> Option<PathBuf> {
    let mut dir = dirs::config_dir()?;
    dir.push("rhymr");
    Some(dir)
}

/// Per-workspace config/cache directory (`<workspace>/.rhymr/`), created
/// lazily by callers that write into it.
pub fn workspace_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".rhymr")
}

/// Where the Apple Notes JSON snapshot lives, per the user's
/// [`NotesCacheScope`] setting:
/// - `Workspace` → `<workspace>/.rhymr/apple-notes.json` (per project)
/// - `User` → `<config dir>/rhymr/apple-notes.json` (shared)
///
/// `None` for `Workspace` when no workspace is loaded, or when no config
/// dir is available.
pub fn apple_notes_cache(workspace_root: Option<&Path>, scope: NotesCacheScope) -> Option<PathBuf> {
    match scope {
        NotesCacheScope::User => Some(user_config_dir()?.join("apple-notes.json")),
        NotesCacheScope::Workspace => Some(workspace_dir(workspace_root?).join("apple-notes.json")),
    }
}

/// `<config dir>/rhymr/session.json` — per-workspace UI state (folded
/// folders, panel sizes) remembered across launches. `None` when the OS
/// reports no config dir.
pub fn session_file() -> Option<PathBuf> {
    Some(user_config_dir()?.join("session.json"))
}

/// How often the "Apple Notes" tree re-reads from the Notes app.
pub const APPLE_NOTES_REFRESH: Duration = Duration::from_secs(300);

/// Cap on a single Rhyme Search lookup (the Datamuse round-trips). Past
/// this the lookup is abandoned and the panel shows an error state.
pub const RHYME_LOOKUP_TIMEOUT: Duration = Duration::from_secs(8);
