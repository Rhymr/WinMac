//! The bottom half of the file panel: one collapsible, read-only tree per
//! registered [`TextSource`] (Apple Notes today). Workspace-independent —
//! the same widget lives for the window's lifetime and is only re-pointed
//! at a new workspace's `.rhymr/` cache when the project changes.
//!
//! All source I/O (`osascript`, later HTTP) runs on worker threads; results
//! come back to the UI over `mpsc` channels drained by a short poll timer.

use crate::app::context_menu::ContextMenu;
use crate::app::icons;
use crate::source::{DocId, SourceFolder, SourceRegistry, SourceStatus, SourceTree, TextSource};
use gtk::prelude::*;
use gtk::{Align, Box as GtkBox, Frame, GestureClick, Label, ListBox, Orientation};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

const INDENT_PX: i32 = 16;
const POLL: Duration = Duration::from_millis(400);

type OpenFn = dyn Fn(String, String);

/// A read-only "External Sources" panel — see the module docs.
#[derive(Clone)]
pub struct SourcePanel {
    frame: Frame,
    list: ListBox,
    registry: Rc<SourceRegistry>,
    /// One tree per `registry.sources()`, index-aligned.
    trees: Rc<RefCell<Vec<SourceTree>>>,
    /// Collapsed keys: `""` + source id for a section, `srcid\x1ffolder`
    /// for a folder within it.
    collapsed: Rc<RefCell<HashSet<String>>>,
    /// Called with `(title, body)` when the user opens a document.
    on_open: Rc<RefCell<Option<Rc<OpenFn>>>>,
    /// `tx` for "please reload source #i" — the poll loop owns the `rx`.
    reload_tx: Rc<RefCell<Option<mpsc::Sender<usize>>>>,
}

impl Default for SourcePanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SourcePanel {
    pub fn new() -> Self {
        let list = ListBox::builder()
            .css_classes(["file-list", "source-list"])
            .selection_mode(gtk::SelectionMode::None)
            .build();

        // No own scroller — the left panel's single outer scroller (see
        // `app::layout`) scrolls this together with the project tree.
        let frame = Frame::builder()
            .css_classes(vec!["source-panel"])
            .child(&list)
            .build();

        let registry = Rc::new(SourceRegistry::with_defaults());
        let trees = Rc::new(RefCell::new(vec![
            SourceTree::default();
            registry.sources().len()
        ]));

        Self {
            frame,
            list,
            registry,
            trees,
            collapsed: Rc::new(RefCell::new(HashSet::new())),
            on_open: Rc::new(RefCell::new(None)),
            reload_tx: Rc::new(RefCell::new(None)),
        }
    }

    pub fn get_widget(&self) -> &Frame {
        &self.frame
    }

    /// Register the "open a document" handler — wired to
    /// `Workspace::open_readonly` by the layout.
    pub fn connect_open(&self, f: impl Fn(String, String) + 'static) {
        *self.on_open.borrow_mut() = Some(Rc::new(f));
    }

