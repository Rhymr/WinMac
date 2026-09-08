//! A borderless splash window shown for a beat before the welcome picker —
//! the classic JetBrains product splash (big product name, version, logo
//! mark over a gradient).

use gtk::prelude::*;
use gtk::{Align, ApplicationWindow, Box as GtkBox, Label, Orientation};
use libadwaita::Application;

/// Build, present, and return the splash window. The caller closes it once
/// the welcome window is up (see `main.rs`).
pub fn show(app: &Application) -> ApplicationWindow {
    let window = ApplicationWindow::builder()
        .application(app)
        .decorated(false)
        .resizable(false)
        .default_width(560)
        .default_height(340)
        .css_classes(["splash"])
        .build();

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .css_classes(["splash-body"])
        .hexpand(true)
        .vexpand(true)
        .build();

    let head = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Start)
        .valign(Align::Start)
        .vexpand(true)
        .build();
    head.append(
        &Label::builder()
            .label("Rhymr")
            .halign(Align::Start)
            .css_classes(["splash-name"])
            .build(),
    );
    head.append(
        &Label::builder()
            .label(crate::version::display())
            .halign(Align::Start)
            .css_classes(["splash-version"])
            .build(),
    );
    body.append(&head);

    let logo = crate::app::icons::img("toolbar-toggle-highlighting", 44);
    logo.set_halign(Align::End);
    logo.set_valign(Align::End);
    logo.add_css_class("splash-logo");
    body.append(&logo);

    window.set_child(Some(&body));
    window.present();
    window
}
