//! Rebindable keyboard shortcuts for the app's `gio` actions.
//!
//! [`ACTIONS`] declares every bindable action and its default accelerator;
//! [`Keymap`] loads per-user overrides from `<config dir>/rhymr/keymap.txt`
//! (`key.<action>=<accel>` lines) and applies them to the running
//! `Application`. The settings dialog's Keymap page edits the same file.

use gtk::prelude::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// The platform's primary modifier, as an accelerator token. Stored accels
/// use the literal `<Primary>` placeholder; [`expand`] swaps it for this
/// before the string reaches GTK.
#[cfg(target_os = "macos")]
pub const PRIMARY_MOD: &str = "<Meta>";
#[cfg(not(target_os = "macos"))]
pub const PRIMARY_MOD: &str = "<Control>";

/// The matching `ModifierType` bit for [`PRIMARY_MOD`].
#[cfg(target_os = "macos")]
pub const PRIMARY_MASK: gtk::gdk::ModifierType = gtk::gdk::ModifierType::META_MASK;
#[cfg(not(target_os = "macos"))]
pub const PRIMARY_MASK: gtk::gdk::ModifierType = gtk::gdk::ModifierType::CONTROL_MASK;

/// Stored value meaning "this action has no shortcut" (distinct from
/// "use the default", which is the absence of an override).
pub const UNBOUND: &str = "none";

/// One keyboard-bindable action.
pub struct ActionSpec {
    /// `gio` action name, without the `app.` prefix.
    pub action: &'static str,
    /// Human label for the Keymap page.
    pub label: &'static str,
    /// Sub-group heading on the Keymap page.
    pub group: &'static str,
    /// Default accelerator in GTK form, with `<Primary>` for the platform
    /// mod key (⌘ on macOS, Ctrl elsewhere).
    pub default_accel: &'static str,
}

/// Every rebindable action, in Keymap-page order. Mirrors the accelerators
/// `crate::app::menu` used to hard-code.
pub const ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        action: "new",
        label: "New File",
        group: "File",
        default_accel: "<Primary>n",
    },
    ActionSpec {
        action: "new-folder",
        label: "New Folder",
        group: "File",
        default_accel: "<Primary><Shift>n",
    },
    ActionSpec {
        action: "open",
        label: "Open Project\u{2026}",
        group: "File",
        default_accel: "<Primary>o",
    },
    ActionSpec {
        action: "save",
        label: "Save",
        group: "File",
        default_accel: "<Primary><Alt>s",
    },
    ActionSpec {
        action: "save-as",
        label: "Save As\u{2026}",
        group: "File",
        default_accel: "<Primary><Shift>s",
    },
    ActionSpec {
        action: "save-all",
        label: "Save All",
        group: "File",
        default_accel: "<Primary>s",
    },
    ActionSpec {
        action: "reload-all",
        label: "Reload All from Disk",
        group: "File",
        default_accel: "<Primary><Alt>y",
    },
    ActionSpec {
        action: "close-tab",
        label: "Close Tab",
        group: "File",
        default_accel: "<Primary>w",
    },
    ActionSpec {
        action: "preferences",
        label: "Preferences\u{2026}",
        group: "File",
        default_accel: "<Primary>comma",
    },
    ActionSpec {
        action: "git-commit",
        label: "Commit\u{2026}",
        group: "Version Control",
        default_accel: "<Primary>k",
    },
    ActionSpec {
        action: "git-push",
        label: "Push\u{2026}",
        group: "Version Control",
        default_accel: "<Primary><Shift>k",
    },
    ActionSpec {
        action: "git-pull",
        label: "Update Project",
        group: "Version Control",
        default_accel: "<Primary>t",
    },
    ActionSpec {
        action: "git-log",
        label: "Git Log",
        group: "Version Control",
        default_accel: "<Alt>9",
    },
];

/// Swap the `<Primary>` placeholder for the platform mod key.
pub fn expand(accel: &str) -> String {
    accel.replace("<Primary>", PRIMARY_MOD)
}