    /// Start the background refresh loop (timers + poll). Call once. The
    /// workspace root arrives later via [`set_workspace_root`].
    pub fn start(&self) {
        self.repaint_from_cache();

        // Channels: `load_tx` carries fresh `(idx, SourceTree)` back to the
        // UI; `reload_tx` lets menu/refresh actions ask for a reload.
        let (load_tx, load_rx) = mpsc::channel::<(usize, SourceTree)>();
        let (reload_tx, reload_rx) = mpsc::channel::<usize>();
        *self.reload_tx.borrow_mut() = Some(reload_tx);

        let spawn_load = {
            let sources: Vec<Arc<dyn TextSource>> = self.registry.sources().to_vec();
            let load_tx = load_tx.clone();
            move |idx: usize| {
                let Some(source) = sources.get(idx).cloned() else {
                    return;
                };
                if matches!(source.status(), SourceStatus::Unavailable(_)) {
                    return;
                }
                let load_tx = load_tx.clone();
                std::thread::spawn(move || {
                    if let Ok(tree) = source.load() {
                        let _ = load_tx.send((idx, tree));
                    }
                });
            }
        };

        // Kick one load per source now, then on a fixed interval.
        for i in 0..self.registry.sources().len() {
            spawn_load(i);
        }
        {
            let spawn_load = spawn_load.clone();
            let n = self.registry.sources().len();
            glib::timeout_add_local(crate::config::APPLE_NOTES_REFRESH, move || {
                for i in 0..n {
                    spawn_load(i);
                }
                glib::ControlFlow::Continue
            });
        }

        // Drain both channels onto the UI.
        let panel = self.clone();
        glib::timeout_add_local(POLL, move || {
            while let Ok(idx) = reload_rx.try_recv() {
                spawn_load(idx);
            }
            let mut changed = false;
            while let Ok((idx, tree)) = load_rx.try_recv() {
                if let Some(slot) = panel.trees.borrow_mut().get_mut(idx)
                    && *slot != tree
                {
                    *slot = tree;
                    changed = true;
                }
            }
            if changed {
                panel.rebuild();
            }
            glib::ControlFlow::Continue
        });
    }

    /// Re-point every source at `root`'s `.rhymr/` cache, repaint from that
    /// cache, and ask for a fresh load. Called when the project changes.
    pub fn set_workspace_root(&self, root: Option<PathBuf>) {
        for source in self.registry.sources() {
            source.set_workspace(root.as_deref());
        }
        self.repaint_from_cache();
        for i in 0..self.registry.sources().len() {
            self.request_reload(i);
        }
    }

    fn repaint_from_cache(&self) {
        {
            let mut trees = self.trees.borrow_mut();
            for (i, source) in self.registry.sources().iter().enumerate() {
                if matches!(source.status(), SourceStatus::Unavailable(_)) {
                    continue;
                }
                trees[i] = source.cached();
            }
        }
        self.rebuild();
    }

    fn request_reload(&self, idx: usize) {
        if let Some(tx) = self.reload_tx.borrow().as_ref() {
            let _ = tx.send(idx);
        }
    }

    /// Open document `id` from source `idx` — its text is fetched on a
    /// worker thread, then handed to the open handler.
    fn open_doc(&self, idx: usize, id: DocId, title: String) {
        let Some(on_open) = self.on_open.borrow().clone() else {
            return;
        };
        let Some(source) = self.registry.sources().get(idx).cloned() else {
            return;
        };
        let (tx, rx) = mpsc::channel::<Option<String>>();
        std::thread::spawn(move || {
            let _ = tx.send(source.document_text(&id).ok());
        });
        glib::timeout_add_local(POLL, move || match rx.try_recv() {
            Ok(Some(body)) => {
                on_open(title.clone(), body);
                glib::ControlFlow::Break
            }
            Ok(None) | Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        });
    }

    fn is_collapsed(&self, key: &str) -> bool {
        self.collapsed.borrow().contains(key)
    }

    fn toggle_collapsed(&self, key: String) {
        let mut c = self.collapsed.borrow_mut();
        if !c.remove(&key) {
            c.insert(key);
        }
        drop(c);
        self.rebuild();
    }

    /// Every folder key under source `idx` (its section stays expanded).
    fn folder_keys(&self, idx: usize) -> Vec<String> {
        fn walk(prefix: &str, folders: &[SourceFolder], out: &mut Vec<String>) {
            for f in folders {
                out.push(format!("{prefix}\u{1f}{}", f.name));
                walk(prefix, &f.folders, out);
            }
        }
        let mut out = Vec::new();
        if let Some(source) = self.registry.sources().get(idx)
            && let Some(tree) = self.trees.borrow().get(idx)
        {
            walk(source.id(), &tree.folders, &mut out);
        }
        out
    }

    fn expand_all(&self, idx: usize) {
        let Some(source) = self.registry.sources().get(idx) else {
            return;
        };
        let section_key = format!("\u{1}{}", source.id());
        let folder_keys = self.folder_keys(idx);
        let mut c = self.collapsed.borrow_mut();
        c.remove(&section_key);
        for k in folder_keys {
            c.remove(&k);
        }
        drop(c);
        self.rebuild();
    }

