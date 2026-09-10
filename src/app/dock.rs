//! `DockArea` — the JetBrains-style tool-window manager. It owns the nested
//! `Paned` tree around the editor, one stripe per edge (Left / Right /
//! Bottom), a per-edge header bar, and a registry of [`ToolWindow`]s. It is
//! responsible for showing / hiding a window, remembering each edge's size
//! and open/closed state (persisted to `config::dock_layout_file`), and
//! restoring that on launch.
//!
//! P1 scope: one window visible per edge, chosen from that edge's stripe;
//! no drag-to-move between edges yet (a follow-up), so a window always sits
//! on its `default_anchor`.

pub mod state;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk::{Align, Box as GtkBox, Button, Label, Orientation, Paned, ToggleButton};

use crate::app::icons::img;
use crate::app::tool_window::{Anchor, ToolWindow};
use crate::app::vertical_label::VerticalLabel;
use state::{DockLayout, WindowState};

/// Min / max px an edge can be sized to (also clamps restored values).
const MIN_SIZE: i32 = 120;
const MAX_SIZE: i32 = 1200;
/// Debounce before a splitter drag is written to disk.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(400);

/// One registered tool window plus its live state.
struct Slot {
    tw: ToolWindow,
    size: Cell<i32>,
    open: Cell<bool>,
    button: ToggleButton,
}

/// The widgets making up one dock edge.
struct Edge {
    stripe: GtkBox,
    header_title: Label,
    hide_button: Button,
    /// Where the visible window's content widget is reparented.
    content_slot: GtkBox,
    /// `header + content_slot`; the collapsible child of `paned`.
    panel: GtkBox,
    /// The `Paned` whose collapsible child is `panel`.
    paned: Paned,
    /// `true` when `panel` is `paned`'s end child (Right / Bottom), so the
    /// stored size maps to `total - position`.
    panel_is_end: bool,
}

/// The tool-window manager. Cheap to clone — all state is `Rc`-backed.
#[derive(Clone)]
pub struct DockArea {
    root: GtkBox,
    slots: Rc<RefCell<HashMap<&'static str, Rc<Slot>>>>,
    edges: Rc<HashMap<Anchor, Edge>>,
    /// Guards radio re-entrancy from `ToggleButton::set_active`.
    updating: Rc<Cell<bool>>,
    /// Bumped on every splitter move so only the last one writes to disk.
    save_gen: Rc<Cell<u64>>,
}

