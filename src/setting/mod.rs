pub mod dialog;
pub mod spec;

use spec::{SettingKind, SettingValue};
use std::fs;
use std::path::PathBuf;

/// Light vs. dark app-wide color scheme — drives both the custom CSS
/// palette (see `crate::css`) and, in tandem, `AdwStyleManager` plus the
/// editor's GtkSourceView style scheme, so all three stay in lockstep.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            _ => None,
        }
    }
}

/// Which bundled icon set to draw the UI with (see `crate::app::icons`).
/// `Color` is the JetBrains "NetIcons" colour set; `Monochrome` is the flat
/// grey set, which then tracks [`Theme`] (dark greys vs light greys).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconTheme {
    Color,
    Monochrome,
}

impl IconTheme {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            IconTheme::Color => "color",
            IconTheme::Monochrome => "monochrome",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "color" => Some(IconTheme::Color),
            "monochrome" => Some(IconTheme::Monochrome),
            _ => None,
        }
    }
}

/// Where the Apple Notes snapshot cache is stored (see
/// `crate::source::apple_notes`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotesCacheScope {
    /// `<workspace>/.rhymr/apple-notes.json` — per project.
    Workspace,
    /// `<config dir>/rhymr/apple-notes.json` — one copy shared by every
    /// workspace.
    User,
}

impl NotesCacheScope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            NotesCacheScope::Workspace => "workspace",
            NotesCacheScope::User => "user",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "workspace" => Some(NotesCacheScope::Workspace),
            "user" => Some(NotesCacheScope::User),
            _ => None,
        }
    }
}

