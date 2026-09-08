//! Classic JetBrains-style window chrome: a compact top toolbar (active-file
//! breadcrumb on the left; file actions + a colour-coded Git group + a
//! settings gear on the right), a left tool-window stripe with a vertical
//! "Project" label, and a bottom stripe for the Rhyme Search panel.

use crate::app::context_menu::ContextMenu;
use crate::app::icons::img;
use crate::app::vertical_label::VerticalLabel;
use crate::workspace::controller::WorkspaceController;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Label, MenuButton, Orientation, Separator, ToggleButton,
    pango,
};
use std::rc::Rc;

/// Fire an `app.*` action from a closure that has no widget handle.
fn activate_app(name: &str) {
    if let Some(app) = gtk::gio::Application::default() {
        app.activate_action(name, None);
    }
}

/// Toolbar icon size, matching JetBrains.
const TOOLBAR_ICON: i32 = 16;

/// A flat toolbar button showing bundled css.gg icon `icon`, wired to an
/// `app.*` action, with `class` (`run-action` / `git-*`) for its tint.
fn tool_button(icon: &str, action: &str, tooltip: &str, class: &str) -> Button {
    let button = Button::builder()
        .action_name(action)
        .tooltip_text(tooltip)
        .valign(Align::Center)
        .css_classes([class])
        .build();
    button.set_child(Some(&img(icon, TOOLBAR_ICON)));
    button
}

/// The settings gear (JetBrains-style) — Settings first, then Help. Uses the
/// same styled dropdown as every context menu.
fn settings_button() -> MenuButton {
    let (button, menu) = ContextMenu::dropdown(None, None);
    button.set_child(Some(&img("settings", TOOLBAR_ICON)));
    button.set_tooltip_text(Some("Settings"));
    button.add_css_class("settings-gear");

    menu.add_item("Settings\u{2026}", None, None, || {
        activate_app("app.preferences")
    });
    menu.add_separator();
    menu.add_item("Documentation", None, None, || activate_app("app.docs"));
    menu.add_item("Report Issue", None, None, || {
        activate_app("app.report-issue")
    });
    menu.add_item("About", None, None, || activate_app("app.about"));

    button
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

    // File actions, sitting where a JetBrains toolbar puts the run controls.
    bar.append(&tool_button(
        "new-file",
        "app.new",
        "New File",
        "run-action",
    ));
    bar.append(&tool_button(
        "new-folder",
        "app.new-folder",
        "New Folder",
        "run-action",
    ));
    bar.append(&tool_button(
        "save-all",
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
    bar.append(&tool_button(
        "git-pull",
        "app.git-pull",
        "Pull\u{2026}",
        "git-pull",
    ));
    bar.append(&tool_button(
        "git-commit",
        "app.git-commit",
        "Commit\u{2026}",
        "git-commit",
    ));
    bar.append(&tool_button(
        "git-push",
        "app.git-push",
        "Push\u{2026}",
        "git-push",
    ));
    bar.append(&tool_button(
        "git-fetch",
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
    content.append(&img("tree-folder", 13));
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
    content.append(&img("search", 13));
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
