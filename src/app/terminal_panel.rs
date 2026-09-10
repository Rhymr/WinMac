//! The embedded Terminal tool window — a `DrawingArea` rendering an
//! `alacritty_terminal` grid, fed by a [`crate::terminal::TerminalSession`]
//! PTY. Bottom-docked like Rhyme Search / Git Log.
//!
//! P1 scope: one session, rooted at the workspace folder (home dir when no
//! project is open), colour + resize + scrollback, mouse selection with
//! Cmd/Ctrl-C copy and Cmd/Ctrl-V paste. Off-thread PTY IO; the UI polls a
//! dirty flag and repaints.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{TermMode, viewport_to_point};
use gtk::gdk::{Key, ModifierType};
use gtk::glib::Propagation;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, DrawingArea, EventControllerKey, EventControllerScroll,
    EventControllerScrollFlags, Frame, GestureClick, GestureDrag, Label, Orientation, gdk, glib,
};

use crate::setting::{Settings, Theme};
use crate::terminal::TerminalSession;
use crate::terminal::render::{self, Metrics};

/// UI-side look, cached so the draw callback doesn't re-read `Settings`
/// every frame. Refreshed on each panel open.
#[derive(Clone)]
struct Look {
    family: String,
    size_px: f64,
    dark: bool,
}

/// The Terminal panel. Cheap to clone — all state is `Rc`/GObject-backed.
#[derive(Clone)]
pub struct TerminalPanel {
    frame: Frame,
    header_actions: GtkBox,
    title_label: Label,
    area: DrawingArea,
    session: Rc<RefCell<Option<Rc<TerminalSession>>>>,
    cwd: Rc<RefCell<Option<PathBuf>>>,
    metrics: Rc<Cell<Metrics>>,
    look: Rc<RefCell<Look>>,
    collapsed: Rc<Cell<bool>>,
    started: Rc<Cell<bool>>,
}

impl Default for TerminalPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalPanel {
    pub fn new() -> Self {
        let look = look_from_settings();
        let metrics = measure(&look.family, look.size_px);

        let area = DrawingArea::builder()
            .hexpand(true)
            .vexpand(true)
            .focusable(true)
            .can_focus(true)
            .css_classes(["terminal-view"])
            .build();

        let header = GtkBox::new(Orientation::Horizontal, 6);
        header.set_css_classes(&["terminal-header"]);
        let title_label = Label::new(Some("Terminal"));
        title_label.set_css_classes(&["terminal-title"]);
        title_label.set_hexpand(true);
        title_label.set_halign(Align::Start);
        let header_actions = GtkBox::new(Orientation::Horizontal, 2);
        header.append(&title_label);
        header.append(&header_actions);

        let frame = Frame::builder()
            .child(&area)
            .css_classes(["terminal-container"])
            .build();
        frame.set_label_widget(Some(&header));

        let panel = Self {
            frame,
            header_actions,
            title_label,
            area: area.clone(),
            session: Rc::new(RefCell::new(None)),
            cwd: Rc::new(RefCell::new(None)),
            metrics: Rc::new(Cell::new(metrics)),
            look: Rc::new(RefCell::new(look)),
            collapsed: Rc::new(Cell::new(true)),
            started: Rc::new(Cell::new(false)),
        };

        panel.wire_draw();
        panel.wire_input();
        panel.wire_resize();
        panel
    }

    /// Outer widget for the dock.
    pub fn get_widget(&self) -> &Frame {
        &self.frame
    }

    /// Trailing header action area, for the dock's minimise button.
    pub fn header_actions(&self) -> GtkBox {
        self.header_actions.clone()
    }

    /// Whether the panel body is hidden.
    pub fn is_collapsed(&self) -> bool {
        self.collapsed.get()
    }

    /// Show / hide the panel body; starts the shell on first open.
    pub fn set_expanded(&self, expanded: bool) {
        self.collapsed.set(!expanded);
        if let Some(child) = self.frame.child() {
            child.set_visible(expanded);
        }
        if expanded {
            *self.look.borrow_mut() = look_from_settings();
            let l = self.look.borrow().clone();
            self.metrics.set(measure(&l.family, l.size_px));
            self.ensure_started();
            self.area.grab_focus();
        }
    }

