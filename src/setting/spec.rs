//! The settings registry: every user-configurable setting declared once,
//! in `crate::setting`'s `settings! { … }` block, as a [`SettingSpec`].
//!
//! `load` / `save` (the flat `key=value` file) and — from the next commit —
//! the settings dialog both derive from [`crate::setting::SPECS`], so a new
//! setting is one macro entry, not five hand-kept copies.

use super::Settings;

/// A serialised scalar, carried between the flat file, the spec `get` / `set`
/// thunks, and (later) the dialog widgets. Owns no borrow of [`Settings`].
#[derive(Clone, PartialEq, Debug)]
pub enum SettingValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl SettingValue {
    /// Render for one `key=value` line. Text loses CR/LF so a pasted value
    /// can't split the line.
    pub fn render(&self) -> String {
        match self {
            SettingValue::Bool(b) => b.to_string(),
            SettingValue::Int(n) => n.to_string(),
            // `{}` round-trips and drops trailing zeros ("3.25", "0", "1.5").
            SettingValue::Float(f) => format!("{f}"),
            SettingValue::Text(s) => s.replace(['\n', '\r'], ""),
        }
    }
}

/// A setting's value type plus its validation domain. The dialog picks a
/// widget from this; `load` clamps/validates raw file text against it.
#[derive(Clone, Copy)]
pub enum SettingKind {
    Bool,
    Int {
        min: i64,
        max: i64,
        step: i64,
    },
    /// A fractional number. `step` also fixes the spin button's shown
    /// decimal places.
    Float {
        min: f64,
        max: f64,
        step: f64,
    },
    /// A closed set: `values` are the on-disk tokens, `labels` the UI
    /// strings — index-aligned, equal length.
    Enum {
        values: &'static [&'static str],
        labels: &'static [&'static str],
    },
    /// Free text.
    Text,
    /// A `FontDialogButton` bound to a family field plus a paired size
    /// field.
    Font,
    /// The size half of a [`SettingKind::Font`] pair — the dialog's page
    /// builder skips it (the font widget owns it); `load` / `save` treat it
    /// as a clamped int.
    FontSize {
        min: i64,
        max: i64,
    },
}

impl SettingKind {
    /// Parse one raw file value, validating and clamping to the domain.
    /// `None` = unusable (unparseable, or an unknown enum token) — the
    /// caller keeps the current field value, matching the old lenient
    /// `load`.
    pub fn parse_clamped(&self, raw: &str) -> Option<SettingValue> {
        match *self {
            // The old rule: anything but exactly "true" is false.
            SettingKind::Bool => Some(SettingValue::Bool(raw == "true")),
            SettingKind::Int { min, max, .. } | SettingKind::FontSize { min, max } => raw
                .parse::<i64>()
                .ok()
                .map(|n| SettingValue::Int(n.clamp(min, max))),
            SettingKind::Float { min, max, .. } => raw
                .parse::<f64>()
                .ok()
                .filter(|f| f.is_finite())
                .map(|f| SettingValue::Float(f.clamp(min, max))),
            SettingKind::Enum { values, .. } => values
                .contains(&raw)
                .then(|| SettingValue::Text(raw.to_string())),
            SettingKind::Text | SettingKind::Font => {
                (!raw.is_empty()).then(|| SettingValue::Text(raw.to_string()))
            }
        }
    }
}

/// Whether a change reaches open tabs / on-screen chrome without a
/// relaunch. Read by the dialog to decide whether to show a
/// "(restart required)" note and which live-apply path to run.
#[derive(Clone, Copy, PartialEq)]
pub enum LiveApply {
    /// Pushed to open editors by `TextEditor::apply_settings`.
    Editor,
    /// Applied by re-rendering the stylesheet (`css::reload`).
    Css,
    EditorAndCss,
    /// A dedicated hook (icon variant swap, tree/chrome rebuild).
    Other,
    /// Read once at subsystem construction; the dialog shows
    /// "(restart required)".
    Restart,
}

/// One declared setting: everything the file format, the dirty-check, and
/// the dialog need, in one place.
pub struct SettingSpec {
    pub key: &'static str,
    pub kind: SettingKind,
    pub category: CategoryId,
    /// Sub-header within the page; `""` for the first, unlabelled group.
    pub group: &'static str,
    pub label: &'static str,
    /// Shown under the control; `""` for none.
    pub description: &'static str,
    pub live: LiveApply,
    /// Read the value out of a [`Settings`].
    pub get: fn(&Settings) -> SettingValue,
    /// Write an already-clamped value back. A wrong [`SettingValue`] variant
    /// is a silent no-op.
    pub set: fn(&mut Settings, SettingValue),
}

/// A node in the settings dialog's left-hand category tree.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CategoryId {
    Appearance,
    AppearanceWindow,
    ColorScheme,
    EditorGeneral,
    EditorRhyme,
    EditorCompletion,
    EditorFileTree,
    Keymap,
    VersionControlGit,
    ToolsNetwork,
    Advanced,
}

