//! Persistence for the tool-window layout — a small JSON file in the user
//! config dir (`config::dock_layout_file`), separate from the per-workspace
//! `session.json` because the dock layout is global. Losing it is purely
//! cosmetic: every field falls back to the `ToolWindow`'s own defaults.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Remembered state for one tool window.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct WindowState {
    /// Dock edge (`"left"` / `"right"` / `"bottom"`).
    pub anchor: String,
    /// Extent in px (width for Left/Right, height for Bottom).
    pub size: i32,
    /// Whether the window was open.
    pub open: bool,
}

/// The whole layout: one entry per tool-window id.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Eq, Debug)]
pub struct DockLayout {
    #[serde(default)]
    pub windows: BTreeMap<String, WindowState>,
}

impl DockLayout {
    /// Load the saved layout, or an empty one when there's no file yet / it
    /// can't be read or parsed.
    pub fn load() -> Self {
        let Some(path) = crate::config::dock_layout_file() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Write the layout, creating the config dir if needed. Failures are
    /// logged, not surfaced — this is cosmetic state.
    pub fn save(&self) {
        let Some(path) = crate::config::dock_layout_file() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    log::warn!("dock layout: could not write {path:?}: {e}");
                }
            }
            Err(e) => log::warn!("dock layout: could not serialise: {e}"),
        }
    }
}
