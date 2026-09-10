use crate::setting::{Settings, Theme};
use grass::Options;
use gtk::{CssProvider, gdk};
use std::cell::RefCell;
use std::path::PathBuf;

/// SCSS stems under `assets/scss/`, combined into the app's stylesheet at
/// launch by `build_css`. The build script (`scripts.rs`) mirrors this list
/// to precompile `assets/css/`; a new stylesheet's stem goes in `CSS_FILES`
/// here **and** in `scripts.rs`.
const CSS_FILES: [&str; 16] = [
    "base",
    "chrome",
    "context_menu",
    "dialog",
    "dock",
    "editor",
    "empty_state",
    "file_tree",
    "git_log",
    "layout",
    "notebook",
    "rhyme_search",
    "settings",
    "splash",
    "status_bar",
    "welcome",
];

/// Absolute path to `assets/scss/<stem>.scss`, resolved against
/// [`crate::config::assets_dir`] so a bundled `.app` (working directory `/`)
/// finds its stylesheets too.
fn scss_path(stem: &str) -> PathBuf {
    crate::config::assets_dir()
        .join("scss")
        .join(format!("{stem}.scss"))
}

/// Grass options shared by every compile: a load path pointing at
/// `assets/scss/` so `@use "mixins"` resolves regardless of the working
/// directory.
fn scss_options() -> Options<'static> {
    Options::default().load_path(crate::config::assets_dir().join("scss"))
}

/// Every themeable color, as (css-var-name, dark-value, light-value).
/// The single source of truth for both palettes — `theme_css()` below is
/// the only place that reads this, so base.scss carries no hardcoded
/// `:root` colors of its own to drift out of sync with a second copy here.
/// The Color Scheme settings page builds a picker per entry.
pub(crate) const PALETTE: &[(&str, &str, &str)] = &[
    // Dark column = classic Darcula; light column = classic "IntelliJ Light".
    ("bg-darkest", "#2b2b2b", "#ffffff"),
    ("bg-dark", "#3c3f41", "#ececec"),
    ("bg-mid", "#45494a", "#ffffff"),
    ("bg-light", "#4e5254", "#d9d9d9"),
    ("bg-hover", "#4b4f51", "#ededed"),
    ("text-bright", "#bbbbbb", "#1d1d1d"),
    ("text-dim", "#a9b7c6", "#2b2b2b"),
    ("text-not-so-dim", "#a0a0a0", "#4a4a4a"),
    ("text-muted", "#808080", "#8c8c8c"),
    ("text-number", "#606366", "#9a9a9a"),
    ("text-green", "#6a8759", "#4a8f3c"),
    // Editor gutter — a shade off the editor background, JetBrains-style.
    ("gutter-bg", "#313335", "#f0f0f0"),
    // The syllable count in the gutter is always this green, both themes.
    ("syllable-green", "#57a64a", "#3a8a2e"),
    // Sticky stanza-line text — matches the editor scheme's body text fg
    // (rhymr.xml / rhymr-light.xml), which `--text-*` don't track per theme.
    ("sticky-fg", "#a9b7c6", "#000000"),
    ("text-modified", "#d19a66", "#a85f1d"),
    ("text-new", "#6fbf73", "#1f8a3d"),
    ("text-renamed", "#61afef", "#1568c9"),
    // Git-ignored entries and read-only external-source trees — instead of
    // tinting the label text, the row sits on this muted goldenrod so the
    // name keeps its default colour and the "ignored / read-only" state
    // reads as a highlight.
    ("bg-ignored", "#544628", "#ede0b3"),
    // VCS gutter change bars (JetBrains convention: green add / blue modify).
    ("vcs-added", "#59a869", "#4a8f3c"),
    ("vcs-modified", "#4a88c7", "#3573b8"),
    ("border-dark", "#2b2b2b", "#c0c0c0"),
    ("border-light", "#4c4c4c", "#c0c0c0"),
    ("border-hover", "#5e6060", "#a6a6a6"),
    ("border-active", "#6b6b6b", "#6e6e6e"),
    ("button-hover", "#4c5052", "#e0e0e0"),
    ("button-active", "#5a5d5f", "#d0d0d0"),
    ("button-disabled", "#3a3d3f", "#f0f0f0"),
    ("selection-bg", "#2f65ca", "#2675bf"),
    ("selection-hover", "#365880", "#4080c0"),
    ("selection-active", "#1f4a7a", "#1c5a9e"),
    // Elevated surface for popovers/context menus — a shade off the panel
    // background so the menu reads as floating above it.
    ("popover-bg-color", "#3c3f41", "#ffffff"),
    ("popover-fg-color", "#bbbbbb", "#1d1d1d"),
    // Text/icon color for anything painted on top of an accent/destructive
    // surface (selected rows, suggested/destructive buttons) — always
    // light, independent of `--text-bright`, which flips per theme.
    ("selection-fg", "#ffffff", "#ffffff"),
    ("destructive", "#c75450", "#c0392b"),
    ("destructive-hover", "#d16460", "#d0473a"),
    ("destructive-active", "#a5423f", "#a32f24"),
    ("destructive-text", "#e57474", "#b3261e"),
];