/// One category-tree entry: its id, its parent (`None` = top level), its
/// sidebar/breadcrumb label, and the group name shown before the label in
/// the content header when it has no parent node of its own (JetBrains
/// "Version Control › Git").
pub struct CategoryNode {
    pub id: CategoryId,
    pub parent: Option<CategoryId>,
    pub label: &'static str,
    pub breadcrumb_parent: &'static str,
}

/// Depth-first; the dialog renders it verbatim.
pub const CATEGORY_TREE: &[CategoryNode] = &[
    CategoryNode {
        id: CategoryId::Appearance,
        parent: None,
        label: "Appearance",
        breadcrumb_parent: "Appearance & Behavior",
    },
    CategoryNode {
        id: CategoryId::AppearanceWindow,
        parent: Some(CategoryId::Appearance),
        label: "Window & Startup",
        breadcrumb_parent: "",
    },
    CategoryNode {
        id: CategoryId::ColorScheme,
        parent: Some(CategoryId::Appearance),
        label: "Color Scheme",
        breadcrumb_parent: "",
    },
    CategoryNode {
        id: CategoryId::EditorGeneral,
        parent: None,
        label: "Editor",
        breadcrumb_parent: "",
    },
    CategoryNode {
        id: CategoryId::EditorRhyme,
        parent: Some(CategoryId::EditorGeneral),
        label: "Rhyme Highlighting",
        // "Editor" already comes from the parent chain.
        breadcrumb_parent: "",
    },
    CategoryNode {
        id: CategoryId::EditorCompletion,
        parent: Some(CategoryId::EditorGeneral),
        label: "Completions",
        breadcrumb_parent: "",
    },
    CategoryNode {
        id: CategoryId::EditorFileTree,
        parent: Some(CategoryId::EditorGeneral),
        label: "File Tree",
        breadcrumb_parent: "",
    },
    CategoryNode {
        id: CategoryId::Keymap,
        parent: None,
        label: "Keymap",
        breadcrumb_parent: "",
    },
    CategoryNode {
        id: CategoryId::VersionControlGit,
        parent: None,
        label: "Git",
        breadcrumb_parent: "Version Control",
    },
    CategoryNode {
        id: CategoryId::ToolsNetwork,
        parent: None,
        label: "Sources",
        breadcrumb_parent: "",
    },
    CategoryNode {
        id: CategoryId::Advanced,
        parent: None,
        label: "Advanced Settings",
        breadcrumb_parent: "",
    },
];

/// Declare the [`Settings`] struct and the `SPECS` registry from one entry
/// per setting. Each entry, `;`-terminated field by field:
///
/// ```ignore
/// <field>: <type> = <default> ;
///     kind <SettingKind expr> ; in <CategoryId variant> / <"group"> ;
///     label <"label"> ; help <"description"> ; live <LiveApply variant> ;
///     get <|&Settings| -> SettingValue> ;
///     set <|&mut Settings, SettingValue|> ;
/// ```
///
/// Expands to `pub struct Settings { …, unknown: BTreeMap<String,String> }`,
/// its `Default`, and `pub const SPECS: &[SettingSpec]` — all in the
/// invoking module (`crate::setting`).
macro_rules! settings {
    ($(
        $key:ident : $ty:ty = $default:expr ;
            kind $kind:expr ; in $cat:ident / $group:literal ;
            label $label:literal ; help $help:literal ; live $live:ident ;
            get $get:expr ;
            set $set:expr ;
    )*) => {
        /// User-configurable app behavior, persisted to a flat `key=value`
        /// file across launches. Declared field by field via `settings!`
        /// in this module; see [`crate::setting::spec`].
        #[derive(Clone, PartialEq, Debug)]
        pub struct Settings {
            $( pub $key : $ty, )*
            /// Editor color-scheme overrides, `palette.<name>` in the file,
            /// consulted by `crate::css::theme_css`. `<name>` is a
            /// `crate::css::PALETTE` entry; the value is a `#rrggbb` hex.
            pub palette_overrides: std::collections::BTreeMap<String, String>,
            /// `key=value` lines whose key isn't in `SPECS` (and not a
            /// `palette.` override) — kept verbatim so a file written by a
            /// newer build round-trips unharmed.
            unknown: std::collections::BTreeMap<String, String>,
        }

        impl Default for Settings {
            fn default() -> Self {
                Self {
                    $( $key : $default, )*
                    palette_overrides: std::collections::BTreeMap::new(),
                    unknown: std::collections::BTreeMap::new(),
                }
            }
        }

        /// Every setting, in file + dialog order.
        pub const SPECS: &[$crate::setting::spec::SettingSpec] = &[
            $( $crate::setting::spec::SettingSpec {
                key: stringify!($key),
                kind: $kind,
                category: $crate::setting::spec::CategoryId::$cat,
                group: $group,
                label: $label,
                description: $help,
                live: $crate::setting::spec::LiveApply::$live,
                get: $get,
                set: $set,
            }, )*
        ];
    };
}
pub(crate) use settings;
