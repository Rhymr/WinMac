//! Classic JetBrains-style window chrome the app didn't originally have: a
//! thin top toolbar of icon buttons, and a left tool-window stripe that
//! toggles the Project / Rhyme Search panels.
//!
//! The toolbar buttons drive existing `app.*` actions by name (registered
//! in [`crate::app::menu`]), so the row is functional rather than
//! decorative. The stripe buttons just flip a panel widget's visibility —
//! `GtkPaned` hands the freed space to the editor on its own.

use gtk::gio;
use gtk::prelude::*;
use gtk::{Align, Box as GtkBox, Button, MenuButton, Orientation, Separator, ToggleButton, Widget};

/// A flat icon button wired to an `app.*` action by name.
fn tool_button(icon: &str, action: &str, tooltip: &str) -> Button {
    Button::builder()
        .icon_name(icon)
        .action_name(action)
        .tooltip_text(tooltip)
        .build()
}

/// The "Configure" dropdown at the right of the toolbar — Settings first,
/// then the Help entries. Replaces the old bare gear icon.
fn configure_button() -> MenuButton {
    let menu = gio::Menu::new();
    menu.append(Some("Settings…"), Some("app.preferences"));

    let help = gio::Menu::new();
    help.append(Some("Documentation"), Some("app.docs"));
    help.append(Some("Report Issue"), Some("app.report-issue"));
    help.append(Some("About"), Some("app.about"));
    menu.append_section(None, &help);

    MenuButton::builder()
        .label("Configure")
        .icon_name("preferences-system-symbolic")
        .always_show_arrow(true)
        .menu_model(&menu)
        .tooltip_text("Configure")
        .build()
}

/// The top toolbar row: file ops, a separator, VCS ops, then (pushed to the
/// right) the Configure dropdown.
pub fn main_toolbar() -> GtkBox {
    let bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(["main-toolbar"])
        .spacing(1)
        .build();

    bar.append(&tool_button("document-new-symbolic", "app.new", "New File"));
    bar.append(&tool_button("document-open-symbolic", "app.open", "Open…"));
    bar.append(&tool_button(
        "document-save-symbolic",
        "app.save-all",
        "Save All",
    ));

    bar.append(&Separator::new(Orientation::Vertical));

    bar.append(&tool_button(
        "object-select-symbolic",
        "app.git-commit",
        "Commit…",
    ));
    bar.append(&tool_button("go-up-symbolic", "app.git-push", "Push…"));
    bar.append(&tool_button("go-down-symbolic", "app.git-pull", "Pull…"));
    bar.append(&tool_button(
        "view-refresh-symbolic",
        "app.git-fetch",
        "Fetch",
    ));

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);

    bar.append(&configure_button());

    bar
}

/// The left tool-window stripe: one toggle per dockable panel. `active`
/// tracks the panel's current visibility; toggling flips it.
pub fn left_stripe(project_panel: Widget, rhyme_panel: Widget) -> GtkBox {
    let stripe = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .css_classes(["tool-stripe"])
        .spacing(1)
        .valign(Align::Fill)
        .build();

    let project_btn = stripe_button("folder-symbolic", "Project", project_panel.is_visible());
    let rhyme_btn = stripe_button(
        "system-search-symbolic",
        "Rhyme Search",
        rhyme_panel.is_visible(),
    );

    project_btn.connect_toggled(move |b| project_panel.set_visible(b.is_active()));
    rhyme_btn.connect_toggled(move |b| rhyme_panel.set_visible(b.is_active()));

    stripe.append(&project_btn);
    stripe.append(&rhyme_btn);
    stripe
}

fn stripe_button(icon: &str, tooltip: &str, active: bool) -> ToggleButton {
    ToggleButton::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .css_classes(["tool-stripe-button"])
        .active(active)
        .build()
}