/// Theme-invariant tokens that don't depend on any setting. The
/// setting-driven ones (`--radius-*`, `--transition-*`, `--ui-font-size`)
/// are emitted by `theme_css` instead, which is appended last so it wins.
const TOKENS: &str = r#":root {
    /* Chrome (everything outside the editor) uses the OS UI font, classic-IDE
       style; the editor + its gutter keep the monospace `--app-font-*` set
       from Settings (see editor.scss). Pango picks the first installed
       family from the list; unknown names are skipped. */
    --ui-font-family: "SF Pro Text", "Helvetica Neue", "Segoe UI", Cantarell, "Ubuntu", "Noto Sans", sans-serif;
}
"#;

thread_local! {
    static PROVIDER: RefCell<Option<CssProvider>> = const { RefCell::new(None) };
}

/// The themed default for palette entry `name`, with no user override
/// applied — the Color Scheme settings page seeds each picker from this.
pub(crate) fn palette_default(name: &str, theme: Theme) -> Option<&'static str> {
    PALETTE
        .iter()
        .find(|(n, ..)| *n == name)
        .map(|(_, dark, light)| if theme == Theme::Dark { *dark } else { *light })
}

/// The runtime-generated `:root { ... }` block: theme-invariant tokens,
/// the active theme's palette (with any `Settings::palette_overrides`
/// applied), and the current font settings. Appended after the compiled
/// SCSS so it's the single place driving both the custom-widget palette and
/// (via matching `--accent-*`/`--destructive-*` names) libadwaita's own
/// chrome — see `sync_style_manager` for the other half of theme switching,
/// which points `AdwStyleManager` at the same `Settings.theme`.
fn theme_css(settings: &Settings) -> String {
    let is_dark = settings.theme == Theme::Dark;
    // A user color-scheme override wins over the themed default, for both
    // our own `--name` vars and the libadwaita mirror below.
    let color = |name: &str, themed: &str| -> String {
        settings
            .palette_overrides
            .get(name)
            .cloned()
            .unwrap_or_else(|| themed.to_string())
    };

    let mut vars = String::new();
    for (name, dark, light) in PALETTE {
        let themed = if is_dark { dark } else { light };
        vars.push_str(&format!("    --{name}: {};\n", color(name, themed)));
    }

    // Mirror the accent/destructive roles onto libadwaita's own named
    // colors so native Adwaita chrome (the header bar, its buttons) reads
    // as part of the same system rather than stock GNOME blue/red.
    let selection_bg = color("selection-bg", if is_dark { "#2f65ca" } else { "#2675bf" });
    let selection_hover = color(
        "selection-hover",
        if is_dark { "#365880" } else { "#4080c0" },
    );
    let destructive = color("destructive", if is_dark { "#c75450" } else { "#c0392b" });
    vars.push_str(&format!(
        "    --accent-bg-color: {selection_bg};\n    --accent-color: {selection_hover};\n    --accent-fg-color: #ffffff;\n"
    ));
    vars.push_str(&format!(
        "    --destructive-bg-color: {destructive};\n    --destructive-color: {destructive};\n    --destructive-fg-color: #ffffff;\n"
    ));

    // App-wide font — applied on the universal `* {}` reset in base.scss,
    // so it's the default for every widget, not just the editor. Sanitized
    // since this becomes raw CSS text, not just a string literal's
    // contents, so it must not be able to break out of the declaration.
    let font_family = sanitize_css_text(&settings.font_family);
    vars.push_str(&format!(
        "    --app-font-family: \"{font_family}\";\n    --app-font-size: {}pt;\n",
        settings.font_size
    ));

    // Setting-driven chrome tokens (Settings → Appearance). `--radius-*` are
    // kept as named hooks even at 0 so the stylesheets don't need editing.
    vars.push_str(&format!(
        "    --ui-font-size: {}px;\n",
        settings.ui_font_size
    ));
    let radius = settings.corner_radius;
    vars.push_str(&format!(
        "    --radius-sm: {radius}px;\n    --radius-md: {radius}px;\n    --radius-lg: {radius}px;\n"
    ));
    let (fast, normal) = if settings.animations_enabled {
        ("60ms linear", "90ms linear")
    } else {
        ("0s", "0s")
    };
    vars.push_str(&format!(
        "    --transition-fast: {fast};\n    --transition-normal: {normal};\n"
    ));

    format!(":root {{\n{vars}}}\n")
}