    fn collapse_all(&self, idx: usize) {
        let folder_keys = self.folder_keys(idx);
        let mut c = self.collapsed.borrow_mut();
        for k in folder_keys {
            c.insert(k);
        }
        drop(c);
        self.rebuild();
    }

    /// The Expand All / Collapse All / Refresh menu shared by the section
    /// and folder rows of source `idx`.
    fn source_menu(&self, idx: usize, x: f64, y: f64) {
        let menu = ContextMenu::new(&self.frame);
        let p = self.clone();
        menu.add_item(None, "Expand All", None, None, move || p.expand_all(idx));
        let p = self.clone();
        menu.add_item(None, "Collapse All", None, None, move || {
            p.collapse_all(idx)
        });
        menu.add_separator();
        let p = self.clone();
        menu.add_item(Some("search"), "Refresh", None, None, move || {
            p.request_reload(idx)
        });
        menu.popup_at(&self.frame, x, y);
    }

    /// Rebuild every row from `trees` + `collapsed`.
    fn rebuild(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        let sources = self.registry.sources();
        let trees = self.trees.borrow();
        let mut any = false;

        for (idx, source) in sources.iter().enumerate() {
            if matches!(source.status(), SourceStatus::Unavailable(_)) {
                continue;
            }
            any = true;

            let section_key = format!("\u{1}{}", source.id());
            let expanded = !self.is_collapsed(&section_key);
            self.list
                .append(&self.section_row(idx, source, expanded, &section_key));
            if !expanded {
                continue;
            }

            let Some(tree) = trees.get(idx) else { continue };
            for folder in &tree.folders {
                self.append_folder(idx, source, folder, 1);
            }
            for doc in &tree.docs {
                self.list.append(&self.doc_row(idx, &doc.id, &doc.title, 1));
            }
        }

        self.frame.set_visible(any);
    }

    fn append_folder(
        &self,
        idx: usize,
        source: &Arc<dyn TextSource>,
        folder: &SourceFolder,
        depth: i32,
    ) {
        let key = format!("{}\u{1f}{}", source.id(), folder.name);
        let expanded = !self.is_collapsed(&key);
        self.list.append(&self.folder_row(
            idx,
            source.icon(),
            &folder.name,
            depth,
            expanded,
            key.clone(),
        ));
        if !expanded {
            return;
        }
        for sub in &folder.folders {
            self.append_folder(idx, source, sub, depth + 1);
        }
        for doc in &folder.docs {
            self.list
                .append(&self.doc_row(idx, &doc.id, &doc.title, depth + 1));
        }
    }

    // --- row builders --------------------------------------------------

    fn row_box(depth: i32) -> GtkBox {
        let hbox = GtkBox::new(Orientation::Horizontal, 0);
        hbox.set_valign(Align::Center);
        hbox.set_css_classes(&["file-list-row"]);
        hbox.set_margin_start(INDENT_PX * depth);
        hbox
    }

    fn chevron(expanded: bool) -> Label {
        let glyph = if expanded { "\u{25BE}" } else { "\u{25B8}" };
        let chevron = Label::new(Some(glyph));
        chevron.set_css_classes(&["dir-chevron"]);
        chevron
    }

    fn section_row(
        &self,
        idx: usize,
        source: &Arc<dyn TextSource>,
        expanded: bool,
        key: &str,
    ) -> gtk::ListBoxRow {
        let hbox = Self::row_box(0);
        hbox.add_css_class("file-tree-header");
        hbox.append(&Self::chevron(expanded));
        {
            let icon = icons::img(source.icon(), 16);
            icon.set_css_classes(&["file-icon"]);
            hbox.append(&icon);
        }
        let label = Label::new(Some(source.label()));
        label.set_css_classes(&["dir-label"]);
        hbox.append(&label);

        // "[read only]" hint — every external source is read-only.
        let ro = Label::new(Some("[read only]"));
        ro.set_css_classes(&["source-readonly-tag"]);
        ro.set_margin_start(6);
        hbox.append(&ro);

        let panel = self.clone();
        let key = key.to_string();
        let click = GestureClick::new();
        click.set_button(1);
        click.connect_released(move |_, _, _, _| panel.toggle_collapsed(key.clone()));
        hbox.add_controller(click);
        self.attach_source_menu(&hbox, idx);

        Self::wrap(hbox)
    }

