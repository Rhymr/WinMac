//! `DockArea` — the JetBrains-style tool-window manager. It owns the nested
//! `Paned` tree around the editor, one stripe per edge (Left / Right /
//! Bottom), and a registry of [`ToolWindow`]s. It shows / hides a window,
//! remembers each window's edge, size and open/closed state (persisted to
//! `config::dock_layout_file`) and restores that on launch, and lets a
//! window be dragged from one edge's stripe to another.
//!
//! Each tool window keeps its own header (title + a minimise button the
//! dock injects); the dock draws no header of its own. One window is
//! visible per edge at a time, chosen from that edge's stripe. The bottom
//! edge spans the full width — left tool window, editor and right tool
//! window all sit above it.

pub mod state;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, DragSource, DropTarget, Label, Orientation, Paned, ToggleButton,
};

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
    /// Current edge — starts at `tw.default_anchor`, changed by a drag.
    anchor: Cell<Anchor>,
    size: Cell<i32>,
    open: Cell<bool>,
    button: ToggleButton,
}

/// The widgets making up one dock edge.
struct Edge {
    /// The stripe of toggle buttons (one per window anchored here).
    stripe: GtkBox,
    /// Collapsible child of `paned`; the active window's content is
    /// reparented straight into it.
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

        let left_panel = dock_panel();
        let right_panel = dock_panel();
        let bottom_panel = dock_panel();
        left_panel.set_visible(false);
        right_panel.set_visible(false);
        bottom_panel.set_visible(false);

        // editor | right
        let center_right = Paned::new(Orientation::Horizontal);
        center_right.set_start_child(Some(center));
        center_right.set_end_child(Some(&right_panel));
        center_right.set_resize_start_child(true);
        center_right.set_resize_end_child(false);
        center_right.set_shrink_start_child(false);
        center_right.set_shrink_end_child(false);

        // left | (editor | right)
        let mid_h = Paned::new(Orientation::Horizontal);
        mid_h.set_start_child(Some(&left_panel));
        mid_h.set_end_child(Some(&center_right));
        mid_h.set_resize_start_child(false);
        mid_h.set_resize_end_child(true);
        mid_h.set_shrink_start_child(false);
        mid_h.set_shrink_end_child(false);
        mid_h.set_hexpand(true);

        // (left | editor | right) over bottom — bottom spans the full width
        let main_v = Paned::new(Orientation::Vertical);
        main_v.set_start_child(Some(&mid_h));
        main_v.set_end_child(Some(&bottom_panel));
        main_v.set_resize_start_child(true);
        main_v.set_resize_end_child(false);
        main_v.set_shrink_start_child(false);
        main_v.set_shrink_end_child(false);
        main_v.set_vexpand(true);

        let body = GtkBox::new(Orientation::Horizontal, 0);
        body.set_vexpand(true);
        body.append(&left_stripe);
        body.append(&main_v);
        body.append(&right_stripe);

        let root = GtkBox::new(Orientation::Vertical, 0);
        root.append(&body);
        root.append(&bottom_stripe);

        let mut edges = HashMap::new();
        edges.insert(
            Anchor::Left,
            Edge {
                stripe: left_stripe,
                panel: left_panel,
                paned: mid_h,
                panel_is_end: false,
            },
        );
        edges.insert(
            Anchor::Right,
            Edge {
                stripe: right_stripe,
                panel: right_panel,
                paned: center_right,
                panel_is_end: true,
            },
        );
        edges.insert(
            Anchor::Bottom,
            Edge {
                stripe: bottom_stripe,
                panel: bottom_panel,
                paned: main_v,
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
            // Each stripe is a drop target: dropping a window's id here moves
            // that window to this edge.
            let drop = DropTarget::new(glib::types::Type::STRING, gdk::DragAction::MOVE);
            {
                let stripe = edge.stripe.clone();
                drop.connect_enter(move |_, _, _| {
                    stripe.add_css_class("drop-zone-active");
                    gdk::DragAction::MOVE
                });
            }
            {
                let stripe = edge.stripe.clone();
                drop.connect_leave(move |_| stripe.remove_css_class("drop-zone-active"));
            }
            {
                let dock = dock.clone();
                let stripe = edge.stripe.clone();
                drop.connect_drop(move |_, value, _, _| {
                    stripe.remove_css_class("drop-zone-active");
                    if let Ok(id) = value.get::<String>() {
                        dock.move_window(&id, anchor);
                        true
                    } else {
                        false
                    }
                });
            }
            edge.stripe.add_controller(drop);
        }

        dock
    }

    /// The widget to place in the window.
    pub fn widget(&self) -> &GtkBox {
        &self.root
    }

    /// Register a tool window: add its stripe button, inject a minimise
    /// button into its own header, and record it. Call [`DockArea::restore`]
    /// once every window is registered.
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
        add_drag_source(&button, id);
        edge.stripe.append(&button);

        // Minimise button, in the panel's *own* header (the dock has none).
        let hide = Button::builder()
            .css_classes(["flat", "tool-window-hide"])
            .tooltip_text("Hide")
            .child(&img("hide", 14))
            .build();
        {
            let dock = self.clone();
            hide.connect_clicked(move |_| dock.set_open(id, false));
        }
        tw.header_actions.append(&hide);