    /// Point the terminal at a workspace root (home dir when `None`). Only
    /// affects the *next* session start; a running shell is not restarted.
    pub fn set_cwd(&self, root: Option<PathBuf>) {
        *self.cwd.borrow_mut() = root;
    }

    // --- internals -----------------------------------------------------

    fn ensure_started(&self) {
        if self.started.get() {
            return;
        }
        // The area is usually not allocated yet on first show, so fall back
        // to a standard 80×24; `connect_resize` corrects it once laid out.
        let m = self.metrics.get();
        let (aw, ah) = (self.area.width(), self.area.height());
        let cols = if aw > 1 {
            ((f64::from(aw) / m.cell_w).floor() as usize).max(2)
        } else {
            80
        };
        let rows = if ah > 1 {
            ((f64::from(ah) / m.cell_h).floor() as usize).max(1)
        } else {
            24
        };

        let cwd = self
            .cwd
            .borrow()
            .clone()
            .filter(|p| p.is_dir())
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));

        match TerminalSession::spawn(&cwd, cols, rows) {
            Ok(session) => {
                *self.session.borrow_mut() = Some(session);
                self.started.set(true);
                self.start_poll();
            }
            Err(e) => {
                log::error!("terminal: {e}");
                self.title_label.set_text(&format!("Terminal — {e}"));
            }
        }
        self.area.queue_draw();
    }

    /// 30 Hz poll: repaint when the session flags itself dirty, and reflect
    /// the OSC title / shell exit in the header.
    fn start_poll(&self) {
        let this = self.clone();
        glib::timeout_add_local(Duration::from_millis(33), move || {
            let Some(session) = this.session.borrow().clone() else {
                return glib::ControlFlow::Break;
            };
            if session.take_dirty() {
                this.area.queue_draw();
                let title = session.title();
                let text = if session.has_exited() {
                    "Terminal — process finished".to_string()
                } else if title.is_empty() {
                    "Terminal".to_string()
                } else {
                    format!("Terminal — {title}")
                };
                if this.title_label.text() != text {
                    this.title_label.set_text(&text);
                }
            }
            glib::ControlFlow::Continue
        });
    }

    fn wire_draw(&self) {
        let session = self.session.clone();
        let look = self.look.clone();
        let metrics = self.metrics.clone();
        self.area.set_draw_func(move |_area, ctx, w, h| {
            if w <= 0 || h <= 0 {
                return;
            }
            let l = look.borrow();
            match session.borrow().as_ref() {
                Some(s) => render::draw(
                    s.term(),
                    ctx,
                    &l.family,
                    l.size_px,
                    metrics.get(),
                    l.dark,
                    (w, h),
                ),
                None => {
                    // Not started yet — just the background.
                    let bg = if l.dark { 0.17 } else { 1.0 };
                    ctx.set_source_rgb(bg, bg, bg);
                    ctx.rectangle(0.0, 0.0, f64::from(w), f64::from(h));
                    let _ = ctx.fill();
                }
            }
        });
    }

    fn wire_resize(&self) {
        let session = self.session.clone();
        let metrics = self.metrics.clone();
        self.area.connect_resize(move |_, w, h| {
            let m = metrics.get();
            let cols = ((f64::from(w.max(1)) / m.cell_w).floor() as usize).max(2);
            let rows = ((f64::from(h.max(1)) / m.cell_h).floor() as usize).max(1);
            if let Some(s) = session.borrow().as_ref() {
                s.resize(cols, rows);
            }
        });
    }

    fn wire_input(&self) {
        // Focus on click.
        {
            let area = self.area.clone();
            let click = GestureClick::new();
            click.connect_pressed(move |_, _, _, _| {
                area.grab_focus();
            });
            self.area.add_controller(click);
        }

        // Keyboard → PTY.
        {
            let panel = self.clone();
            let keys = EventControllerKey::new();
            keys.connect_key_pressed(move |_, keyval, _code, state| panel.on_key(keyval, state));
            self.area.add_controller(keys);
        }

        // Scroll wheel → scrollback.
        {
            let session = self.session.clone();
            let area = self.area.clone();
            let scroll = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
            scroll.connect_scroll(move |_, _dx, dy| {
                if let Some(s) = session.borrow().as_ref() {
                    // Wheel up (dy < 0) scrolls towards history.
                    s.scroll((-dy * 3.0) as i32);
                    area.queue_draw();
                }
                Propagation::Stop
            });
            self.area.add_controller(scroll);
        }

        // Mouse drag → text selection.
        {
            let panel = self.clone();
            let drag = GestureDrag::new();
            {
                let panel = panel.clone();
                drag.connect_drag_begin(move |_, x, y| {
                    panel.area.grab_focus();
                    panel.selection_begin(x, y);
                });
            }
            drag.connect_drag_update(move |g, dx, dy| {
                if let Some((sx, sy)) = g.start_point() {
                    panel.selection_update(sx + dx, sy + dy);
                }
            });
            self.area.add_controller(drag);
        }
    }

    fn on_key(&self, keyval: Key, state: ModifierType) -> Propagation {
        let Some(session) = self.session.borrow().clone() else {
            return Propagation::Proceed;
        };
        let ctrl = state.contains(ModifierType::CONTROL_MASK);
        let meta = state.contains(ModifierType::META_MASK);
        let shift = state.contains(ModifierType::SHIFT_MASK);
        let alt = state.contains(ModifierType::ALT_MASK);

        // Copy / paste: Cmd+C/V (macOS) or Ctrl+Shift+C/V.
        if (meta || (ctrl && shift)) && matches!(keyval, Key::c | Key::C) {
            if let Some(text) = session.selection_text() {
                self.area.clipboard().set_text(&text);
            }
            return Propagation::Stop;
        }
        if (meta || (ctrl && shift)) && matches!(keyval, Key::v | Key::V) {
            self.paste(session);
            return Propagation::Stop;
        }

        let app_cursor = session
            .term()
            .lock()
            .map(|t| t.mode().contains(TermMode::APP_CURSOR))
            .unwrap_or(false);

        if let Some(bytes) = encode_key(keyval, ctrl, alt, app_cursor) {
            session.write(&bytes);
            self.area.queue_draw();
            return Propagation::Stop;
        }
        Propagation::Proceed
    }

    fn paste(&self, session: Rc<TerminalSession>) {
        let bracketed = session
            .term()
            .lock()
            .map(|t| t.mode().contains(TermMode::BRACKETED_PASTE))
            .unwrap_or(false);
        self.area
            .clipboard()
            .read_text_async(gdk::gio::Cancellable::NONE, move |res| {
                if let Ok(Some(text)) = res {
                    let text = text.replace('\r', "");
                    let bytes = if bracketed {
                        format!("\x1b[200~{text}\x1b[201~").into_bytes()
                    } else {
                        text.into_bytes()
                    };
                    session.write(&bytes);
                }
            });
    }

    fn selection_begin(&self, x: f64, y: f64) {
        let Some(session) = self.session.borrow().clone() else {
            return;
        };
        let (point, side) = self.point_at(&session, x, y);
        if let Ok(mut term) = session.term().lock() {
            term.selection = Some(Selection::new(SelectionType::Simple, point, side));
        }
        self.area.queue_draw();
    }

    fn selection_update(&self, x: f64, y: f64) {
        let Some(session) = self.session.borrow().clone() else {
            return;
        };
        let (point, side) = self.point_at(&session, x, y);
        if let Ok(mut term) = session.term().lock()
            && let Some(sel) = term.selection.as_mut()
        {
            sel.update(point, side);
        }
        self.area.queue_draw();
    }

    /// Map a widget-space pixel to a grid `Point` + cell side.
    fn point_at(&self, session: &TerminalSession, x: f64, y: f64) -> (Point, Side) {
        let m = self.metrics.get();
        let (cols, rows, display_offset) = session
            .term()
            .lock()
            .map(|t| (t.columns(), t.screen_lines(), t.grid().display_offset()))
            .unwrap_or((80, 24, 0));

        let col = ((x / m.cell_w).max(0.0) as usize).min(cols.saturating_sub(1));
        let vline = ((y / m.cell_h).max(0.0) as usize).min(rows.saturating_sub(1));
        let side = if (x / m.cell_w).fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        (
            viewport_to_point(display_offset, Point::new(vline, Column(col))),
            side,
        )
    }
}

