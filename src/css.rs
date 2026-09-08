use crate::setting::{Settings, Theme};
use grass::Options;
use gtk::{CssProvider, gdk};
use std::cell::RefCell;
use std::fs;

const CSS_FILES: [&str; 12] = [
    "assets/{1}/base.{1}",
    "assets/{1}/context_menu.{1}",
    "assets/{1}/dialog.{1}",
    "assets/{1}/editor.{1}",
    "assets/{1}/empty_state.{1}",
    "assets/{1}/file_tree.{1}",
    "assets/{1}/layout.{1}",
    "assets/{1}/notebook.{1}",
    "assets/{1}/rhyme_search.{1}",
    "assets/{1}/settings.{1}",
    "assets/{1}/status_bar.{1}",
    "assets/{1}/welcome.{1}",
];

/// Every themeable color, as (css-var-name, dark-value, light-value).
/// The single source of truth for both palettes — `theme_css()` below is
/// the only place that reads this, so base.scss carries no hardcoded
/// `:root` colors of its own to drift out of sync with a second copy here.
const PALETTE: &[(&str, &str, &str)] = &[
    ("bg-darkest", "#1e1e1e", "#ffffff"),
    ("bg-dark", "#2e2e2e", "#f3f3f3"),
    ("bg-mid", "#3e3e3e", "#ececec"),
    ("bg-light", "#4e4e4e", "#dcdcdc"),
    ("bg-hover", "#404040", "#e6e6e6"),
    ("text-bright", "#ffffff", "#1a1a1a"),
    ("text-dim", "#e1e1e1", "#2b2b2b"),
    ("text-not-so-dim", "#bababa", "#4a4a4a"),
    ("text-muted", "#888888", "#767676"),
    ("text-number", "#666666", "#9a9a9a"),
    ("text-green", "#00ff00", "#1a7f37"),
    ("text-modified", "#d19a66", "#a85f1d"),
    ("text-new", "#6fbf73", "#1f8a3d"),
    ("text-renamed", "#61afef", "#1568c9"),
    ("border-dark", "#000000", "#d0d0d0"),
    ("border-light", "#5e5e5e", "#c7c7c7"),
    ("border-hover", "#cfcfcf", "#8a8a8a"),
    ("border-active", "#8e8e8e", "#6e6e6e"),
    ("button-hover", "#707070", "#dcdcdc"),
    ("button-active", "#484848", "#cacaca"),
    ("button-disabled", "#2c2c2c", "#f0f0f0"),
    ("selection-bg", "#2b5278", "#3a6ea5"),
    ("selection-hover", "#366391", "#4a80b8"),
    ("selection-active", "#1e4271", "#2c5680"),
    // Elevated surface for popovers/context menus — a shade off the panel
    // background so the menu reads as floating above it.
    ("popover-bg-color", "#34373c", "#ffffff"),
    ("popover-fg-color", "#ffffff", "#1a1a1a"),
    // Text/icon color for anything painted on top of an accent/destructive
    // surface (selected rows, suggested/destructive buttons) — always
    // light, independent of `--text-bright`, which flips per theme.
    ("selection-fg", "#ffffff", "#ffffff"),
    ("destructive", "#b23b3b", "#c53030"),
    ("destructive-hover", "#c94444", "#d64545"),
    ("destructive-active", "#942f2f", "#a52a2a"),
    ("destructive-text", "#e57474", "#b3261e"),
];

/// Theme-invariant spacing/radius/motion tokens, applied consistently
/// across every custom widget for a cohesive, modern feel.
const TOKENS: &str = r#":root {
    --radius-sm: 4px;
    --radius-md: 6px;
    --radius-lg: 9px;
    --transition-fast: 100ms ease-out;
    --transition-normal: 150ms ease-out;
}
"#;

thread_local! {
    static PROVIDER: RefCell<Option<CssProvider>> = const { RefCell::new(None) };
}

pub fn compile_sass() -> Result<(), Box<dyn std::error::Error>> {
    for css_file in CSS_FILES {
        let scss_path = css_file.replace("{1}", "scss");
        let css_path = css_file.replace("{1}", "css");

        println!("Compiling {scss_path}");
        let css_output = grass::from_path(&scss_path, &Options::default())?;
        fs::write(css_path, css_output)?;
    }

    Ok(())
}

/// The runtime-generated `:root { ... }` block: theme-invariant tokens,
/// the active theme's palette, and the current font settings. Appended
/// after the compiled SCSS so it's the single place driving both the
/// custom-widget palette and (via matching `--accent-*`/`--destructive-*`
/// names) libadwaita's own chrome — see `sync_style_manager` for the other
/// half of theme switching, which points `AdwStyleManager` at the same
/// `Settings.theme`.
fn theme_css(settings: &Settings) -> String {
    let is_dark = settings.theme == Theme::Dark;
    let mut vars = String::new();
    for (name, dark, light) in PALETTE {
        let value = if is_dark { dark } else { light };
        vars.push_str(&format!("    --{name}: {value};\n"));
    }

    // Mirror the accent/destructive roles onto libadwaita's own named
    // colors so native Adwaita chrome (the header bar, its buttons) reads
    // as part of the same system rather than stock GNOME blue/red.
    let selection_bg = if is_dark { "#2b5278" } else { "#3a6ea5" };
    let selection_hover = if is_dark { "#366391" } else { "#4a80b8" };
    let destructive = if is_dark { "#b23b3b" } else { "#c53030" };
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

    for css_file in CSS_FILES {
        let scss_path = css_file.replace("{1}", "scss");
        match grass::from_path(&scss_path, &Options::default()) {
            Ok(css) => {
                let css = css
                    .strip_prefix("@charset \"UTF-8\";")
                    .unwrap_or(&css)
                    .trim_start();
                combined_css.push_str(css);
                combined_css.push('\n');
            }
            Err(err) => eprintln!("Failed to compile {scss_path}: {err}"),
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
