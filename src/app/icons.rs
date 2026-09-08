//! One place to build the app's icons — a bundled, recoloured
//! [css.gg](https://css.gg/) set (`assets/icons/gg-*.svg`), so every icon
//! in the UI shares one geometric style, JetBrains-like. Bundled rather
//! than themed because the system icon theme is missing several of the
//! names the toolbar needs.

/// A `GtkImage` for the bundled css.gg icon `name` (`"git-commit"`,
/// `"tree-file"`, …), sized to `px`.
pub fn img(name: &str, px: i32) -> gtk::Image {
    let image = gtk::Image::from_resource(&format!("/org/gtk_rs/rhymr/icons/gg-{name}.svg"));
    image.set_pixel_size(px);
    image
}