// ===========================================================================
// The registry. One entry per setting -> the `Settings` struct, its
// `Default`, and the `SPECS` table that `load` / `save` (and, from the next
// commit, the settings dialog) read. New `TextEditor`s read `Settings` at
// construction; live changes reach open tabs through
// `WorkspaceController::apply_settings`.
// ===========================================================================
spec::settings! {
    show_syllable_gutter: bool = true ;
        kind SettingKind::Bool ; in EditorGeneral / "Gutter" ;
        label "Show syllable count in the gutter" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.show_syllable_gutter) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.show_syllable_gutter = b } ;

    show_vcs_gutter: bool = true ;
        kind SettingKind::Bool ; in EditorGeneral / "Gutter" ;
        label "Show VCS change markers in the gutter" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.show_vcs_gutter) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.show_vcs_gutter = b } ;

    show_line_numbers: bool = true ;
        kind SettingKind::Bool ; in EditorGeneral / "Gutter" ;
        label "Show line numbers" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.show_line_numbers) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.show_line_numbers = b } ;

    highlight_current_line: bool = true ;
        kind SettingKind::Bool ; in EditorGeneral / "View" ;
        label "Highlight the current line" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.highlight_current_line) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.highlight_current_line = b } ;

    line_spacing_px: u32 = 1 ;
        kind SettingKind::Int { min: 0, max: 8, step: 1 } ; in EditorGeneral / "View" ;
        label "Extra space between lines (px)" ; help "" ; live Editor ;
        get |s| SettingValue::Int(s.line_spacing_px as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.line_spacing_px = n as u32 } ;

    wrap_lines: bool = true ;
        kind SettingKind::Bool ; in EditorGeneral / "View" ;
        label "Wrap long lines to the editor width" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.wrap_lines) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.wrap_lines = b } ;

    show_whitespace: bool = false ;
        kind SettingKind::Bool ; in EditorGeneral / "View" ;
        label "Show spaces and tabs" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.show_whitespace) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.show_whitespace = b } ;

    highlight_brackets: bool = false ;
        kind SettingKind::Bool ; in EditorGeneral / "View" ;
        label "Highlight matching brackets" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.highlight_brackets) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.highlight_brackets = b } ;

    sticky_scroll: bool = true ;
        kind SettingKind::Bool ; in EditorGeneral / "View" ;
        label "Pin the current stanza's first line to the top" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.sticky_scroll) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.sticky_scroll = b } ;

    show_right_margin: bool = false ;
        kind SettingKind::Bool ; in EditorGeneral / "Right margin" ;
        label "Show a right-margin guide" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.show_right_margin) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.show_right_margin = b } ;

    right_margin_column: u32 = 80 ;
        kind SettingKind::Int { min: 20, max: 200, step: 1 } ; in EditorGeneral / "Right margin" ;
        label "Right-margin column" ; help "" ; live Editor ;
        get |s| SettingValue::Int(s.right_margin_column as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.right_margin_column = n as u32 } ;

    rhyme_highlighting: bool = false ;
        kind SettingKind::Bool ; in EditorRhyme / "" ;
        label "Highlight rhyming syllables" ;
        help "Colors the text of syllables that rhyme with another word elsewhere in the document." ;
        live Editor ;
        get |s| SettingValue::Bool(s.rhyme_highlighting) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.rhyme_highlighting = b } ;

    rhyme_stop_at_blank_line: bool = true ;
        kind SettingKind::Bool ; in EditorRhyme / "" ;
        label "Don't match rhymes across a blank line" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.rhyme_stop_at_blank_line) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.rhyme_stop_at_blank_line = b } ;

    show_rhyme_legend: bool = true ;
        kind SettingKind::Bool ; in EditorRhyme / "" ;
        label "Show the rhyme-group legend under the editor" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.show_rhyme_legend) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.show_rhyme_legend = b } ;

    rhyme_hover_emphasis: bool = true ;
        kind SettingKind::Bool ; in EditorRhyme / "" ;
        label "Hover a word to emphasise its rhyme group" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.rhyme_hover_emphasis) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.rhyme_hover_emphasis = b } ;

    rhyme_line_window: u32 = 3 ;
        kind SettingKind::Int { min: 1, max: 8, step: 1 } ; in EditorRhyme / "Engine" ;
        label "Lines of look-back" ; help "" ; live Editor ;
        get |s| SettingValue::Int(s.rhyme_line_window as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.rhyme_line_window = n as u32 } ;

    rhyme_hue_count: u32 = 24 ;
        kind SettingKind::Int { min: 6, max: 24, step: 1 } ; in EditorRhyme / "Engine" ;
        label "Distinct group colours before the palette repeats" ; help "" ; live Editor ;
        get |s| SettingValue::Int(s.rhyme_hue_count as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.rhyme_hue_count = n as u32 } ;

    rhyme_debounce_ms: u32 = 400 ;
        kind SettingKind::Int { min: 100, max: 2000, step: 50 } ; in EditorRhyme / "Engine" ;
        label "Recompute delay after the last keystroke (ms)" ; help "" ; live Editor ;
        get |s| SettingValue::Int(s.rhyme_debounce_ms as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.rhyme_debounce_ms = n as u32 } ;

    rhyme_merge_threshold: f64 = 3.25 ;
        kind SettingKind::Float { min: 1.0, max: 6.0, step: 0.05 } ; in EditorRhyme / "Engine" ;
        label "Merge threshold" ;
        help "Minimum length-normalised score for two syllables to join the same colour group. Higher = fewer, stricter groups." ;
        live Editor ;
        get |s| SettingValue::Float(s.rhyme_merge_threshold) ;
        set |s, v| if let SettingValue::Float(f) = v { s.rhyme_merge_threshold = f } ;

    rhyme_anchor: f64 = 1.5 ;
        kind SettingKind::Float { min: 0.0, max: 5.0, step: 0.1 } ; in EditorRhyme / "Engine" ;
        label "Anchor threshold" ; help "" ; live Editor ;
        get |s| SettingValue::Float(s.rhyme_anchor) ;
        set |s, v| if let SettingValue::Float(f) = v { s.rhyme_anchor = f } ;

    rhyme_extend: f64 = 0.0 ;
        kind SettingKind::Float { min: 0.0, max: 5.0, step: 0.1 } ; in EditorRhyme / "Engine" ;
        label "Extend threshold" ; help "" ; live Editor ;
        get |s| SettingValue::Float(s.rhyme_extend) ;
        set |s, v| if let SettingValue::Float(f) = v { s.rhyme_extend = f } ;

    rhyme_jump: f64 = 2.5 ;
        kind SettingKind::Float { min: 0.0, max: 5.0, step: 0.1 } ; in EditorRhyme / "Engine" ;
        label "Jump threshold" ;
        help "Anchor-and-extend detection tunables from Hirjee & Brown. Defaults suit most lyrics; changing them alters how aggressively near-rhymes are detected." ;
        live Editor ;
        get |s| SettingValue::Float(s.rhyme_jump) ;
        set |s, v| if let SettingValue::Float(f) = v { s.rhyme_jump = f } ;

    word_completion: bool = false ;
        kind SettingKind::Bool ; in EditorCompletion / "" ;
        label "Enable dictionary word completion" ;
        help "Suggests words from the bundled dictionary as you type. Tab or Enter accepts a suggestion." ;
        live Editor ;
        get |s| SettingValue::Bool(s.word_completion) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.word_completion = b } ;

    completion_min_prefix: u32 = 2 ;
        kind SettingKind::Int { min: 1, max: 6, step: 1 } ; in EditorCompletion / "" ;
        label "Characters typed before suggesting" ; help "" ; live Editor ;
        get |s| SettingValue::Int(s.completion_min_prefix as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.completion_min_prefix = n as u32 } ;

    completion_max_suggestions: u32 = 100 ;
        kind SettingKind::Int { min: 10, max: 500, step: 10 } ; in EditorCompletion / "" ;
        label "Maximum suggestions shown" ; help "" ; live Editor ;
        get |s| SettingValue::Int(s.completion_max_suggestions as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.completion_max_suggestions = n as u32 } ;

    tree_show_dotfiles: bool = false ;
        kind SettingKind::Bool ; in EditorFileTree / "" ;
        label "Show hidden files (dotfiles)" ; help "" ; live Other ;
        get |s| SettingValue::Bool(s.tree_show_dotfiles) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.tree_show_dotfiles = b } ;

    tree_folders_first: bool = true ;
        kind SettingKind::Bool ; in EditorFileTree / "" ;
        label "List folders before files" ; help "" ; live Other ;
        get |s| SettingValue::Bool(s.tree_folders_first) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.tree_folders_first = b } ;

    tree_sort_case_sensitive: bool = false ;
        kind SettingKind::Bool ; in EditorFileTree / "" ;
        label "Case-sensitive name sorting" ; help "" ; live Other ;
        get |s| SettingValue::Bool(s.tree_sort_case_sensitive) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.tree_sort_case_sensitive = b } ;

    tree_indent_px: u32 = 16 ;
        kind SettingKind::Int { min: 8, max: 32, step: 1 } ; in EditorFileTree / "" ;
        label "Indent per level (px)" ; help "" ; live Other ;
        get |s| SettingValue::Int(s.tree_indent_px as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.tree_indent_px = n as u32 } ;

    tree_extra_ignored: String = String::new() ;
        kind SettingKind::Text ; in EditorFileTree / "" ;
        label "Also hide (comma-separated)" ;
        help "Names hidden in the project tree in addition to target and node_modules — e.g. \"dist, .cache\"." ;
        live Other ;
        get |s| SettingValue::Text(s.tree_extra_ignored.clone()) ;
        set |s, v| if let SettingValue::Text(t) = v { s.tree_extra_ignored = t } ;

    auto_indent: bool = true ;
        kind SettingKind::Bool ; in EditorGeneral / "Indentation" ;
        label "Auto-indent new lines" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.auto_indent) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.auto_indent = b } ;

    insert_spaces: bool = false ;
        kind SettingKind::Bool ; in EditorGeneral / "Indentation" ;
        label "Insert spaces instead of tabs" ; help "" ; live Editor ;
        get |s| SettingValue::Bool(s.insert_spaces) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.insert_spaces = b } ;

    tab_width: u32 = 4 ;
        kind SettingKind::Int { min: 1, max: 8, step: 1 } ; in EditorGeneral / "Indentation" ;
        label "Tab width" ; help "" ; live Editor ;
        get |s| SettingValue::Int(s.tab_width as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.tab_width = n as u32 } ;

    autosave_debounce_ms: u32 = 600 ;
        kind SettingKind::Int { min: 100, max: 5000, step: 50 } ; in EditorGeneral / "Saving" ;
        label "Autosave delay after the last keystroke (ms)" ; help "" ; live Editor ;
        get |s| SettingValue::Int(s.autosave_debounce_ms as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.autosave_debounce_ms = n as u32 } ;

    git_autostage: bool = true ;
        kind SettingKind::Bool ; in VersionControlGit / "" ;
        label "Automatically stage changes when saving" ; help "" ; live Other ;
        get |s| SettingValue::Bool(s.git_autostage) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.git_autostage = b } ;

    git_remote_name: String = "origin".to_string() ;
        kind SettingKind::Text ; in VersionControlGit / "Remote" ;
        label "Remote name" ; help "" ; live Other ;
        get |s| SettingValue::Text(s.git_remote_name.clone()) ;
        set |s, v| if let SettingValue::Text(t) = v
            && !t.is_empty()
        {
            s.git_remote_name = t
        } ;

    git_default_commit_message: String = "Update".to_string() ;
        kind SettingKind::Text ; in VersionControlGit / "Commits" ;
        label "Default commit message" ;
        help "Used when the commit dialog's message box is left empty." ;
        live Other ;
        get |s| SettingValue::Text(s.git_default_commit_message.clone()) ;
        set |s, v| if let SettingValue::Text(t) = v
            && !t.is_empty()
        {
            s.git_default_commit_message = t
        } ;

    git_signature_fallback: String = "Pneuma <pneuma@local>".to_string() ;
        kind SettingKind::Text ; in VersionControlGit / "Commits" ;
        label "Fallback author" ;
        help "\"Name <email>\" used to sign a commit when git has no user.name / user.email configured." ;
        live Other ;
        get |s| SettingValue::Text(s.git_signature_fallback.clone()) ;
        set |s, v| if let SettingValue::Text(t) = v
            && !t.is_empty()
        {
            s.git_signature_fallback = t
        } ;

    notes_cache_scope: NotesCacheScope = NotesCacheScope::Workspace ;
        kind SettingKind::Enum {
            values: &["workspace", "user"],
            labels: &["This workspace", "All workspaces"],
        } ;
        in ToolsNetwork / "" ;
        label "Apple Notes cache" ;
        help "Where the Apple Notes snapshot is stored. \"All workspaces\" keeps one shared copy under your user config dir instead of per-project .rhymr/." ;
        live Other ;
        get |s| SettingValue::Text(s.notes_cache_scope.as_str().to_string()) ;
        set |s, v| if let SettingValue::Text(t) = v
            && let Some(x) = NotesCacheScope::parse(&t)
        {
            s.notes_cache_scope = x
        } ;

    apple_notes_refresh_secs: u32 = 300 ;
        kind SettingKind::Int { min: 30, max: 3600, step: 30 } ; in ToolsNetwork / "" ;
        label "Apple Notes refresh interval (s)" ; help "" ; live Restart ;
        get |s| SettingValue::Int(s.apple_notes_refresh_secs as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.apple_notes_refresh_secs = n as u32 } ;

    rhyme_lookup_timeout_secs: u32 = 8 ;
        kind SettingKind::Int { min: 1, max: 60, step: 1 } ; in ToolsNetwork / "" ;
        label "Rhyme search timeout (s)" ;
        help "How long to wait on the Datamuse rhyme lookup before falling back to the local dictionary." ;
        live Other ;
        get |s| SettingValue::Int(s.rhyme_lookup_timeout_secs as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.rhyme_lookup_timeout_secs = n as u32 } ;

    theme: Theme = Theme::Dark ;
        kind SettingKind::Enum { values: &["dark", "light"], labels: &["Dark", "Light"] } ;
        in Appearance / "" ;
        label "Theme" ; help "" ; live EditorAndCss ;
        get |s| SettingValue::Text(s.theme.as_str().to_string()) ;
        set |s, v| if let SettingValue::Text(t) = v
            && let Some(x) = Theme::parse(&t)
        {
            s.theme = x
        } ;

    icon_theme: IconTheme = IconTheme::Color ;
        kind SettingKind::Enum {
            values: &["color", "monochrome"],
            labels: &["Color", "Monochrome"],
        } ;
        in Appearance / "" ;
        label "Icons" ; help "" ; live Other ;
        get |s| SettingValue::Text(s.icon_theme.as_str().to_string()) ;
        set |s, v| if let SettingValue::Text(t) = v
            && let Some(x) = IconTheme::parse(&t)
        {
            s.icon_theme = x
        } ;

    font_family: String = "JetBrains Mono".to_string() ;
        kind SettingKind::Font ; in Appearance / "" ;
        label "Editor font" ; help "" ; live Css ;
        get |s| SettingValue::Text(s.font_family.clone()) ;
        set |s, v| if let SettingValue::Text(t) = v
            && !t.is_empty()
        {
            s.font_family = t
        } ;

    font_size: u32 = 13 ;
        kind SettingKind::FontSize { min: 6, max: 96 } ; in Appearance / "" ;
        label "Editor font size" ; help "" ; live Css ;
        get |s| SettingValue::Int(s.font_size as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.font_size = n as u32 } ;

    ui_font_size: u32 = 12 ;
        kind SettingKind::Int { min: 8, max: 24, step: 1 } ; in Appearance / "Interface" ;
        label "Interface font size (px)" ;
        help "Applies to the toolbar, trees, dialogs and menus — not the editor." ;
        live Css ;
        get |s| SettingValue::Int(s.ui_font_size as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.ui_font_size = n as u32 } ;

    corner_radius: u32 = 0 ;
        kind SettingKind::Int { min: 0, max: 8, step: 1 } ; in Appearance / "Interface" ;
        label "Corner radius (px)" ;
        help "0 keeps the classic boxy look; higher rounds buttons, menus and inputs." ;
        live Css ;
        get |s| SettingValue::Int(s.corner_radius as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.corner_radius = n as u32 } ;

    animations_enabled: bool = true ;
        kind SettingKind::Bool ; in Appearance / "Interface" ;
        label "Enable UI animations" ; help "" ; live Css ;
        get |s| SettingValue::Bool(s.animations_enabled) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.animations_enabled = b } ;

    reopen_last_project: bool = false ;
        kind SettingKind::Bool ; in AppearanceWindow / "Startup" ;
        label "Reopen the last project on launch" ;
        help "Skips the welcome window and opens the most recently used project." ;
        live Restart ;
        get |s| SettingValue::Bool(s.reopen_last_project) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.reopen_last_project = b } ;

    show_splash: bool = true ;
        kind SettingKind::Bool ; in AppearanceWindow / "Startup" ;
        label "Show the splash screen" ; help "" ; live Restart ;
        get |s| SettingValue::Bool(s.show_splash) ;
        set |s, v| if let SettingValue::Bool(b) = v { s.show_splash = b } ;

    splash_duration_ms: u32 = 1600 ;
        kind SettingKind::Int { min: 300, max: 5000, step: 100 } ; in AppearanceWindow / "Startup" ;
        label "Splash duration (ms)" ; help "" ; live Restart ;
        get |s| SettingValue::Int(s.splash_duration_ms as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.splash_duration_ms = n as u32 } ;

    window_width: u32 = 1280 ;
        kind SettingKind::Int { min: 640, max: 6000, step: 10 } ; in AppearanceWindow / "Default sizes" ;
        label "Main window width (px)" ; help "" ; live Restart ;
        get |s| SettingValue::Int(s.window_width as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.window_width = n as u32 } ;

    window_height: u32 = 720 ;
        kind SettingKind::Int { min: 480, max: 4000, step: 10 } ; in AppearanceWindow / "Default sizes" ;
        label "Main window height (px)" ; help "" ; live Restart ;
        get |s| SettingValue::Int(s.window_height as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.window_height = n as u32 } ;

    left_panel_width: u32 = 300 ;
        kind SettingKind::Int { min: 120, max: 900, step: 10 } ; in AppearanceWindow / "Default sizes" ;
        label "Project panel width (px)" ;
        help "Starting width for a project with no remembered layout." ;
        live Restart ;
        get |s| SettingValue::Int(s.left_panel_width as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.left_panel_width = n as u32 } ;

    rhyme_panel_height: u32 = 240 ;
        kind SettingKind::Int { min: 80, max: 900, step: 10 } ; in AppearanceWindow / "Default sizes" ;
        label "Rhyme Search panel height (px)" ; help "" ; live Restart ;
        get |s| SettingValue::Int(s.rhyme_panel_height as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.rhyme_panel_height = n as u32 } ;

    recent_projects_limit: u32 = 10 ;
        kind SettingKind::Int { min: 3, max: 50, step: 1 } ; in Advanced / "" ;
        label "Recent projects to remember" ; help "" ; live Restart ;
        get |s| SettingValue::Int(s.recent_projects_limit as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.recent_projects_limit = n as u32 } ;

    tab_title_max_chars: u32 = 18 ;
        kind SettingKind::Int { min: 8, max: 60, step: 1 } ; in Advanced / "" ;
        label "Tab title length before it's truncated" ; help "" ; live Restart ;
        get |s| SettingValue::Int(s.tab_title_max_chars as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.tab_title_max_chars = n as u32 } ;

    rhyme_paint_margin_lines: u32 = 64 ;
        kind SettingKind::Int { min: 0, max: 400, step: 8 } ; in Advanced / "" ;
        label "Rhyme paint margin (lines)" ;
        help "How far past the visible range rhyme colours are painted ahead of scrolling." ;
        live Editor ;
        get |s| SettingValue::Int(s.rhyme_paint_margin_lines as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.rhyme_paint_margin_lines = n as u32 } ;

    rhyme_scroll_debounce_ms: u32 = 120 ;
        kind SettingKind::Int { min: 0, max: 1000, step: 10 } ; in Advanced / "" ;
        label "Rhyme repaint delay after scrolling (ms)" ; help "" ; live Editor ;
        get |s| SettingValue::Int(s.rhyme_scroll_debounce_ms as i64) ;
        set |s, v| if let SettingValue::Int(n) = v { s.rhyme_scroll_debounce_ms = n as u32 } ;
}