impl DockArea {
    /// Build the dock around `center` (the editor area). Register windows
    /// with [`DockArea::register`], then call [`DockArea::restore`].
    pub fn new(center: &impl IsA<gtk::Widget>) -> Self {
        let left_stripe = stripe(Orientation::Vertical);
        let right_stripe = stripe(Orientation::Vertical);
        let bottom_stripe = stripe(Orientation::Horizontal);
        bottom_stripe.remove_css_class("tool-stripe");
        bottom_stripe.add_css_class("bottom-stripe");

        let (left_panel, left_title, left_hide, left_slot) = dock_panel();
        let (right_panel, right_title, right_hide, right_slot) = dock_panel();
        let (bottom_panel, bottom_title, bottom_hide, bottom_slot) = dock_panel();
        left_panel.set_visible(false);
        right_panel.set_visible(false);
        bottom_panel.set_visible(false);

        let center_right = split(Orientation::Horizontal, center, &right_panel);
        let center_bottom = split(Orientation::Vertical, &center_right, &bottom_panel);
        center_bottom.set_vexpand(true);
        // left | (everything else): the collapsible child is the *start*.
        let left_main = Paned::new(Orientation::Horizontal);
        left_main.set_start_child(Some(&left_panel));
        left_main.set_end_child(Some(&center_bottom));
        left_main.set_resize_start_child(false);
        left_main.set_resize_end_child(true);
        left_main.set_shrink_start_child(false);
        left_main.set_shrink_end_child(false);
        left_main.set_hexpand(true);

        let body = GtkBox::new(Orientation::Horizontal, 0);
        body.set_vexpand(true);
        body.append(&left_stripe);
        body.append(&left_main);
        body.append(&right_stripe);

        let root = GtkBox::new(Orientation::Vertical, 0);
        root.append(&body);
        root.append(&bottom_stripe);

        let mut edges = HashMap::new();
        edges.insert(
            Anchor::Left,
            Edge {
                stripe: left_stripe,
                header_title: left_title,
                hide_button: left_hide,
                content_slot: left_slot,
                panel: left_panel,
                paned: left_main,
                panel_is_end: false,
            },
        );
        edges.insert(
            Anchor::Right,
            Edge {
                stripe: right_stripe,
                header_title: right_title,
                hide_button: right_hide,
                content_slot: right_slot,
                panel: right_panel,
                paned: center_right,
                panel_is_end: true,
            },
        );
        edges.insert(
            Anchor::Bottom,
            Edge {
                stripe: bottom_stripe,
                header_title: bottom_title,
                hide_button: bottom_hide,
                content_slot: bottom_slot,
                panel: bottom_panel,
                paned: center_bottom,
                panel_is_end: true,
            },
        );

        let dock = Self {
            root,
            slots: Rc::new(RefCell::new(HashMap::new())),
            edges: Rc::new(edges),
            updating: Rc::new(Cell::new(false)),
            save_gen: Rc::new(Cell::new(0)),
        };

        for (&anchor, edge) in dock.edges.iter() {
            {
                let dock = dock.clone();
                edge.paned
                    .connect_position_notify(move |_| dock.on_splitter_moved(anchor));
            }
            {
                let dock = dock.clone();
                edge.hide_button
                    .connect_clicked(move |_| dock.close_edge(anchor));
            }
        }

        dock
    }

    /// The widget to place in the window.
    pub fn widget(&self) -> &GtkBox {
        &self.root
    }

    /// Register a tool window: add its stripe button and record it. Call
    /// [`DockArea::restore`] once every window is registered.
    pub fn register(&self, tw: ToolWindow) {
        let anchor = tw.default_anchor;
        let Some(edge) = self.edges.get(&anchor) else {
            return;
        };

        let button = ToggleButton::builder()
            .css_classes([stripe_button_class(anchor)])
            .tooltip_text(tw.title)
            .build();
        button.set_child(Some(&stripe_button_child(anchor, tw.title, tw.icon)));

        let id = tw.id;
        {
            let dock = self.clone();
            button.connect_toggled(move |b| {
                if dock.updating.get() {
                    return;
                }
                dock.set_open(id, b.is_active());
            });
        }
        edge.stripe.append(&button);

        let slot = Rc::new(Slot {
            size: Cell::new(tw.default_size.clamp(MIN_SIZE, MAX_SIZE)),
            open: Cell::new(false),
            button,
            tw,
        });
        self.slots.borrow_mut().insert(id, slot);
    }

    /// Whether the named window is currently open.
    pub fn is_open(&self, id: &str) -> bool {
        self.slots
            .borrow()
            .get(id)
            .map(|s| s.open.get())
            .unwrap_or(false)
    }

    /// Toggle a window open/closed (the `app.*` action and stripe button
    /// both route here).
    pub fn toggle(&self, id: &str) {
        let is_open = self
            .slots
            .borrow()
            .get(id)
            .map(|s| s.open.get())
            .unwrap_or(false);
        self.set_open(id, !is_open);
    }

    /// Load the saved layout and apply it (sizes + which windows are open),
    /// falling back to each window's own defaults.
    pub fn restore(&self) {
        let layout = DockLayout::load();
        let ids: Vec<&'static str> = self.slots.borrow().keys().copied().collect();
        for id in ids {
            let open = {
                let slots = self.slots.borrow();
                let Some(slot) = slots.get(id) else { continue };
                let (size, open) = match layout.windows.get(id) {
                    Some(w) => (w.size.clamp(MIN_SIZE, MAX_SIZE), w.open),
                    None => (
                        slot.tw.default_size.clamp(MIN_SIZE, MAX_SIZE),
                        slot.tw.default_open,
                    ),
                };
                slot.size.set(size);
                open
            };
            if open {
                self.set_open(id, true);
            }
        }
    }

