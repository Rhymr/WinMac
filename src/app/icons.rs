//! One place to build the app's icons — a bundled subset of JetBrains'
//! "NetIcons" set (`assets/icons/{color,dark,light}/<name>-<variant>.svg`,
//! Apache-2.0, see `assets/icons/NOTICE`), so every icon in the UI shares
//! one style. Bundled rather than themed because the system icon theme is
//! missing several of the names the toolbar needs.
//!
//! Each icon ships in three variants: `color` (the gradient NetIcons look),
//! and flat `dark` / `light` greys for the "Monochrome" icon theme. Which
//! one [`img`] resolves is set once per launch (and on a settings change)
//! by [`set_variant`], from `Settings::icon_theme` in tandem with the app
//! `Theme`. Widgets already built keep whatever variant they were made
//! with — a full switch shows on the next launch — matching how the rest
//! of `Settings` takes effect.

use std::cell::Cell;

use crate::setting::{IconTheme, Settings, Theme};

/// The variant directory used for every icon path.
#[derive(Clone, Copy)]
enum Variant {
    Color,
    Dark,
    Light,
}

impl Variant {
    fn dir(self) -> &'static str {
        match self {
            Variant::Color => "color",
            Variant::Dark => "dark",
            Variant::Light => "light",
        }
    }
}

thread_local! {
    static VARIANT: Cell<Variant> = const { Cell::new(Variant::Color) };
}

/// Pick the icon variant for `settings`: `Color` → the colour set;
/// `Monochrome` → the grey set for the current light/dark `Theme`. Call
/// this before building any chrome (and again when settings are applied).
pub fn set_variant(settings: &Settings) {
    let variant = match settings.icon_theme {
        IconTheme::Color => Variant::Color,
        IconTheme::Monochrome => match settings.theme {
            Theme::Dark => Variant::Dark,
            Theme::Light => Variant::Light,
        },
    };
    VARIANT.with(|v| v.set(variant));
}

fn dir() -> &'static str {
    VARIANT.with(|v| v.get()).dir()
}

/// A `GtkImage` for the bundled icon `name` (`"git-commit"`, `"file"`, …),
/// in the current variant, sized to `px`.
pub fn img(name: &str, px: i32) -> gtk::Image {
    let dir = dir();
    let image =
        gtk::Image::from_resource(&format!("/org/gtk_rs/rhymr/icons/{dir}/{name}-{dir}.svg"));
    image.set_pixel_size(px);
    image
}

/// The same icon as a `Paintable`, for the file tree (which draws icons
/// into rows rather than adding an `Image` widget).
pub fn paintable(name: &str) -> Option<gtk::gdk::Paintable> {
    img(name, 16).paintable()
}
