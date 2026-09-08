//! Shared right-click context-menu builder — the file tree
//! (file/tree_menu.rs), the editor (editor/mod.rs), and the welcome
//! screen's per-project row menu (app/welcome.rs) all build their popovers
//! through this, so every menu in the app looks and behaves identically
//! rather than each call site hand-rolling its own popover/CSS-class/
//! popdown-on-click boilerplate.
use gtk::gdk;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Image, Label, MenuButton, Orientation, Popover, Separator, Widget,
};
use std::cell::Cell;
use std::rc::Rc;

/// Shortcut hints shown next to a menu item's label — ⌘ on macOS, "Ctrl+"
/// elsewhere, so the menu never shows a Mac-only symbol on Windows/Linux
/// or vice versa. Shared by the file tree's and the editor's context
/// menus (see file/tree_menu.rs and editor/mod.rs).
#[cfg(target_os = "macos")]
pub mod hint {
    pub const CUT: &str = "\u{2318}X";
    pub const COPY: &str = "\u{2318}C";
    pub const COPY_PATH: &str = "\u{21E7}\u{2318}C";
    pub const PASTE: &str = "\u{2318}V";
    pub const DELETE: &str = "\u{232B}";
    pub const SELECT_ALL: &str = "\u{2318}A";
}
#[cfg(not(target_os = "macos"))]
pub mod hint {
    pub const CUT: &str = "Ctrl+X";
    pub const COPY: &str = "Ctrl+C";
    pub const COPY_PATH: &str = "Ctrl+Shift+C";
    pub const PASTE: &str = "Ctrl+V";
    pub const DELETE: &str = "Del";
    pub const SELECT_ALL: &str = "Ctrl+A";
}

#[derive(Clone)]
pub struct ContextMenu {
    popover: Popover,
    menu_box: GtkBox,
    /// The widget the popover is parented to — kept so `popup_below`/
    /// `popup_at` can translate an anchor widget's or a click point's
    /// coordinates into this widget's coordinate space without every call
    /// site having to pass it again.
    parent: Widget,
}

impl ContextMenu {
    /// `parent` is the widget the popover attaches to. Prefer an ancestor
    /// outside whatever CSS subtree has row hover/selection-recoloring
    /// rules (e.g. the file tree's outer `frame` rather than a row's own
    /// box) — those rules use unscoped descendant selectors that would
    /// otherwise bleed into the menu and repaint its items with row
    /// colors instead of `.context-menu-item`'s own.
    pub fn new(parent: &impl IsA<Widget>) -> Self {
        let popover = Popover::new();
        popover.set_parent(parent);
        popover.set_has_arrow(false);

        let menu_box = GtkBox::new(Orientation::Vertical, 0);
        menu_box.set_css_classes(&["context-menu"]);
        popover.set_child(Some(&menu_box));

        // Popovers don't destroy themselves on close; parented-but-hidden
        // ones would otherwise accumulate as the menu is reopened.
        let popover_for_close = popover.clone();
        popover.connect_closed(move |_| {
            popover_for_close.unparent();
        });

        Self {
            popover,
            menu_box,
            parent: parent.clone().upcast(),
        }
    }

    /// A left-aligned, full-width menu row with an optional leading `icon`
    /// (a bundled `crate::app::icons` name), an optional right-aligned
    /// shortcut hint and an optional extra CSS class (e.g. for a
    /// destructive action). Rows without an icon keep a blank slot so every
    /// label in the menu stays aligned. The menu pops down before
    /// `on_click` runs. Returns the row's `Button` so a caller can
    /// `set_sensitive(false)` it (e.g. "Cut" with nothing selected) — most
    /// callers just ignore it.
    pub fn add_item(
        &self,
        icon: Option<&str>,
        text: &str,
        shortcut: Option<&str>,
        extra_class: Option<&str>,
        on_click: impl Fn() + 'static,
    ) -> Button {
        let button = item_button(icon, text, shortcut, extra_class);
        self.menu_box.append(&button);

        let popover = self.popover.clone();
        button.connect_clicked(move |_| {
            popover.popdown();
            on_click();
        });
        button
    }

    /// Same as `add_item`, but with a trailing "opens a submenu" arrow.
    /// Unlike `add_item`, `on_click` is responsible for popping this menu
    /// down itself (typically right before opening the submenu) — the two
    /// menus can't both be open at once, but the submenu needs this one
    /// gone first, not after.
    pub fn add_submenu_item(&self, icon: Option<&str>, text: &str, on_click: impl Fn() + 'static) {
        let button = submenu_button(icon, text);
        self.menu_box.append(&button);
        button.connect_clicked(move |_| on_click());
    }

    pub fn add_separator(&self) {
        self.menu_box
            .append(&Separator::new(Orientation::Horizontal));
    }

    /// Build a dropdown button (toolbar gear, sidebar "Configure"/"Help",
    /// …) whose popover *is* this same styled menu, so every dropdown and
    /// context menu in the app is visually identical. Populate the returned
    /// `ContextMenu` with `add_item` / `add_separator` exactly as for a
    /// right-click menu.
    pub fn dropdown(icon: Option<&str>, label: Option<&str>) -> (MenuButton, Self) {
        let popover = Popover::new();
        popover.set_has_arrow(false);

        let menu_box = GtkBox::new(Orientation::Vertical, 0);
        menu_box.set_css_classes(&["context-menu"]);
        popover.set_child(Some(&menu_box));

        let button = MenuButton::builder().valign(Align::Center).build();
        if let Some(icon) = icon {
            button.set_icon_name(icon);
        }
        if let Some(label) = label {
            button.set_label(label);
        }
        // The MenuButton owns the popover's lifetime — no self-unparenting.
        button.set_popover(Some(&popover));

        let menu = Self {
            popover,
            menu_box,
            parent: button.clone().upcast(),
        };
        (button, menu)
    }

