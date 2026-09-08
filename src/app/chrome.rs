//! Classic JetBrains-style window chrome: a compact top toolbar (active-file
//! breadcrumb on the left; file actions + a colour-coded Git group + a
//! settings gear on the right), a left tool-window stripe with a vertical
//! "Project" label, and a bottom stripe for the Rhyme Search panel.

use crate::app::vertical_label::VerticalLabel;
use crate::workspace::controller::WorkspaceController;
use gtk::gio;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Image, Label, MenuButton, Orientation, Separator, ToggleButton,
    pango,
};
use std::rc::Rc;

/// Toolbar icon size, matching JetBrains.
const TOOLBAR_ICON: i32 = 16;

/// A flat icon button wired to an `app.*` action by name.
fn tool_button(icon: &str, action: &str, tooltip: &str) -> Button {
    let image = Image::from_icon_name(icon);
    image.set_pixel_size(TOOLBAR_ICON);
    let button = Button::builder()
        .action_name(action)
        .tooltip_text(tooltip)
        .valign(Align::Center)
        .build();
    button.set_child(Some(&image));
    button
}

/// Same, plus an extra CSS class (used to colour the Git / run actions).
fn tinted_button(icon: &str, action: &str, tooltip: &str, class: &str) -> Button {
    let b = tool_button(icon, action, tooltip);
    b.add_css_class(class);
    b
}

/// The settings gear (JetBrains-style) — Settings first, then Help.
fn settings_button() -> MenuButton {
    let menu = gio::Menu::new();
    menu.append(Some("Settings\u{2026}"), Some("app.preferences"));

    let help = gio::Menu::new();
    help.append(Some("Documentation"), Some("app.docs"));
    help.append(Some("Report Issue"), Some("app.report-issue"));
    help.append(Some("About"), Some("app.about"));
    menu.append_section(None, &help);

    MenuButton::builder()
        .icon_name("emblem-system-symbolic")
        .menu_model(&menu)
        .tooltip_text("Settings")
        .valign(Align::Center)
        .css_classes(["settings-gear"])
        .build()
}

/// The compact top toolbar.
pub fn main_toolbar(controller: &Rc<WorkspaceController>) -> GtkBox {
    let bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(["main-toolbar"])
        .spacing(2)
        .build();

    // Left: breadcrumb showing where the active tab lives.
    let breadcrumb = Label::builder()
        .css_classes(["nav-breadcrumb"])
        .halign(Align::Start)
        .hexpand(true)
        .xalign(0.0)
        .ellipsize(pango::EllipsizeMode::Start)
        .build();
    {
        let breadcrumb = breadcrumb.clone();
        controller.set_nav_listener(move |segments| {
            breadcrumb.set_text(&segments.join("  \u{203a}  "));
        });
    }
    controller.refresh_nav();
    bar.append(&breadcrumb);

    // File actions, sitting where a JetBrains toolbar puts the run controls —
    // tinted like run buttons, but keeping their own action icons.
    bar.append(&tinted_button(
        "document-new-symbolic",
        "app.new",
        "New File",
        "run-action",
    ));
    bar.append(&tinted_button(
        "folder-new-symbolic",
        "app.new-folder",
        "New Folder",
        "run-action",
    ));
    bar.append(&tinted_button(
        "document-save-symbolic",
        "app.save-all",
        "Save All",
        "run-action",
    ));

    bar.append(&Separator::new(Orientation::Vertical));

    // Git: <pull> <commit> <push> <fetch>
    bar.append(
        &Label::builder()
            .label("Git:")
            .css_classes(["toolbar-group-label"])
            .build(),
    );
    bar.append(&tinted_button(
        "go-down-symbolic",
        "app.git-pull",
        "Pull\u{2026}",
        "git-pull",
    ));
    bar.append(&tinted_button(
        "object-select-symbolic",
        "app.git-commit",
        "Commit\u{2026}",
        "git-commit",
    ));
    bar.append(&tinted_button(
        "go-up-symbolic",
        "app.git-push",
        "Push\u{2026}",
        "git-push",
    ));
    bar.append(&tinted_button(
        "view-refresh-symbolic",
        "app.git-fetch",
        "Fetch",
        "git-fetch",
    ));

    bar.append(&Separator::new(Orientation::Vertical));
    bar.append(&settings_button());

    bar
}

/// The left tool-window stripe — a narrow column of vertical-text toggles.
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
        .halign(Align::Center)
        .valign(Align::Start)
        .build();
    btn.set_child(Some(&content));
    btn.connect_toggled(move |b| on_toggle(b.is_active()));

    stripe.append(&btn);
    stripe
}

/// The bottom stripe — horizontal toggles for bottom-docked tool windows.
pub fn bottom_stripe<F: Fn(bool) + 'static>(rhyme_visible: bool, on_toggle: F) -> GtkBox {
    let stripe = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(["bottom-stripe"])
        .spacing(1)
        .build();

    let content = GtkBox::new(Orientation::Horizontal, 4);
    let icon = Image::from_icon_name("system-search-symbolic");
    icon.set_pixel_size(13);
    content.append(&icon);
    content.append(&Label::new(Some("Rhyme Search")));

    let btn = ToggleButton::builder()
        .css_classes(["bottom-stripe-button"])
        .active(rhyme_visible)
        .tooltip_text("Rhyme Search")
        .valign(Align::Center)
        .build();
    btn.set_child(Some(&content));
    btn.connect_toggled(move |b| on_toggle(b.is_active()));

    stripe.append(&btn);
    stripe
}