/// The default accelerator for `action` (stored form), if it's a known action.
pub fn default_accel(action: &str) -> Option<&'static str> {
    ACTIONS
        .iter()
        .find(|a| a.action == action)
        .map(|a| a.default_accel)
}

/// A `<Primary>n` style accel rendered for display: ⌘⇧K on macOS, Ctrl+Shift+K
/// elsewhere. An empty accel renders as an en dash.
pub fn pretty(accel: &str) -> String {
    if accel.is_empty() {
        return "\u{2013}".to_string();
    }
    if accel == UNBOUND {
        return "Unbound".to_string();
    }
    #[cfg(target_os = "macos")]
    let (primary, shift, alt, join) = ("\u{2318}", "\u{21e7}", "\u{2325}", "");
    #[cfg(not(target_os = "macos"))]
    let (primary, shift, alt, join) = ("Ctrl", "Shift", "Alt", "+");

    let mut out = String::new();
    let mut rest = accel;
    for (token, label) in [
        ("<Primary>", primary),
        ("<Control>", "Ctrl"),
        ("<Meta>", "\u{2318}"),
        ("<Shift>", shift),
        ("<Alt>", alt),
    ] {
        if let Some(stripped) = rest.strip_prefix(token) {
            if !out.is_empty() {
                out.push_str(join);
            }
            out.push_str(label);
            rest = stripped;
        }
    }
    if !out.is_empty() && !rest.is_empty() {
        out.push_str(join);
    }
    out.push_str(&pretty_key(rest));
    out
}

fn pretty_key(key: &str) -> String {
    match key {
        "comma" => ",".to_string(),
        "period" => ".".to_string(),
        "slash" => "/".to_string(),
        "" => String::new(),
        k if k.chars().count() == 1 => k.to_uppercase(),
        k => k.to_string(),
    }
}

fn keymap_file() -> Option<PathBuf> {
    let mut dir = dirs::config_dir()?;
    dir.push("rhymr");
    fs::create_dir_all(&dir).ok()?;
    dir.push("keymap.txt");
    Some(dir)
}

/// Per-user accelerator overrides. An action with no entry uses its
/// [`ActionSpec::default_accel`].
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Keymap {
    /// `action -> accel` (stored form, may contain `<Primary>`).
    overrides: BTreeMap<String, String>,
}

impl Keymap {
    /// Read `keymap.txt`, keeping only lines for known actions.
    pub fn load() -> Self {
        let mut map = Keymap::default();
        let Some(path) = keymap_file() else {
            return map;
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            return map;
        };
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if let Some(action) = key.strip_prefix("key.")
                && default_accel(action).is_some()
            {
                map.overrides.insert(action.to_string(), value.to_string());
            }
        }
        map
    }

    /// Write every overridden binding back. A write failure is logged, not
    /// propagated.
    pub fn save(&self) {
        let Some(path) = keymap_file() else {
            return;
        };
        let mut out = String::new();
        for (action, accel) in &self.overrides {
            out.push_str("key.");
            out.push_str(action);
            out.push('=');
            out.push_str(accel);
            out.push('\n');
        }
        if let Err(err) = fs::write(&path, out) {
            log::warn!("could not save keymap to {}: {err}", path.display());
        }
    }

    /// The effective stored accel for `action` (override, else default).
    pub fn accel(&self, action: &str) -> String {
        self.overrides
            .get(action)
            .cloned()
            .or_else(|| default_accel(action).map(str::to_string))
            .unwrap_or_default()
    }

    /// Set (`Some`) or clear (`None`) an override. Clearing, or setting the
    /// default value, removes the entry.
    pub fn set(&mut self, action: &str, accel: Option<&str>) {
        match accel {
            Some(a) if Some(a) != default_accel(action) && !a.is_empty() => {
                self.overrides.insert(action.to_string(), a.to_string());
            }
            _ => {
                self.overrides.remove(action);
            }
        }
    }

    /// The action currently bound to stored accel `accel`, if any (skipping
    /// `except`). Used for conflict detection on the Keymap page.
    pub fn action_for_accel(&self, accel: &str, except: &str) -> Option<&'static str> {
        ACTIONS
            .iter()
            .find(|a| a.action != except && self.accel(a.action) == accel)
            .map(|a| a.action)
    }

    /// Apply every action's effective accel to `app`.
    pub fn apply(&self, app: &impl IsA<gtk::Application>) {
        for spec in ACTIONS {
            let name = format!("app.{}", spec.action);
            let stored = self.accel(spec.action);
            if stored == UNBOUND || stored.is_empty() {
                app.set_accels_for_action(&name, &[]);
            } else {
                app.set_accels_for_action(&name, &[expand(&stored).as_str()]);
            }
        }
    }
}

