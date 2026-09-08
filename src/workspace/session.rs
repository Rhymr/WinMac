//! Per-workspace UI state remembered between launches — which folders are
//! folded, how wide the left panel is, whether the Rhyme Search panel is
//! open and how tall. Stored as one JSON map in the user config dir
//! (`config::session_file`), keyed by workspace root path, so any project
//! reopens looking the way it was left.
//!
//! This is *view* state, deliberately separate from [`Settings`]
//! (behavioural preferences) — losing it is cosmetic, never breaking.
//!
//! [`Settings`]: crate::setting::Settings

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Remembered view state for one workspace.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct WorkspaceSession {
    /// Folded project-tree directories, as paths relative to the workspace
    /// root (so the record survives the project moving on disk).
    #[serde(default)]
    pub collapsed_dirs: Vec<String>,
    /// Folded rows in the read-only source panel — the panel's own opaque
    /// keys (`\x01<source-id>` for a section, `<source-id>\x1f<folder>` for
    /// a folder). `None` means the user has never touched the panel, so it
    /// gets its default (every source section folded); `Some` is the exact
    /// folded set, authoritative even when empty.
    #[serde(default)]
    pub collapsed_sources: Option<Vec<String>>,
    /// Left panel (project + source trees) width in px.
    #[serde(default)]
    pub left_panel_width: Option<i32>,
    /// Rhyme Search panel height in px, when it was last open.
    #[serde(default)]
    pub rhyme_panel_height: Option<i32>,
    /// Whether the Rhyme Search panel was open.
    #[serde(default)]
    pub rhyme_panel_visible: Option<bool>,
}

type SessionMap = HashMap<String, WorkspaceSession>;

fn read_all() -> SessionMap {
    let Some(path) = crate::config::session_file() else {
        return SessionMap::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return SessionMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_all(map: &SessionMap) {
    let Some(path) = crate::config::session_file() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(map) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("session: could not write {path:?}: {e}");
            }
        }
        Err(e) => log::warn!("session: could not serialise state: {e}"),
    }
}

fn key(root: &Path) -> String {
    root.to_string_lossy().into_owned()
}

/// The remembered state for `root` — a default (nothing folded, no sizes)
/// when this workspace has never been saved.
pub fn load(root: &Path) -> WorkspaceSession {
    read_all().remove(&key(root)).unwrap_or_default()
}

/// Merge `f`'s changes into `root`'s remembered state and persist. Reads
/// the file fresh each call so concurrent windows don't clobber each other
/// wholesale (last writer per field still wins, which is fine for view
/// state).
pub fn update(root: &Path, f: impl FnOnce(&mut WorkspaceSession)) {
    let mut map = read_all();
    let entry = map.entry(key(root)).or_default();
    let before = entry.clone();
    f(entry);
    if *entry != before {
        write_all(&map);
    }
}