fn settings_file() -> Option<PathBuf> {
    let mut dir = dirs::config_dir()?;
    dir.push("rhymr");
    fs::create_dir_all(&dir).ok()?;
    dir.push("settings.txt");
    Some(dir)
}

/// Fold a flat `key=value` document into `settings`: known keys through
/// their spec (validated and clamped), unknown keys stashed verbatim so a
/// file from a newer build round-trips unharmed. A line without `=`, or one
/// whose value the spec can't use, is skipped — the field keeps its current
/// (default) value, as the old lenient `load` did.
fn parse_into(settings: &mut Settings, contents: &str) {
    for line in contents.lines() {
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let (key, raw) = (key.trim(), raw.trim());
        if let Some(name) = key.strip_prefix("palette.") {
            if is_hex_color(raw) && !name.is_empty() {
                settings
                    .palette_overrides
                    .insert(name.to_string(), raw.to_lowercase());
            }
            continue;
        }
        match SPECS.iter().find(|spec| spec.key == key) {
            Some(spec) => {
                if let Some(value) = spec.kind.parse_clamped(raw) {
                    (spec.set)(settings, value);
                }
            }
            None => {
                settings
                    .unknown
                    .insert(key.to_string(), raw.replace(['\n', '\r'], ""));
            }
        }
    }
}