/// Bytes to send for a key press, or `None` to let GTK handle it.
fn encode_key(keyval: Key, ctrl: bool, alt: bool, app_cursor: bool) -> Option<Vec<u8>> {
    let csi = |c: char| format!("\x1b[{c}").into_bytes();
    let ss3 = |c: char| format!("\x1bO{c}").into_bytes();

    let named: Option<Vec<u8>> = match keyval {
        Key::Return | Key::KP_Enter => Some(b"\r".to_vec()),
        Key::BackSpace => Some(vec![0x7f]),
        Key::Tab => Some(b"\t".to_vec()),
        Key::ISO_Left_Tab => Some(b"\x1b[Z".to_vec()),
        Key::Escape => Some(vec![0x1b]),
        Key::Up => Some(if app_cursor { ss3('A') } else { csi('A') }),
        Key::Down => Some(if app_cursor { ss3('B') } else { csi('B') }),
        Key::Right => Some(if app_cursor { ss3('C') } else { csi('C') }),
        Key::Left => Some(if app_cursor { ss3('D') } else { csi('D') }),
        Key::Home => Some(b"\x1b[H".to_vec()),
        Key::End => Some(b"\x1b[F".to_vec()),
        Key::Page_Up => Some(b"\x1b[5~".to_vec()),
        Key::Page_Down => Some(b"\x1b[6~".to_vec()),
        Key::Delete => Some(b"\x1b[3~".to_vec()),
        Key::Insert => Some(b"\x1b[2~".to_vec()),
        _ => None,
    };
    if named.is_some() {
        return named;
    }

    let ch = keyval.to_unicode()?;
    let mut out = Vec::with_capacity(4);
    if alt {
        out.push(0x1b);
    }
    if ctrl {
        // Ctrl-A..Ctrl-Z and a few adjacent control codes.
        let b = ch as u32;
        let code = match ch {
            'a'..='z' => (b - b'a' as u32 + 1) as u8,
            'A'..='Z' => (b - b'A' as u32 + 1) as u8,
            ' ' | '@' => 0,
            '[' => 0x1b,
            '\\' => 0x1c,
            ']' => 0x1d,
            '^' => 0x1e,
            '_' => 0x1f,
            _ => {
                out.extend_from_slice(ch.encode_utf8(&mut [0u8; 4]).as_bytes());
                return Some(out);
            }
        };
        out.push(code);
        return Some(out);
    }
    let mut buf = [0u8; 4];
    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    Some(out)
}

/// Editor monospace family + a px size (the setting is stored in points).
fn look_from_settings() -> Look {
    let s = Settings::load();
    Look {
        family: s.font_family.clone(),
        size_px: f64::from(s.font_size) * 96.0 / 72.0,
        dark: s.theme == Theme::Dark,
    }
}

/// Measure cell geometry on a throwaway surface (no widget needed).
fn measure(family: &str, size_px: f64) -> Metrics {
    let fallback = Metrics {
        cell_w: (size_px * 0.6).ceil().max(1.0),
        cell_h: (size_px * 1.3).ceil().max(1.0),
        baseline: size_px,
    };
    let Ok(surface) = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, 8, 8) else {
        return fallback;
    };
    match gtk::cairo::Context::new(&surface) {
        Ok(ctx) => render::metrics(&ctx, family, size_px),
        Err(_) => fallback,
    }
}