    /// A simple value picker built on the same styled `.context-menu`
    /// popover as every other dropdown (rather than a `gtk::DropDown`):
    /// `options` become menu items, choosing one relabels the button,
    /// stores its index in the returned cell and calls `on_change`.
    pub fn select_dropdown(
        options: &[&str],
        initial: usize,
        on_change: impl Fn(usize) + 'static,
    ) -> (MenuButton, Rc<Cell<usize>>) {
        let (button, menu) = Self::dropdown(None, options.get(initial).copied());
        button.set_always_show_arrow(true);

        let selected = Rc::new(Cell::new(initial));
        let on_change = Rc::new(on_change);
        for (index, option) in options.iter().enumerate() {
            let button = button.clone();
            let selected = selected.clone();
            let on_change = on_change.clone();
            let label = (*option).to_string();
            menu.add_item(None, option, None, None, move || {
                button.set_label(&label);
                if selected.replace(index) != index {
                    on_change(index);
                }
            });
        }

        (button, selected)
    }

    pub fn popdown(&self) {
        self.popover.popdown();
    }

    /// Anchor to the bottom edge of `anchor`, spanning its full width, so
    /// the menu always drops down from directly under whatever was
    /// right-clicked, regardless of exactly where inside it the click
    /// landed. `anchor`'s coordinates are relative to its own parent, so
    /// they're translated into the popover's parent's coordinate space
    /// before building the anchor rectangle.
    pub fn popup_below(&self, anchor: &impl IsA<Widget>) {
        let width = anchor.width().max(1);
        let height = anchor.height().max(1);
        let origin = self.translate(anchor, 0.0, 0.0);
        self.popover.set_pointing_to(Some(&gdk::Rectangle::new(
            origin.0 as i32,
            origin.1 as i32 + height,
            width,
            1,
        )));
        self.popover.set_position(gtk::PositionType::Bottom);
        self.popover.popup();
    }

    /// Anchor at `(x, y)` — coordinates relative to `widget` — rather than
    /// below a specific row. Used by the editor's context menu, which
    /// anchors at the exact point a right-click landed rather than at a
    /// single row.
    pub fn popup_at(&self, widget: &impl IsA<Widget>, x: f64, y: f64) {
        let (x, y) = self.translate(widget, x, y);
        self.popover
            .set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        self.popover.popup();
    }

    /// `(x, y)`, relative to `widget`, translated into the popover's
    /// parent's coordinate space.
    fn translate(&self, widget: &impl IsA<Widget>, x: f64, y: f64) -> (f64, f64) {
        widget
            .compute_point(&self.parent, &gtk::graphene::Point::new(x as f32, y as f32))
            .map(|p| (p.x() as f64, p.y() as f64))
            .unwrap_or((x, y))
    }
}

/// The leading icon column — an `Image` (empty when `icon` is `None`) with
/// class `menu-item-icon`, whose fixed CSS width keeps every label in the
/// menu aligned whether or not the row has a glyph.
fn icon_slot(icon: Option<&str>) -> Image {
    let image = match icon {
        Some(name) => crate::app::icons::img(name, 14),
        None => Image::new(),
    };
    image.add_css_class("menu-item-icon");
    image
}

fn item_button(
    icon: Option<&str>,
    text: &str,
    shortcut: Option<&str>,
    extra_class: Option<&str>,
) -> Button {
    let hbox = GtkBox::new(Orientation::Horizontal, 0);

    hbox.append(&icon_slot(icon));

    let label = Label::new(Some(text));
    label.set_halign(Align::Start);
    label.set_hexpand(true);
    hbox.append(&label);

    if let Some(shortcut) = shortcut {
        let hint = Label::new(Some(shortcut));
        hint.set_halign(Align::End);
        hint.set_css_classes(&["menu-shortcut"]);
        hbox.append(&hint);
    }

    let button = Button::new();
    button.set_child(Some(&hbox));
    button.set_halign(Align::Fill);

    let mut classes = vec!["flat", "context-menu-item"];
    if let Some(extra) = extra_class {
        classes.push(extra);
    }
    button.set_css_classes(&classes);
    button
}

fn submenu_button(icon: Option<&str>, text: &str) -> Button {
    let hbox = GtkBox::new(Orientation::Horizontal, 0);

    let label = Label::new(Some(text));
    label.set_halign(Align::Start);
    label.set_hexpand(true);

    let arrow = Label::new(Some("\u{203A}"));
    arrow.set_halign(Align::End);
    arrow.set_css_classes(&["dim-label"]);

    hbox.append(&icon_slot(icon));
    hbox.append(&label);
    hbox.append(&arrow);

    let button = Button::new();
    button.set_child(Some(&hbox));
    button.set_halign(Align::Fill);
    button.set_css_classes(&["flat", "context-menu-item"]);
    button
}