/// True for a key that's only ever a modifier — a chord capture ignores
/// these and waits for the real key.
pub fn is_modifier(key: gtk::gdk::Key) -> bool {
    use gtk::gdk::Key;
    matches!(
        key,
        Key::Shift_L
            | Key::Shift_R
            | Key::Control_L
            | Key::Control_R
            | Key::Alt_L
            | Key::Alt_R
            | Key::Meta_L
            | Key::Meta_R
            | Key::Super_L
            | Key::Super_R
            | Key::Hyper_L
            | Key::Hyper_R
            | Key::Caps_Lock
    )
}

/// Turn a captured keypress into a stored accel string (`<Primary><Shift>k`).
/// `None` for a bare key with no usable modifier, or an unnameable key.
pub fn accel_from(key: gtk::gdk::Key, state: gtk::gdk::ModifierType) -> Option<String> {
    use gtk::gdk::ModifierType;
    let name = key.name()?;
    let name = name.as_str();
    if name.is_empty() {
        return None;
    }

    let primary = state.contains(PRIMARY_MASK);
    let ctrl = state.contains(ModifierType::CONTROL_MASK);
    let meta = state.contains(ModifierType::META_MASK);
    let alt = state.contains(ModifierType::ALT_MASK);
    let shift = state.contains(ModifierType::SHIFT_MASK);
    if !(primary || ctrl || meta || alt) {
        return None;
    }

    let mut out = String::new();
    if primary {
        out.push_str("<Primary>");
    } else {
        if ctrl {
            out.push_str("<Control>");
        }
        if meta {
            out.push_str("<Meta>");
        }
    }
    if shift {
        out.push_str("<Shift>");
    }
    if alt {
        out.push_str("<Alt>");
    }
    out.push_str(name);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_stores_only_real_overrides() {
        let mut km = Keymap::default();
        km.set("save", Some("<Primary><Alt>s")); // == default -> not stored
        assert_eq!(km, Keymap::default());

        km.set("save", Some("<Primary>e"));
        assert_eq!(km.accel("save"), "<Primary>e");

        km.set("save", None); // clear -> back to default
        assert_eq!(km.accel("save"), "<Primary><Alt>s");

        km.set("save", Some(UNBOUND));
        assert_eq!(km.accel("save"), UNBOUND);
    }

    #[test]
    fn conflict_lookup_skips_the_action_itself() {
        let km = Keymap::default();
        // git-push default is <Primary><Shift>k
        assert_eq!(
            km.action_for_accel("<Primary><Shift>k", "new"),
            Some("git-push")
        );
        assert_eq!(km.action_for_accel("<Primary><Shift>k", "git-push"), None);
    }

    #[test]
    fn pretty_is_readable() {
        assert_eq!(pretty(""), "\u{2013}");
        assert_eq!(pretty(UNBOUND), "Unbound");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(pretty("<Primary><Shift>k"), "Ctrl+Shift+K");
        #[cfg(target_os = "macos")]
        assert_eq!(pretty("<Primary>comma"), "\u{2318},");
    }
}