fn sanitize_css_text(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '"' | '\\' | '\n' | '\r' | '{' | '}' | ';'))
        .collect()
}

fn build_css(settings: &Settings) -> String {
    // Grass prepends `@charset "UTF-8";` to any file it compiles that
    // references a non-ASCII character (e.g. the "›" submenu arrow or
    // curly quotes in a comment) — GTK's CSS parser doesn't recognize the
    // rule at all (`Unknown @ rule`), so it's stripped rather than kept
    // (the string is already UTF-8; there's nothing for it to declare).
    let mut combined_css = String::from(TOKENS);

    let options = scss_options();
    for stem in CSS_FILES {
        let path = scss_path(stem);
        match grass::from_path(&path, &options) {
            Ok(css) => {
                let css = css
                    .strip_prefix("@charset \"UTF-8\";")
                    .unwrap_or(&css)
                    .trim_start();
                combined_css.push_str(css);
                combined_css.push('\n');
            }
            Err(err) => log::error!("failed to compile {}: {err}", path.display()),
        }
    }

    combined_css.push_str(&theme_css(settings));
    combined_css
}

/// Build the CSS provider for `settings` and remember it, so later calls
/// to `reload()` (e.g. from the settings dialog's Apply/OK) can restyle
/// the whole app in place without tearing down and re-adding a provider.
pub fn init(settings: &Settings) -> CssProvider {
    let provider = CssProvider::new();
    provider.load_from_string(&build_css(settings));
    PROVIDER.with(|p| *p.borrow_mut() = Some(provider.clone()));
    provider
}

/// Re-render the stored provider's CSS from `settings` — the live-apply
/// path for theme/font changes made in the settings dialog.
pub fn reload(settings: &Settings) {
    PROVIDER.with(|p| {
        if let Some(provider) = p.borrow().as_ref() {
            provider.load_from_string(&build_css(settings));
        }
    });
}

/// Point `AdwStyleManager` at the same theme as our own CSS palette, so
/// genuine libadwaita chrome (the header bar) switches light/dark in
/// lockstep with the rest of the app instead of following the OS.
pub fn sync_style_manager(settings: &Settings) {
    let scheme = match settings.theme {
        Theme::Dark => libadwaita::ColorScheme::ForceDark,
        Theme::Light => libadwaita::ColorScheme::ForceLight,
    };
    libadwaita::StyleManager::default().set_color_scheme(scheme);
}

pub fn apply_css_to_app(css_provider: &CssProvider) {
    if let Some(default_display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &default_display,
            css_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