    /// Close everything, reset every window to its default size, reopen the
    /// defaults — "Window → Restore Default Layout".
    pub fn restore_default_layout(&self) {
        let ids: Vec<&'static str> = self.slots.borrow().keys().copied().collect();
        for id in &ids {
            self.set_open(id, false);
        }
        for id in &ids {
            let default_open = {
                let slots = self.slots.borrow();
                let Some(slot) = slots.get(id) else { continue };
                slot.size
                    .set(slot.tw.default_size.clamp(MIN_SIZE, MAX_SIZE));
                slot.tw.default_open
            };
            if default_open {
                self.set_open(id, true);
            }
        }
        self.persist();
    }

    // --- internals -------------------------------------------------------

    fn close_edge(&self, anchor: Anchor) {
        let open_id = self
            .slots
            .borrow()
            .values()
            .find(|s| s.tw.default_anchor == anchor && s.open.get())
            .map(|s| s.tw.id);
        if let Some(id) = open_id {
            self.set_open(id, false);
        }
    }

    fn set_open(&self, id: &str, open: bool) {
        let Some(slot) = self.slots.borrow().get(id).cloned() else {
            return;
        };
        let anchor = slot.tw.default_anchor;
        let Some(edge) = self.edges.get(&anchor) else {
            return;
        };

        self.updating.set(true);
        slot.button.set_active(open);

        if open {
            // One visible per edge: close whatever else is open here.
            let siblings: Vec<Rc<Slot>> = self
                .slots
                .borrow()
                .values()
                .filter(|s| s.tw.default_anchor == anchor && s.tw.id != slot.tw.id && s.open.get())
                .cloned()
                .collect();
            for sib in siblings {
                sib.open.set(false);
                sib.button.set_active(false);
            }

            reparent(&slot.tw.content, &edge.content_slot);
            edge.header_title.set_text(slot.tw.title);
            edge.panel.set_visible(true);
            self.apply_size(edge, slot.size.get());
            slot.open.set(true);
        } else {
            if slot.open.get() {
                slot.size
                    .set(self.read_size(edge).clamp(MIN_SIZE, MAX_SIZE));
            }
            edge.panel.set_visible(false);
            if edge.content_slot.first_child().as_ref() == Some(&slot.tw.content) {
                slot.tw.content.unparent();
            }
            slot.open.set(false);
        }

        self.updating.set(false);
        self.persist();
    }

    fn apply_size(&self, edge: &Edge, size: i32) {
        let paned = edge.paned.clone();
        let panel_is_end = edge.panel_is_end;
        let set = move || {
            let total = if paned.orientation() == Orientation::Horizontal {
                paned.width()
            } else {
                paned.height()
            };
            if panel_is_end {
                if total > size {
                    paned.set_position(total - size);
                }
            } else {
                paned.set_position(size);
            }
        };
        set();
        // Again next tick, in case the paned isn't sized yet at first show.
        glib::idle_add_local_once(set);
    }

    fn read_size(&self, edge: &Edge) -> i32 {
        let total = if edge.paned.orientation() == Orientation::Horizontal {
            edge.paned.width()
        } else {
            edge.paned.height()
        };
        let pos = edge.paned.position();
        if edge.panel_is_end {
            (total - pos).max(MIN_SIZE)
        } else {
            pos.max(MIN_SIZE)
        }
    }

