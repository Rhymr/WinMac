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

/// The on-disk `assets/` tree the running app still reads at startup — the
/// SCSS sources compiled by [`crate::css`], and the GtkSourceView colour
/// schemes under `styles/`. Resolution order:
///
/// 1. `RHYMR_ASSETS_DIR`, if set — an explicit override for unusual layouts.
/// 2. Inside a macOS `.app` bundle: `Contents/Resources/assets`, next to the
///    executable (`scripts/bundle-mac.sh` copies the tree there). This is
///    what makes a Finder/Dock launch work — its working directory is `/`,
///    so a bare `assets/…` path would miss.
/// 3. `<current dir>/assets` — the dev layout, run from the repo root
///    (see CLAUDE.md § Build / run / check).
pub fn assets_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("RHYMR_ASSETS_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(bundled) = bundled_assets_dir() {
        return bundled;
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("assets")
}

/// `Contents/Resources/assets` when the executable sits at
/// `…/<name>.app/Contents/MacOS/<bin>`, and that directory exists.
#[cfg(target_os = "macos")]
fn bundled_assets_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let macos_dir = exe.parent()?;
    if macos_dir.file_name()? != "MacOS" {
        return None;
    }
    let candidate = macos_dir.parent()?.join("Resources").join("assets");
    candidate.is_dir().then_some(candidate)
}

/// No `.app` bundle layout off macOS — fall through to the dev path.
#[cfg(not(target_os = "macos"))]
fn bundled_assets_dir() -> Option<PathBuf> {
    None
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

/// How often the "Apple Notes" tree re-reads from the Notes app
/// (Settings → Tools → Network & Sources; default 300s). Read once when
/// the source panel arms its refresh timer.
pub fn apple_notes_refresh() -> Duration {
    Duration::from_secs(u64::from(
        crate::setting::Settings::load().apple_notes_refresh_secs,
    ))
}

/// Cap on a single Rhyme Search lookup (the Datamuse round-trips); past
/// this the lookup is abandoned and the panel shows an error state
/// (Settings → Tools → Network & Sources; default 8s). Read per lookup.
pub fn rhyme_lookup_timeout() -> Duration {
    Duration::from_secs(u64::from(
        crate::setting::Settings::load().rhyme_lookup_timeout_secs,
    ))
}
