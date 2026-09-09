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
    Align, Box as GtkBox, Button, Label, MenuButton, Orientation, Separator, ToggleButton, pango,
};
use std::rc::Rc;

/// Fire an app action from a closure that has no widget handle. Accepts
/// either the bare action name or the `app.`-prefixed form —
/// `Application::activate_action` wants the bare name, unlike
/// `Widget::set_action_name`.
fn activate_app(name: &str) {
    if let Some(app) = gtk::gio::Application::default() {
        app.activate_action(name.strip_prefix("app.").unwrap_or(name), None);
    }
}

/// Toolbar icon size, matching JetBrains.
const TOOLBAR_ICON: i32 = 16;

/// A flat toolbar button showing bundled icon `icon`, wired to an `app.*`
/// action, with `class` (`run-action` / `git-*`) for its tint.
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

    menu.add_item(Some("settings"), "Settings\u{2026}", None, None, || {
        activate_app("app.preferences")
    });
    menu.add_separator();
    menu.add_item(None, "Documentation", None, None, || {
        activate_app("app.docs")
    });
    menu.add_item(None, "Report Issue", None, None, || {
        activate_app("app.report-issue")
    });
    menu.add_item(None, "About", None, None, || activate_app("app.about"));

    button
}

/// The compact top toolbar.
pub fn main_toolbar(controller: &Rc<WorkspaceController>) -> GtkBox {
    let bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(["main-toolbar"])
        .spacing(2)
        .build();

    // Left: breadcrumb showing where the active tab lives — the workspace
    // folder as the first segment, then each directory, then the file. A
    // folder icon per segment, a file icon on the leaf.
    let breadcrumb = GtkBox::builder()
        .css_classes(["nav-breadcrumb"])
        .orientation(Orientation::Horizontal)
        .halign(Align::Start)
        .hexpand(true)
        .spacing(2)
        .build();
    {
        let breadcrumb = breadcrumb.clone();
        controller.set_nav_listener(move |segments| {
            while let Some(child) = breadcrumb.first_child() {
                breadcrumb.remove(&child);
            }
            let last = segments.len().saturating_sub(1);
            // A leaf file only exists once there's more than the root.
            let leaf_is_file = segments.len() > 1;
            for (i, segment) in segments.iter().enumerate() {
                if i > 0 {
                    breadcrumb.append(
                        &Label::builder()
                            .label("\u{203a}")
                            .css_classes(["nav-sep"])
                            .build(),
                    );
                }
                let icon = if i == last && leaf_is_file {
                    img("file", 12)
                } else {
                    img("folder", 12)
                };
                breadcrumb.append(&icon);

                let label = Label::builder().label(segment).build();
                if i == last {
                    label.set_ellipsize(pango::EllipsizeMode::End);
                    label.set_hexpand(true);
                    label.set_xalign(0.0);
                }
                breadcrumb.append(&label);
            }
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

    // Git: <pull> <commit> <push> <fetch>. The whole group hides when the
    // workspace isn't a repo; pull/push/fetch grey out when it has no
    // `origin` remote (commit still works). Driven by the controller's
    // git-availability listener, refreshed on every `set_root_path`.
    let git_separator = Separator::new(Orientation::Vertical);
    bar.append(&git_separator);

    let git_label = Label::builder()
        .label("Git:")
        .css_classes(["toolbar-group-label"])
        .build();
    bar.append(&git_label);

    let git_commit = tool_button(
        "git-commit",
        "app.git-commit",
        "Commit\u{2026}",
        "git-commit",
    );
    let git_pull = tool_button("git-pull", "app.git-pull", "Pull\u{2026}", "git-pull");
    let git_push = tool_button("git-push", "app.git-push", "Push\u{2026}", "git-push");
    let git_fetch = tool_button("git-fetch", "app.git-fetch", "Fetch", "git-fetch");
    bar.append(&git_commit);
    bar.append(&git_pull);
    bar.append(&git_push);
    bar.append(&git_fetch);

    {
        use crate::workspace::controller::GitAvailability;
        let git_separator = git_separator.clone();
        let git_label = git_label.clone();
        let git_commit = git_commit.clone();
        let remote_buttons = [git_pull.clone(), git_push.clone(), git_fetch.clone()];
        controller.set_git_listener(move |availability| {
            let has_repo = availability != GitAvailability::None;
            let has_remote = availability == GitAvailability::Full;
            git_separator.set_visible(has_repo);
            git_label.set_visible(has_repo);
            git_commit.set_visible(has_repo);
            git_commit.set_sensitive(has_repo);
            for button in &remote_buttons {
                button.set_visible(has_repo);
                button.set_sensitive(has_remote);
            }
        });
    }
    controller.refresh_git_availability();

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

    // Label on top (reads bottom-to-top), the icon beneath it — same
    // icon↔text gap as the bottom "Rhyme Search" stripe.
    let content = GtkBox::new(Orientation::Vertical, 4);
    content.set_halign(Align::Center);
    content.append(&VerticalLabel::new("Project"));
    content.append(&img("folder", 12));

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
    content.append(&img("search", 16));
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