        let slot = Rc::new(Slot {
            anchor: Cell::new(anchor),
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
        let is_open = self.is_open(id);
        self.set_open(id, !is_open);
    }

    /// Move a window to another edge — reparents its stripe button, closes
    /// and reopens it on the new edge if it was visible, and persists the
    /// new anchor.
    pub fn move_window(&self, id: &str, new_anchor: Anchor) {
        let Some(slot) = self.slots.borrow().get(id).cloned() else {
            return;
        };
        let old_anchor = slot.anchor.get();
        if old_anchor == new_anchor {
            return;
        }
        if !self.edges.contains_key(&new_anchor) {
            return;
        }

        let was_open = slot.open.get();
        if was_open {
            self.set_open(id, false);
        }

        if let Some(old_edge) = self.edges.get(&old_anchor) {
            old_edge.stripe.remove(&slot.button);
        }
        slot.button
            .set_css_classes(&[stripe_button_class(new_anchor)]);
        slot.button.set_child(Some(&stripe_button_child(
            new_anchor,
            slot.tw.title,
            slot.tw.icon,
        )));
        if let Some(new_edge) = self.edges.get(&new_anchor) {
            new_edge.stripe.append(&slot.button);
        }
        slot.anchor.set(new_anchor);

        if was_open {
            self.set_open(id, true);
        }
        self.persist();
    }

    /// Load the saved layout and apply it (edge + size + open state),
    /// falling back to each window's own defaults.
    pub fn restore(&self) {
        let layout = DockLayout::load();
        let ids: Vec<&'static str> = self.slots.borrow().keys().copied().collect();
        for id in ids {
            let (target_anchor, open) = {
                let slots = self.slots.borrow();
                let Some(slot) = slots.get(id) else { continue };
                match layout.windows.get(id) {
                    Some(w) => {
                        slot.size.set(w.size.clamp(MIN_SIZE, MAX_SIZE));
                        (Anchor::parse(&w.anchor, slot.tw.default_anchor), w.open)
                    }
                    None => {
                        slot.size
                            .set(slot.tw.default_size.clamp(MIN_SIZE, MAX_SIZE));
                        (slot.tw.default_anchor, slot.tw.default_open)
                    }
                }
            };
            self.move_window(id, target_anchor);
            if open {
                self.set_open(id, true);
            }
        }
    }

    /// Close everything, reset every window to its default edge + size,
    /// reopen the defaults — "Window → Restore Default Layout".
    pub fn restore_default_layout(&self) {
        let ids: Vec<&'static str> = self.slots.borrow().keys().copied().collect();
        for id in &ids {
            self.set_open(id, false);
        }
        for id in &ids {
            let (anchor, open) = {
                let slots = self.slots.borrow();
                let Some(slot) = slots.get(id) else { continue };
                slot.size
                    .set(slot.tw.default_size.clamp(MIN_SIZE, MAX_SIZE));
                (slot.tw.default_anchor, slot.tw.default_open)
            };
            self.move_window(id, anchor);
            if open {
                self.set_open(id, true);
            }
        }
        self.persist();
    }

    // --- internals -------------------------------------------------------

    fn set_open(&self, id: &str, open: bool) {
        let Some(slot) = self.slots.borrow().get(id).cloned() else {
            return;
        };
        let anchor = slot.anchor.get();
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
                .filter(|s| s.anchor.get() == anchor && s.tw.id != slot.tw.id && s.open.get())
                .cloned()
                .collect();
            for sib in siblings {
                sib.open.set(false);
                sib.button.set_active(false);
            }

            reparent(&slot.tw.content, &edge.panel);
            edge.panel.set_visible(true);
            self.apply_size(edge, slot.size.get());
            slot.open.set(true);
        } else {
            if slot.open.get() {
                slot.size
                    .set(self.read_size(edge).clamp(MIN_SIZE, MAX_SIZE));
            }
            edge.panel.set_visible(false);
            if edge.panel.first_child().as_ref() == Some(&slot.tw.content) {
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
            .find(|s| s.anchor.get() == anchor && s.open.get())
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
                    anchor: slot.anchor.get().as_str().to_string(),
                    size: slot.size.get(),
                    open: slot.open.get(),
                },
            );
        }
        layout.save();
    }
}

/// Add a `DragSource` carrying `id` to a stripe button, so it can be
/// dragged onto another edge's stripe. Mirrors `file/tree.rs`'s DnD.
fn add_drag_source(button: &ToggleButton, id: &'static str) {
    let source = DragSource::new();
    source.set_actions(gdk::DragAction::MOVE);
    source.connect_prepare(move |_, _, _| Some(gdk::ContentProvider::for_value(&id.to_value())));
    {
        let button = button.clone();
        source.connect_drag_begin(move |_, _| button.add_css_class("dragging"));
    }
    {
        let button = button.clone();
        source.connect_drag_end(move |_, _, _| button.remove_css_class("dragging"));
    }
    button.add_controller(source);
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

/// One edge's collapsible panel — a bare box the active window's content is
/// reparented into (the window brings its own header).
fn dock_panel() -> GtkBox {
    GtkBox::builder()
        .orientation(Orientation::Vertical)
        .css_classes(["dock-panel"])
        .build()
}
