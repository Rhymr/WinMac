pub mod dialog;

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
    fn as_str(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    fn parse(s: &str) -> Option<Self> {
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
    fn as_str(self) -> &'static str {
        match self {
            IconTheme::Color => "color",
            IconTheme::Monochrome => "monochrome",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "color" => Some(IconTheme::Color),
            "monochrome" => Some(IconTheme::Monochrome),
            _ => None,
        }
    }
}

/// User-configurable app behavior, persisted across launches. New
/// `TextEditor`s read this at construction time — changing a setting takes
/// effect for tabs opened afterward, not ones already open.
#[derive(Clone, PartialEq)]
pub struct Settings {
    pub show_syllable_gutter: bool,
    pub show_vcs_gutter: bool,
    pub rhyme_highlighting: bool,
    /// Whether the rhyme highlighter treats a blank line as a stanza
    /// boundary it won't compare across, even if the other line is within
    /// its line-lookback window. On by default since rhyme schemes rarely
    /// intentionally reach across a stanza break; off lets the highlighter
    /// also catch schemes that do.
    pub rhyme_stop_at_blank_line: bool,
    pub word_completion: bool,
    pub auto_indent: bool,
    pub tab_width: u32,
    pub git_autostage: bool,
    pub theme: Theme,
    pub icon_theme: IconTheme,
    pub font_family: String,
    pub font_size: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_syllable_gutter: true,
            show_vcs_gutter: true,
            rhyme_highlighting: false,
            rhyme_stop_at_blank_line: true,
            // Off by default: dictionary-only completion misses most
            // songwriting vocabulary (slang, informal spellings), so it's
            // opt-in rather than on by default.
            word_completion: false,
            auto_indent: true,
            tab_width: 4,
            git_autostage: true,
            theme: Theme::Dark,
            icon_theme: IconTheme::Color,
            // JetBrains Mono, 13px — matches the JetBrains IDE look the
            // rest of the app's styling is chasing. Falls back to whatever
            // Pango's normal font matching picks if it isn't installed
            // (this app doesn't bundle the font file itself — see the
            // settings dialog's font picker, which only ever lists
            // already-installed system fonts).
            font_family: "JetBrains Mono".to_string(),
            font_size: 13,
        }
    }
}

fn settings_file() -> Option<PathBuf> {
    let mut dir = dirs::config_dir()?;
    dir.push("rhymr");
    fs::create_dir_all(&dir).ok()?;
    dir.push("settings.txt");
    Some(dir)
}

impl Settings {
    pub fn load() -> Self {
        let mut settings = Settings::default();
        let Some(path) = settings_file() else {
            return settings;
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            return settings;
        };

        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "show_syllable_gutter" => settings.show_syllable_gutter = value == "true",
                "show_vcs_gutter" => settings.show_vcs_gutter = value == "true",
                "rhyme_highlighting" => settings.rhyme_highlighting = value == "true",
                "rhyme_stop_at_blank_line" => settings.rhyme_stop_at_blank_line = value == "true",
                "word_completion" => settings.word_completion = value == "true",
                "auto_indent" => settings.auto_indent = value == "true",
                "tab_width" => settings.tab_width = value.parse().unwrap_or(settings.tab_width),
                "git_autostage" => settings.git_autostage = value == "true",
                "theme" => settings.theme = Theme::parse(value).unwrap_or(settings.theme),
                "icon_theme" => {
                    settings.icon_theme = IconTheme::parse(value).unwrap_or(settings.icon_theme)
                }
                "font_family" if !value.is_empty() => settings.font_family = value.to_string(),
                "font_size" => settings.font_size = value.parse().unwrap_or(settings.font_size),
                _ => {}
            }
        }

        settings
    }

    pub fn save(&self) {
        let Some(path) = settings_file() else {
            return;
        };
        // Lines are a naive `key=value` format — strip newlines from the one
        // free-text field so a pasted font name can't corrupt the file.
        let font_family = self.font_family.replace(['\n', '\r'], "");
        let contents = format!(
            "show_syllable_gutter={}\nshow_vcs_gutter={}\nrhyme_highlighting={}\nrhyme_stop_at_blank_line={}\nword_completion={}\nauto_indent={}\ntab_width={}\ngit_autostage={}\ntheme={}\nicon_theme={}\nfont_family={}\nfont_size={}\n",
            self.show_syllable_gutter,
            self.show_vcs_gutter,
            self.rhyme_highlighting,
            self.rhyme_stop_at_blank_line,
            self.word_completion,
            self.auto_indent,
            self.tab_width,
            self.git_autostage,
            self.theme.as_str(),
            self.icon_theme.as_str(),
            font_family,
            self.font_size,
        );
        let _ = fs::write(path, contents);
    }
}
