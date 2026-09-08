//! Classic JetBrains-style window chrome: a top toolbar (file actions on the
//! left, a colour-coded `Git:` group on the right), a left tool-window
//! stripe with a vertical "Project" label, and a bottom stripe for the
//! Rhyme Search panel — the way a JetBrains IDE arranges its tool windows.

use crate::app::vertical_label::VerticalLabel;
use gtk::gio;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Image, Label, MenuButton, Orientation, Separator, ToggleButton,
};

/// Toolbar icon size, matching JetBrains.
const TOOLBAR_ICON: i32 = 16;

/// A flat icon button wired to an `app.*` action by name.
fn tool_button(icon: &str, action: &str, tooltip: &str) -> Button {
    let image = Image::from_icon_name(icon);
    image.set_pixel_size(TOOLBAR_ICON);
    let button = Button::builder()
        .action_name(action)
        .tooltip_text(tooltip)
        .build();
    button.set_child(Some(&image));
    button
}

/// Same, plus an extra CSS class (used to colour the Git actions).
fn git_button(icon: &str, action: &str, tooltip: &str, class: &str) -> Button {
    let b = tool_button(icon, action, tooltip);
    b.add_css_class(class);
    b
}

/// The "Configure" dropdown — Settings first, then Help.
fn configure_button() -> MenuButton {
    let menu = gio::Menu::new();
    menu.append(Some("Settings\u{2026}"), Some("app.preferences"));

    let help = gio::Menu::new();
    help.append(Some("Documentation"), Some("app.docs"));
    help.append(Some("Report Issue"), Some("app.report-issue"));
    help.append(Some("About"), Some("app.about"));
    menu.append_section(None, &help);

    MenuButton::builder()
        .label("Configure")
        .always_show_arrow(true)
        .menu_model(&menu)
        .tooltip_text("Configure")
        .build()
}

/// The top toolbar row.
pub fn main_toolbar() -> GtkBox {
    let bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(["main-toolbar"])
        .spacing(1)
        .build();

    bar.append(&tool_button("document-new-symbolic", "app.new", "New File"));
    bar.append(&tool_button(
        "document-open-symbolic",
        "app.open",
        "Open\u{2026}",
    ));
    bar.append(&tool_button(
        "document-save-symbolic",
        "app.save-all",
        "Save All",
    ));

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);

    // Git: <pull> <commit> <push> <fetch> — JetBrains colours (blue update,
    // green commit/push).
    bar.append(
        &Label::builder()
            .label("Git:")
            .css_classes(["toolbar-group-label"])
            .build(),
    );
    bar.append(&git_button(
        "go-down-symbolic",
        "app.git-pull",
        "Pull\u{2026}",
        "git-pull",
    ));
    bar.append(&git_button(
        "object-select-symbolic",
        "app.git-commit",
        "Commit\u{2026}",
        "git-commit",
    ));
    bar.append(&git_button(
        "go-up-symbolic",
        "app.git-push",
        "Push\u{2026}",
        "git-push",
    ));
    bar.append(&git_button(
        "view-refresh-symbolic",
        "app.git-fetch",
        "Fetch",
        "git-fetch",
    ));

    bar.append(&Separator::new(Orientation::Vertical));
    bar.append(&configure_button());

    bar
}

/// The left tool-window stripe — a narrow column of vertical-text toggles.
/// Currently just "Project"; `on_toggle(active)` flips the file tree.
pub fn left_stripe<F: Fn(bool) + 'static>(project_visible: bool, on_toggle: F) -> GtkBox {
    let stripe = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .css_classes(["tool-stripe"])
        .spacing(1)
        .valign(Align::Fill)
        .build();

    let content = GtkBox::new(Orientation::Vertical, 3);
    content.set_halign(Align::Center);
    let icon = Image::from_icon_name("folder-symbolic");
    icon.set_pixel_size(13);
    content.append(&icon);
    content.append(&VerticalLabel::new("Project"));

    let btn = ToggleButton::builder()
        .css_classes(["tool-stripe-button"])
        .active(project_visible)
        .tooltip_text("Project")
        .valign(Align::Start)
        .build();
    btn.set_child(Some(&content));
    btn.connect_toggled(move |b| on_toggle(b.is_active()));

    stripe.append(&btn);
    stripe
}

/// The bottom stripe — horizontal toggles for bottom-docked tool windows.
/// Currently just "Rhyme Search"; `on_toggle(active)` shows/hides it.
pub fn bottom_stripe<F: Fn(bool) + 'static>(rhyme_visible: bool, on_toggle: F) -> GtkBox {
    let stripe = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(["bottom-stripe"])
        .spacing(1)
        .build();

    let content = GtkBox::new(Orientation::Horizontal, 4);
    let icon = Image::from_icon_name("system-search-symbolic");
    icon.set_pixel_size(14);
    content.append(&icon);
    content.append(&Label::new(Some("Rhyme Search")));

    let btn = ToggleButton::builder()
        .css_classes(["bottom-stripe-button"])
        .active(rhyme_visible)
        .tooltip_text("Rhyme Search")
        .build();
    btn.set_child(Some(&content));
    btn.connect_toggled(move |b| on_toggle(b.is_active()));

    stripe.append(&btn);
    stripe
}