/// `#rgb` or `#rrggbb`.
fn is_hex_color(s: &str) -> bool {
    let Some(hex) = s.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Render `settings` to the flat format: every spec in registry order, then
/// the color-scheme overrides, then any stashed unknown keys (all sorted,
/// from their `BTreeMap`s).
fn serialize(settings: &Settings) -> String {
    let mut out = String::new();
    for spec in SPECS {
        out.push_str(spec.key);
        out.push('=');
        out.push_str(&(spec.get)(settings).render());
        out.push('\n');
    }
    for (name, hex) in &settings.palette_overrides {
        out.push_str("palette.");
        out.push_str(name);
        out.push('=');
        out.push_str(hex);
        out.push('\n');
    }
    for (key, value) in &settings.unknown {
        out.push_str(key);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    out
}

impl Settings {
    /// Load from `<config dir>/rhymr/settings.txt`, falling back to
    /// [`Settings::default`] for a missing or unreadable file and for any
    /// unparseable line.
    pub fn load() -> Self {
        let mut settings = Settings::default();
        let Some(path) = settings_file() else {
            return settings;
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            return settings;
        };
        parse_into(&mut settings, &contents);
        settings
    }

    /// Write every setting back in registry order, plus any keys a newer
    /// build left behind. A write failure is logged, not propagated —
    /// settings are non-critical and the app stays usable without them.
    pub fn save(&self) {
        let Some(path) = settings_file() else {
            return;
        };
        if let Err(err) = fs::write(&path, serialize(self)) {
            log::warn!("could not save settings to {}: {err}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips_through_the_flat_format() {
        let original = Settings::default();
        let mut reloaded = Settings::default();
        parse_into(&mut reloaded, &serialize(&original));
        assert_eq!(original, reloaded);
    }

    #[test]
    fn unknown_keys_survive_a_load_and_save() {
        let mut settings = Settings::default();
        parse_into(
            &mut settings,
            "theme=light\nfuture_toggle=true\nvendor.note=hello world\n",
        );
        assert_eq!(settings.theme, Theme::Light);

        let text = serialize(&settings);
        assert!(text.contains("future_toggle=true"));
        assert!(text.contains("vendor.note=hello world"));

        // A second round trip is stable.
        let mut again = Settings::default();
        parse_into(&mut again, &text);
        assert_eq!(settings, again);
    }

    #[test]
    fn out_of_range_ints_are_clamped_on_load() {
        let mut settings = Settings::default();
        parse_into(&mut settings, "tab_width=999\nfont_size=2\n");
        assert_eq!(settings.tab_width, 8);
        assert_eq!(settings.font_size, 6);
    }

    #[test]
    fn floats_round_trip_and_clamp_on_load() {
        let mut settings = Settings::default();
        parse_into(
            &mut settings,
            "rhyme_merge_threshold=2.75\nrhyme_anchor=99\nrhyme_extend=-1\n",
        );
        assert_eq!(settings.rhyme_merge_threshold, 2.75);
        assert_eq!(settings.rhyme_anchor, 5.0); // clamped to max
        assert_eq!(settings.rhyme_extend, 0.0); // clamped to min

        let mut again = Settings::default();
        parse_into(&mut again, &serialize(&settings));
        assert_eq!(settings, again);
    }

    #[test]
    fn palette_overrides_round_trip_and_reject_non_hex() {
        let mut settings = Settings::default();
        parse_into(
            &mut settings,
            "palette.bg-dark=#123456\npalette.text-bright=not-a-color\npalette.=#ffffff\n",
        );
        assert_eq!(
            settings.palette_overrides.get("bg-dark").unwrap(),
            "#123456"
        );
        assert!(!settings.palette_overrides.contains_key("text-bright"));
        assert!(settings.palette_overrides.len() == 1);

        let text = serialize(&settings);
        assert!(text.contains("palette.bg-dark=#123456"));
        let mut again = Settings::default();
        parse_into(&mut again, &text);
        assert_eq!(settings, again);
    }

    #[test]
    fn unknown_enum_token_keeps_the_default() {
        let mut settings = Settings::default();
        parse_into(&mut settings, "theme=chartreuse\nicon_theme=neon\n");
        assert_eq!(settings.theme, Theme::Dark);
        assert_eq!(settings.icon_theme, IconTheme::Color);
    }

    #[test]
    fn a_legacy_settings_file_loads_as_expected() {
        // The exact 15-line shape older builds wrote.
        let legacy = "show_syllable_gutter=false\n\
             show_vcs_gutter=true\n\
             rhyme_highlighting=true\n\
             rhyme_stop_at_blank_line=false\n\
             show_rhyme_legend=false\n\
             rhyme_hover_emphasis=true\n\
             word_completion=true\n\
             auto_indent=false\n\
             tab_width=2\n\
             git_autostage=false\n\
             notes_cache_scope=user\n\
             theme=light\n\
             icon_theme=monochrome\n\
             font_family=Iosevka\n\
             font_size=15\n";
        let mut settings = Settings::default();
        parse_into(&mut settings, legacy);

        assert!(!settings.show_syllable_gutter);
        assert!(settings.rhyme_highlighting);
        assert!(!settings.auto_indent);
        assert_eq!(settings.tab_width, 2);
        assert!(!settings.git_autostage);
        assert_eq!(settings.notes_cache_scope, NotesCacheScope::User);
        assert_eq!(settings.theme, Theme::Light);
        assert_eq!(settings.icon_theme, IconTheme::Monochrome);
        assert_eq!(settings.font_family, "Iosevka");
        assert_eq!(settings.font_size, 15);
    }
}