    /// Right-click on a section / folder row → Expand All / Collapse All /
    /// Refresh for that source.
    fn attach_source_menu(&self, hbox: &GtkBox, idx: usize) {
        let panel = self.clone();
        let menu_click = GestureClick::new();
        menu_click.set_button(3);
        menu_click.connect_pressed(move |_, _, x, y| panel.source_menu(idx, x, y));
        hbox.add_controller(menu_click);
    }

    fn folder_row(
        &self,
        idx: usize,
        icon: &str,
        name: &str,
        depth: i32,
        expanded: bool,
        key: String,
    ) -> gtk::ListBoxRow {
        let hbox = Self::row_box(depth);
        hbox.append(&Self::chevron(expanded));
        {
            let img = icons::img(icon, 16);
            img.set_css_classes(&["file-icon"]);
            hbox.append(&img);
        }
        let label = Label::new(Some(name));
        label.set_css_classes(&["dir-label"]);
        hbox.append(&label);

        let panel = self.clone();
        let click = GestureClick::new();
        click.set_button(1);
        click.connect_released(move |_, _, _, _| panel.toggle_collapsed(key.clone()));
        hbox.add_controller(click);
        self.attach_source_menu(&hbox, idx);

        Self::wrap(hbox)
    }

    fn doc_row(&self, idx: usize, id: &DocId, title: &str, depth: i32) -> gtk::ListBoxRow {
        let hbox = Self::row_box(depth);
        // empty chevron slot so icons line up with folder rows
        let spacer = Label::new(None);
        spacer.set_css_classes(&["dir-chevron"]);
        hbox.append(&spacer);
        {
            // read-only external documents use the "documentation" glyph
            let img = icons::img("usage-documentation", 16);
            img.set_css_classes(&["file-icon"]);
            hbox.append(&img);
        }
        let label = Label::new(Some(title));
        hbox.append(&label);

        let panel = self.clone();
        let id_open = id.clone();
        let title_open = title.to_string();
        let open = GestureClick::new();
        open.set_button(1);
        open.connect_released(move |_, _, _, _| {
            panel.open_doc(idx, id_open.clone(), title_open.clone());
        });
        hbox.add_controller(open);

        let panel = self.clone();
        let id_menu = id.clone();
        let title_menu = title.to_string();
        let frame = self.frame.clone();
        let menu_click = GestureClick::new();
        menu_click.set_button(3);
        menu_click.connect_pressed(move |_, _, x, y| {
            let menu = ContextMenu::new(&frame);

            // Copy text: `document_text` is served from the in-memory body
            // cache the tree was built from, so this doesn't block the UI.
            let p = panel.clone();
            let id_copy = id_menu.clone();
            let frame_copy = frame.clone();
            menu.add_item(Some("copy"), "Copy text", None, None, move || {
                if let Some(src) = p.registry.sources().get(idx).cloned()
                    && let Ok(text) = src.document_text(&id_copy)
                {
                    frame_copy.clipboard().set_text(&text);
                }
            });

            let frame_title = frame.clone();
            let title_copy = title_menu.clone();
            menu.add_item(None, "Copy title", None, None, move || {
                frame_title.clipboard().set_text(&title_copy);
            });

            let p_refresh = panel.clone();
            menu.add_item(Some("search"), "Refresh", None, None, move || {
                p_refresh.request_reload(idx);
            });

            menu.popup_at(&panel.frame, x, y);
        });
        hbox.add_controller(menu_click);

        Self::wrap(hbox)
    }

    fn wrap(hbox: GtkBox) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_child(Some(&hbox));
        row
    }
}