    fn on_splitter_moved(&self, anchor: Anchor) {
        let Some(edge) = self.edges.get(&anchor) else {
            return;
        };
        if !edge.panel.get_visible() {
            return;
        }
        let open_slot = self
            .slots
            .borrow()
            .values()
            .find(|s| s.tw.default_anchor == anchor && s.open.get())
            .cloned();
        let Some(slot) = open_slot else { return };
        slot.size
            .set(self.read_size(edge).clamp(MIN_SIZE, MAX_SIZE));

        let this = self.save_gen.get().wrapping_add(1);
        self.save_gen.set(this);
        let dock = self.clone();
        glib::timeout_add_local_once(SAVE_DEBOUNCE, move || {
            if dock.save_gen.get() == this {
                dock.persist();
            }
        });
    }

    fn persist(&self) {
        let mut layout = DockLayout::default();
        for slot in self.slots.borrow().values() {
            layout.windows.insert(
                slot.tw.id.to_string(),
                WindowState {
                    anchor: slot.tw.default_anchor.as_str().to_string(),
                    size: slot.size.get(),
                    open: slot.open.get(),
                },
            );
        }
        layout.save();
    }
}

/// A `Paned` with `start | end`, start resizable, neither shrinkable.
fn split(
    orientation: Orientation,
    start: &impl IsA<gtk::Widget>,
    end: &impl IsA<gtk::Widget>,
) -> Paned {
    let p = Paned::new(orientation);
    p.set_start_child(Some(start));
    p.set_end_child(Some(end));
    p.set_resize_start_child(true);
    p.set_resize_end_child(false);
    p.set_shrink_start_child(false);
    p.set_shrink_end_child(false);
    p
}

/// Make `child` the sole child of `parent`, detaching `child` from any
/// current parent and evicting whatever `parent` held before (its previous
/// occupant stays alive via the owning `Slot`).
fn reparent(child: &gtk::Widget, parent: &GtkBox) {
    if child.parent().as_ref() == Some(parent.upcast_ref::<gtk::Widget>()) {
        return;
    }
    while let Some(existing) = parent.first_child() {
        existing.unparent();
    }
    if child.parent().is_some() {
        child.unparent();
    }
    parent.append(child);
}

fn stripe(orientation: Orientation) -> GtkBox {
    let s = GtkBox::builder()
        .orientation(orientation)
        .css_classes(["tool-stripe"])
        .spacing(1)
        .build();
    if orientation == Orientation::Vertical {
        s.set_valign(Align::Fill);
    }
    s
}

fn stripe_button_class(anchor: Anchor) -> &'static str {
    match anchor {
        Anchor::Bottom => "bottom-stripe-button",
        _ => "tool-stripe-button",
    }
}

fn stripe_button_child(anchor: Anchor, title: &str, icon: &str) -> GtkBox {
    match anchor {
        Anchor::Bottom => {
            let b = GtkBox::new(Orientation::Horizontal, 4);
            b.append(&img(icon, 16));
            b.append(&Label::new(Some(title)));
            b
        }
        _ => {
            let b = GtkBox::new(Orientation::Vertical, 4);
            b.set_halign(Align::Center);
            b.append(&VerticalLabel::new(title));
            b.append(&img(icon, 12));
            b
        }
    }
}

/// Build one edge's collapsible panel: `header (title + hide) over content`.
/// Returns `(panel, title_label, hide_button, content_slot)`.
fn dock_panel() -> (GtkBox, Label, Button, GtkBox) {
    let panel = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .css_classes(["dock-panel"])
        .build();

    let header = GtkBox::new(Orientation::Horizontal, 4);
    header.add_css_class("tool-window-header");
    let title = Label::new(None);
    title.set_css_classes(&["tool-window-title"]);
    title.set_halign(Align::Start);
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    let hide = Button::builder()
        .css_classes(["flat", "tool-window-hide"])
        .tooltip_text("Hide")
        .label("\u{2715}")
        .build();
    header.append(&title);
    header.append(&spacer);
    header.append(&hide);

    let content_slot = GtkBox::new(Orientation::Vertical, 0);
    content_slot.set_vexpand(true);
    content_slot.set_hexpand(true);

    panel.append(&header);
    panel.append(&content_slot);

    (panel, title, hide, content_slot)
}
