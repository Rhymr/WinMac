//! A `ToolWindow` descriptor — the registry entry the [`crate::app::dock`]
//! `DockArea` shows, hides, sizes and (later) moves. Each tool window is a
//! stable id, a title/icon for its stripe button and header, a default dock
//! edge and size, and the already-built content widget (the panels are
//! constructed once in `layout.rs`).

/// Which edge of the dock a tool window lives on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Anchor {
    Left,
    Right,
    Bottom,
}

impl Anchor {
    /// Stable string form for the persisted layout file.
    pub fn as_str(self) -> &'static str {
        match self {
            Anchor::Left => "left",
            Anchor::Right => "right",
            Anchor::Bottom => "bottom",
        }
    }

    /// Parse [`Anchor::as_str`]; unknown values fall back to `default`.
    pub fn parse(s: &str, default: Anchor) -> Anchor {
        match s {
            "left" => Anchor::Left,
            "right" => Anchor::Right,
            "bottom" => Anchor::Bottom,
            _ => default,
        }
    }
}

/// One registered tool window.
pub struct ToolWindow {
    /// Stable id — used as the persisted-layout key and the toggle action.
    pub id: &'static str,
    /// Shown on the stripe button and the tool-window header.
    pub title: &'static str,
    /// Bundled icon stem (see [`crate::app::icons`]).
    pub icon: &'static str,
    /// The edge this window docks to by default.
    pub default_anchor: Anchor,
    /// Default extent in px (width for Left/Right, height for Bottom).
    pub default_size: i32,
    /// Whether the window is open on a first run / after "Restore Default
    /// Layout" (JetBrains opens Project by default, nothing else).
    pub default_open: bool,
    /// The panel body. Must be parent-agnostic so the dock can reparent it.
    pub content: gtk::Widget,
}
