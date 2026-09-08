pub mod controller;
pub mod manager;
pub mod recent;

use crate::app::context_menu::ContextMenu;
use crate::editor::TextEditor;
use crate::file::ops::FileOps;
use crate::file::tree::FileTree;
use controller::WorkspaceController;
use gtk::prelude::*;
use gtk::{
    Box, Button, EventSequenceState, Frame, GestureClick, Image, Label, Notebook, TextBuffer,
    TextView, Widget, Window, gdk, pango,
};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Same file icon the file tree uses for an "open" row — reused here so a
/// tab's icon matches what the user sees in the tree.
const TAB_ICON_RESOURCE: &str = "/org/gtk_rs/rhymr/icons/gg-tree-file.svg";

pub struct Workspace {
    frame: Frame,
    pub(crate) notebook: Notebook,
    pub(crate) open_files: Rc<RefCell<Vec<PathBuf>>>,
    // Index-aligned with open_files (and with notebook pages, once the
    // empty-state placeholder page is accounted for) — lets path updates
    // (rename, Save As) retarget an already-open tab's auto-save.
    text_editors: Rc<RefCell<Vec<TextEditor>>>,
    controller: Rc<WorkspaceController>,
    pub(crate) file_tree: Option<FileTree>,
}

impl Workspace {
    pub fn new(controller: Rc<WorkspaceController>, file_tree: Option<FileTree>) -> Self {
        // Starts non-scrollable: GTK only ever shrinks tab allocations
        // toward their ellipsized minimum (rather than growing the window)
        // when scrolling is off — with it on, tabs stay at full width and
        // overflow behind scroll arrows instead. `adapt_tab_display` (see
        // its tick callback below) is what turns scrolling back on, but
        // only once every tab has already been squeezed down to icon-only
        // and *still* doesn't fit — the last resort, not the first one.
        let notebook = Notebook::builder()
            .scrollable(false)
            .show_border(false)
            .css_classes(vec!["workspace-notebook"])
            .build();

        // Keeps the tab strip from ever pushing the window wider: every
        // frame, shrink open tabs toward icon-only before falling back to
        // paging arrows, rather than letting the header just demand more
        // width than it's been given.
        notebook.add_tick_callback(|notebook, _clock| {
            adapt_tab_display(notebook);
            glib::ControlFlow::Continue
        });

        let open_files = Rc::new(RefCell::new(Vec::new()));

        let workspace = Self {
            frame: Frame::builder()
                .css_classes(vec!["workspace-frame"])
                .child(&notebook)
                .build(),
            notebook: notebook.clone(),
            open_files: open_files.clone(),
            text_editors: Rc::new(RefCell::new(Vec::new())),
            controller,
            file_tree,
        };

        // Check if there are no open files and display the empty state
        if workspace.open_files.borrow().is_empty() {
            let empty_state = workspace.create_empty_state();
            notebook.append_page(&empty_state, Option::<&gtk::Widget>::None);
            notebook.set_show_tabs(false);
        }

        // Keep the status bar's word count + caret readout pointed at
        // whichever tab is active.
        let controller_for_switch = workspace.controller.clone();
        notebook.connect_switch_page(move |_, _, _| {
            controller_for_switch.refresh_word_count();
            controller_for_switch.refresh_cursor();
            controller_for_switch.refresh_nav();
        });

        workspace
    }

    pub fn set_controller(&mut self, controller: Rc<WorkspaceController>) {
        // Store the controller for use in tab operations
        self.controller = controller;
    }

    pub fn get_controller(&self) -> &Rc<WorkspaceController> {
        &self.controller
    }

    pub fn add_new_tab(&self, path: &Path, content: &str) -> u32 {
        // Remove empty state if it exists
        if self.notebook.n_pages() == 1 && self.open_files.borrow().is_empty() {
            self.notebook.remove_page(Some(0));
        }

        // Ensure the controller is still referenced
        let controller = self.controller.clone();
        let (page_num, text_editor) = add_new_tab(&self.notebook, path, content, Some(controller));
        self.open_files.borrow_mut().push(path.to_path_buf());
        self.text_editors.borrow_mut().push(text_editor);

        // Refresh so the file's row picks up the "open" icon, then highlight it
        if let Some(ref file_tree) = self.file_tree {
            file_tree.refresh();
            file_tree.select_path(path);
        }

        // Ensure notebook shows tabs and has proper styling
        self.notebook.set_show_tabs(true);
        self.notebook.add_css_class("has-open-files");

        self.controller.refresh_word_count();
        self.controller.refresh_nav();

        page_num
    }

    /// Open `path` in the editor: focus its existing tab if already open,
    /// otherwise read it from disk and create a new tab.
    pub fn open_path(&self, path: PathBuf) {
        if path.is_dir() {
            return;
        }

        if let Some(index) = self.open_files.borrow().iter().position(|p| p == &path) {
            self.switch_to_tab(index);
            return;
        }

        if let Ok(content) = fs::read_to_string(&path) {
            self.add_new_tab(&path, &content);
        }
    }

    pub fn get_current_buffer(&self) -> Option<(TextBuffer, Option<PathBuf>)> {
        let current_page = self.notebook.current_page()?;
        let page = self.notebook.nth_page(Some(current_page))?;
        let text_view = text_view_for_page(&page)?;

        // Get the buffer and path
        let buffer = text_view.buffer();
        let path = self.open_files.borrow().get(current_page as usize).cloned();

        // Return the tuple directly since we already handled the Option
        Some((buffer, path))
    }

    /// Save every open tab to its (already-known) path on disk.
    pub fn save_all(&self) {
        let paths = self.open_files.borrow().clone();
        for (index, path) in paths.iter().enumerate() {
            let Some(page) = self.notebook.nth_page(Some(index as u32)) else {
                continue;
            };
            let Some(text_view) = text_view_for_page(&page) else {
                continue;
            };
            let buffer = text_view.buffer();
            let content = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            if let Err(e) = fs::write(path, content.as_str()) {
                eprintln!("Failed to save {path:?}: {e}");
            }
        }

        if let Some(root) = self.controller.get_root_path() {
            crate::git::ops::stage_all_changes(&root);
        }
        if let Some(ref file_tree) = self.file_tree {
            file_tree.refresh();
        }
    }

    /// Reload every open tab's contents from disk, discarding any unsaved
    /// in-editor changes — useful after files changed outside the app.
    pub fn reload_all(&self) {
        let paths = self.open_files.borrow().clone();
        for (index, path) in paths.iter().enumerate() {
            let Some(page) = self.notebook.nth_page(Some(index as u32)) else {
                continue;
            };
            let Some(text_view) = text_view_for_page(&page) else {
                continue;
            };
            match fs::read_to_string(path) {
                Ok(content) => text_view.buffer().set_text(&content),
                Err(e) => eprintln!("Failed to reload {path:?}: {e}"),
            }
        }
    }

    pub fn update_current_tab_path(&self, new_path: PathBuf) {
        if let Some(current_page) = self.notebook.current_page() {
            self.set_tab_label(current_page, &new_path);

            if let Some(path) = self.open_files.borrow_mut().get_mut(current_page as usize) {
                *path = new_path.clone();
            }
            // Retarget auto-save to the new path (Save As)
            if let Some(editor) = self.text_editors.borrow().get(current_page as usize) {
                editor.set_path(new_path.clone());
            }

            // The file may be new on disk (Save As) — rebuild the tree to show it
            if let Some(ref file_tree) = self.file_tree {
                file_tree.refresh();
                file_tree.select_path(&new_path);
            }
        }
    }

    /// Rewrite `old_path` (a file, or a directory whose contents moved) to
    /// `new_path` for every open tab affected, keeping tab titles and saves
    /// pointed at the right place after a file-tree rename.
    pub fn rename_path(&self, old_path: &Path, new_path: &Path) {
        let affected: Vec<(u32, PathBuf)> = {
            let mut open_files = self.open_files.borrow_mut();
            let mut affected = Vec::new();
            for (index, path) in open_files.iter_mut().enumerate() {
                let updated = if path == old_path {
                    Some(new_path.to_path_buf())
                } else if let Ok(rel) = path.strip_prefix(old_path) {
                    Some(new_path.join(rel))
                } else {
                    None
                };

                if let Some(updated) = updated {
                    *path = updated.clone();
                    affected.push((index as u32, updated));
                }
            }
            affected
        };

        for (page_num, path) in affected {
            self.set_tab_label(page_num, &path);
            // Retarget auto-save to the new path
            if let Some(editor) = self.text_editors.borrow().get(page_num as usize) {
                editor.set_path(path);
            }
        }
    }

    /// Close any open tab pointed at `path` itself, or nested under it —
    /// used after a file-tree delete removes something out from under an
    /// open editor.
    pub fn close_paths_under(&self, path: &Path) {
        let indices: Vec<usize> = self
            .open_files
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, p)| *p == path || p.starts_with(path))
            .map(|(index, _)| index)
            .collect();

        // Remove from the highest index down so earlier indices stay valid
        for index in indices.into_iter().rev() {
            self.remove_tab(index);
        }
    }

    fn set_tab_label(&self, page_num: u32, path: &Path) {
        let Some(page) = self.notebook.nth_page(Some(page_num)) else {
            return;
        };

        let (tab_box, close_button) = build_tab_widget(path);
        wire_tab_close_button(&close_button, &self.controller);
        wire_tab_context_menu(&tab_box, &self.notebook, &page, &self.controller, path);

        self.notebook.set_tab_label(&page, Some(&tab_box));
    }

    pub fn get_widget(&self) -> &Frame {
        &self.frame
    }

    pub fn create_empty_state(&self) -> Box {
        let empty_state = Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(vec!["empty-state-box"])
            .valign(gtk::Align::Center)
            .halign(gtk::Align::Center)
            .build();

        let new_file_box = Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .halign(gtk::Align::Center)
            .build();

        new_file_box.set_css_classes(&["empty-file-box"]);

        let new_file_btn = Button::builder()
            .label("New file")
            .css_classes(vec!["empty-file-btn"])
            .build();

        let new_file_ctrl = Label::builder()
            .label("^N")
            .css_classes(vec!["empty-file-ctrl"])
            .build();

        new_file_box.append(&new_file_btn);
        new_file_box.append(&new_file_ctrl);

        let open_file_box = Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .halign(gtk::Align::Center)
            .build();

        open_file_box.set_css_classes(&["empty-file-box"]);

        let open_file_btn = Button::builder()
            .label("Open file")
            .css_classes(vec!["empty-file-btn"])
            .build();

        let open_file_ctrl = Label::builder()
            .label("^O")
            .css_classes(vec!["empty-file-ctrl"])
            .build();

        open_file_box.append(&open_file_btn);
        open_file_box.append(&open_file_ctrl);

        let save_file_box = Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .halign(gtk::Align::Center)
            .build();

        save_file_box.set_css_classes(&["empty-file-box"]);

        let save_file = Label::builder()
            .label("Save file")
            .css_classes(vec!["empty-file-btn"])
            .build();

        let save_file_ctrl = Label::builder()
            .label("^S")
            .css_classes(vec!["empty-file-ctrl"])
            .build();

        save_file_box.append(&save_file);
        save_file_box.append(&save_file_ctrl);

        let close_tab_box = Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .halign(gtk::Align::Center)
            .build();

        close_tab_box.set_css_classes(&["empty-file-box"]);

        let close_tab = Label::builder()
            .label("Close tab")
            .css_classes(vec!["empty-file-btn"])
            .build();

        let close_tab_ctrl = Label::builder()
            .label("^W")
            .css_classes(vec!["empty-file-ctrl"])
            .build();

        close_tab_box.append(&close_tab);
        close_tab_box.append(&close_tab_ctrl);

        // Clone the Rc pointers to avoid borrowing issues
        let controller_ref = self.controller.clone();

        new_file_btn.connect_clicked(move |_button| {
            controller_ref.handle_new_file();
        });

        let controller_ref = self.controller.clone();

        open_file_btn.connect_clicked(move |button| {
            if let Some(window) = button.root().and_downcast::<Window>()
                && let Some((path, content)) = FileOps::open_file(Some(window.clone()))
                && let Some(workspace) = controller_ref.get_workspace()
            {
                workspace.add_new_tab(&path, &content);
            }
        });

        empty_state.append(&new_file_box);
        empty_state.append(&open_file_box);
        empty_state.append(&save_file_box);
        empty_state.append(&close_tab_box);

        empty_state
    }

    pub fn remove_tab(&self, index: usize) {
        // Remove the tab
        self.notebook.remove_page(Some(index as u32));
        self.open_files.borrow_mut().remove(index);
        self.text_editors.borrow_mut().remove(index);

        // Show empty state if no more tabs
        if self.notebook.n_pages() == 0 {
            self.notebook.remove_css_class("has-open-files");
            let empty_state = self.create_empty_state();
            self.notebook
                .append_page(&empty_state, Option::<&gtk::Widget>::None);
            self.notebook.set_show_tabs(false);
        }

        // The closed file's row should drop back to the "not open" icon
        if let Some(ref file_tree) = self.file_tree {
            file_tree.refresh();
            if let Some((_, Some(path))) = self.get_current_buffer() {
                file_tree.select_path(&path);
            }
        }

        self.controller.refresh_word_count();
    }

    pub fn switch_to_tab(&self, index: usize) {
        if index < self.notebook.n_pages() as usize {
            self.notebook.set_current_page(Some(index as u32));
        }
    }

    /// Close the tab at `index` — the tab context menu's "Close".
    pub fn close_tab_at(&self, index: usize) {
        if index < self.open_files.borrow().len() {
            self.remove_tab(index);
        }
    }

    /// Close every open tab except `keep_index`. Highest index first, so
    /// removing one never invalidates the indices of tabs still to close.
    pub fn close_other_tabs(&self, keep_index: usize) {
        let count = self.open_files.borrow().len();
        for index in (0..count).rev() {
            if index != keep_index {
                self.remove_tab(index);
            }
        }
    }

    pub fn close_all_tabs(&self) {
        while !self.open_files.borrow().is_empty() {
            self.remove_tab(0);
        }
    }

    /// Close every tab to the left of `index` (not including it).
    pub fn close_tabs_to_left(&self, index: usize) {
        // The `Ref` from `.borrow()` would otherwise live for the whole
        // loop (a `for` header's temporaries aren't dropped until the loop
        // ends, unlike a `while` condition's), and `remove_tab`'s own
        // `borrow_mut()` would panic on the second iteration.
        let count = self.open_files.borrow().len();
        for i in (0..index.min(count)).rev() {
            self.remove_tab(i);
        }
    }

    /// Close every tab that hasn't been edited since it was opened (or
    /// whose edits have already been autosaved) — mirrors JetBrains'
    /// "Close Unmodified Tabs".
    pub fn close_unmodified_tabs(&self) {
        let indices: Vec<usize> = self
            .text_editors
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, editor)| !editor.is_modified())
            .map(|(index, _)| index)
            .collect();
        for index in indices.into_iter().rev() {
            self.remove_tab(index);
        }
    }

    pub fn any_unmodified_tabs(&self) -> bool {
        self.text_editors
            .borrow()
            .iter()
            .any(|editor| !editor.is_modified())
    }

    /// Live-apply `settings` to every currently open tab — called after the
    /// settings dialog saves, so a toggle takes effect immediately instead
    /// of only for tabs opened afterward.
    pub fn apply_settings_to_open_tabs(&self, settings: &crate::setting::Settings) {
        for editor in self.text_editors.borrow().iter() {
            editor.apply_settings(settings);
        }
    }
}

fn add_new_tab(
    notebook: &Notebook,
    path: &Path,
    content: &str,
    controller: Option<Rc<WorkspaceController>>,
) -> (u32, TextEditor) {
    // Create text editor
    let text_editor = TextEditor::new();
    text_editor.set_text(content);
    // Set after set_text(): loading the initial content also fires the
    // buffer's "changed" signal, and we don't want that mistaken for a
    // real edit that needs auto-saving.
    text_editor.set_path(path.to_path_buf());

    let (tab_box, close_button) = build_tab_widget(path);

    // Add the page with our custom tab
    let page_widget: Widget = text_editor.get_widget().clone().upcast();
    let page_num = notebook.append_page(&page_widget, Some(&tab_box));

    // `tab_expand(false)`: without it, GTK stretches each tab to fill any
    // leftover header width (and centers the row while it's at it) — pin
    // every tab to its own size instead, so the strip stays left aligned.
    // `tab_fill` is deliberately left at its default (true): that's what
    // lets a tab size to its full natural (un-ellipsized) width when
    // there's room, only shrinking toward the label's ellipsized minimum
    // once the open tabs collectively overflow the header — setting it
    // false here instead made every tab render at minimum width always,
    // collapsing names even with plenty of space free.
    let page = notebook.page(&page_widget);
    page.set_tab_expand(false);

    if let Some(controller) = controller {
        // Only the active tab's edits should move the status bar's word
        // count — background tabs keep typing (autosave) without it.
        let notebook_for_word_count = notebook.clone();
        let controller_for_word_count = controller.clone();
        text_editor.connect_changed(move || {
            if notebook_for_word_count.current_page() == Some(page_num) {
                controller_for_word_count.refresh_word_count();
            }
        });

        let notebook_for_cursor = notebook.clone();
        let controller_for_cursor = controller.clone();
        text_editor.connect_cursor_notify(move || {
            if notebook_for_cursor.current_page() == Some(page_num) {
                controller_for_cursor.refresh_cursor();
            }
        });

        wire_tab_close_button(&close_button, &controller);
        wire_tab_context_menu(&tab_box, notebook, &page_widget, &controller, path);
    }

    notebook.set_show_tabs(true);
    notebook.add_css_class("has-open-files");
    notebook.set_current_page(Some(page_num));

    (page_num, text_editor)
}

/// Icon + truncated label + close button for one tab — shared by the
/// initial tab creation and by rename/Save As updates (`set_tab_label`) so
/// both build an identical widget.
fn build_tab_widget(path: &Path) -> (Box, Button) {
    let display_path = path.to_string_lossy().into_owned();
    let tab_box = Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(vec!["tab-box"])
        .spacing(3)
        // Read regardless of label visibility, so a tab shrunk down to
        // icon-only (see `adapt_tab_display`) still identifies itself on
        // hover.
        .tooltip_text(display_path)
        .build();

    let icon = Image::from_resource(TAB_ICON_RESOURCE);
    icon.set_css_classes(&["tab-icon"]);
    icon.set_pixel_size(16);

    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(crate::file::tree::strip_txt_extension)
        .unwrap_or("Untitled");

    let label = Label::new(Some(display_name));
    label.set_css_classes(&["tab-label"]);
    label.set_ellipsize(pango::EllipsizeMode::End);
    // A `Label` with ellipsize on but no explicit width hint requests only
    // its *minimum* size (just enough for "…") as its natural size too —
    // there's otherwise no basis for GTK to know it should ask for more.
    // `max_width_chars` gives it a generous natural-size ceiling instead
    // (comfortably past any real filename, so a tab shows its full name by
    // default), while `width_chars` sets the actual minimum it can shrink
    // down toward once the open tabs don't all fit — see
    // `Notebook::scrollable(false)` in `Workspace::new`.
    label.set_width_chars(8);
    label.set_max_width_chars(28);
    label.set_halign(gtk::Align::Start);

    // Visibility is handled entirely by CSS (`tab:checked`/`tab:hover` in
    // notebook.scss) rather than tracked here — GTK's own `:checked` state
    // on the tab is always correct, unlike hand-rolled bookkeeping that has
    // to be re-run on every switch/add/remove and is easy to miss a spot on.
    // A bare red "×" (JetBrains-style) — a plain label, so no circular
    // symbolic-icon backdrop.
    let close_button = Button::builder()
        .css_classes(vec!["tab-close-button"])
        .build();
    close_button.set_child(Some(&Label::new(Some("\u{2715}"))));

    tab_box.append(&icon);
    tab_box.append(&label);
    tab_box.append(&close_button);

    (tab_box, close_button)
}

/// Keeps the open tabs from ever forcing the notebook (and so the window)
/// wider than it already is. Run every frame from a tick callback (GTK
/// gives no resize/allocation-changed signal for a stock widget we haven't
/// subclassed, and this needs to react to both window resizes and tabs
/// being added/removed) it re-measures every tab and picks the least
/// cramped of three tiers that still fits:
///
/// 1. Full labels — every tab at its natural (un-ellipsized) width.
/// 2. Ellipsized labels — GTK's own min/natural shrink already handles
///    this; no help needed from here.
/// 3. Icon-only — labels hidden entirely.
///
/// Only once even icon-only tabs collectively don't fit does it fall back
/// to paging arrows (`scrollable`), so the header hands off to those
/// rather than to a wider window.
fn adapt_tab_display(notebook: &Notebook) {
    let available = notebook.width() - 16;
    if available <= 0 {
        return;
    }

    let mut tab_widgets = Vec::new();
    let mut labels = Vec::new();

    for i in 0..notebook.n_pages() {
        let Some(page) = notebook.nth_page(Some(i)) else {
            continue;
        };
        // No tab label at all on the empty-state placeholder page.
        let Some(tab_widget) = notebook.tab_label(&page) else {
            continue;
        };
        let Some(label) = tab_widget
            .first_child()
            .and_then(|icon| icon.next_sibling())
            .and_then(|w| w.downcast::<Label>().ok())
        else {
            continue;
        };
        tab_widgets.push(tab_widget);
        labels.push(label);
    }

    if labels.is_empty() {
        return;
    }

    // Always measure as if every label were visible first, regardless of
    // whatever state they're currently in. Measuring a tab whose label is
    // *already* hidden would report its "full" width as just the
    // icon/close-button footprint (a hidden child contributes ~nothing to
    // its box's size) — nowhere near what showing it back would actually
    // need — and the strip would flip back to full labels next frame, only
    // to immediately re-collapse the frame after that: an infinite
    // show/hide oscillation, which is what tabs visibly flying off to the
    // right turned out to be. Toggling visible→hidden synchronously within
    // this same callback, before layout/paint for this frame happens,
    // doesn't flicker — only the final state at the end of this function
    // is ever actually drawn.
    for label in &labels {
        if !label.is_visible() {
            label.set_visible(true);
        }
    }

    let mut full_natural_total = 0;
    let mut min_total = 0;
    for tab_widget in &tab_widgets {
        let (min_w, natural_w, _, _) = tab_widget.measure(gtk::Orientation::Horizontal, -1);
        full_natural_total += natural_w;
        min_total += min_w;
    }

    if full_natural_total <= available || min_total <= available {
        // Labels are already visible from the measurement pass above.
        if notebook.is_scrollable() {
            notebook.set_scrollable(false);
        }
        return;
    }

    for label in &labels {
        label.set_visible(false);
    }

    // Real re-measurement now that labels are actually hidden, rather than
    // the fixed `TAB_ICON_ONLY_WIDTH_ESTIMATE` guess this used to compare
    // against — any mismatch between an estimate and each tab's genuine
    // icon-only footprint (padding/border/margin from notebook.scss) could
    // leave tabs overflowing uncontained, since a non-scrollable notebook
    // doesn't clip its header.
    let icon_only_total: i32 = tab_widgets
        .iter()
        .map(|w| w.measure(gtk::Orientation::Horizontal, -1).1)
        .sum();
    let need_scrolling = icon_only_total > available;
    if notebook.is_scrollable() != need_scrolling {
        notebook.set_scrollable(need_scrolling);
    }
}

fn wire_tab_close_button(close_button: &Button, controller: &Rc<WorkspaceController>) {
    let controller = controller.clone();
    close_button.connect_clicked(move |button| {
        if let Some(window) = button.root().and_downcast::<Window>() {
            controller.handle_close_tab(&window);
        }
    });
}

/// Right-click menu for a tab: Close/Close Other/Close All/Close
/// Unmodified/Close Tabs to the Left, plus Copy Path — matching the same
/// `ContextMenu` used by the file tree and editor. `page_widget` (the
/// page's content, not the tab label) is used to look up this tab's
/// *current* page index at click time via `Notebook::page_num`, so the
/// menu still targets the right tab even after other tabs have been
/// closed/reordered since this one was created.
fn wire_tab_context_menu(
    tab_box: &Box,
    notebook: &Notebook,
    page_widget: &Widget,
    controller: &Rc<WorkspaceController>,
    path: &Path,
) {
    let gesture = GestureClick::new();
    gesture.set_button(gdk::BUTTON_SECONDARY);

    let notebook = notebook.clone();
    let page_widget = page_widget.clone();
    let controller = controller.clone();
    let path = path.to_path_buf();
    let tab_box_for_popup = tab_box.clone();

    gesture.connect_pressed(move |gesture, _n_press, _x, _y| {
        gesture.set_state(EventSequenceState::Claimed);
        let Some(index) = notebook.page_num(&page_widget) else {
            return;
        };
        let index = index as usize;
        let tab_count = notebook.n_pages() as usize;

        let menu = ContextMenu::new(&tab_box_for_popup);

        let c = controller.clone();
        menu.add_item("Close", None, None, move || {
            if let Some(ws) = c.get_workspace() {
                ws.close_tab_at(index);
            }
        });

        let c = controller.clone();
        let other_btn = menu.add_item("Close Other Tabs", None, None, move || {
            if let Some(ws) = c.get_workspace() {
                ws.close_other_tabs(index);
            }
        });
        other_btn.set_sensitive(tab_count > 1);

        let c = controller.clone();
        menu.add_item("Close All Tabs", None, None, move || {
            if let Some(ws) = c.get_workspace() {
                ws.close_all_tabs();
            }
        });

        let c = controller.clone();
        let any_unmodified = controller
            .get_workspace()
            .map(|ws| ws.any_unmodified_tabs())
            .unwrap_or(false);
        let unmodified_btn = menu.add_item("Close Unmodified Tabs", None, None, move || {
            if let Some(ws) = c.get_workspace() {
                ws.close_unmodified_tabs();
            }
        });
        unmodified_btn.set_sensitive(any_unmodified);

        let c = controller.clone();
        let left_btn = menu.add_item("Close Tabs to the Left", None, None, move || {
            if let Some(ws) = c.get_workspace() {
                ws.close_tabs_to_left(index);
            }
        });
        left_btn.set_sensitive(index > 0);

        menu.add_separator();

        let widget_for_clipboard = tab_box_for_popup.clone();
        let path_for_copy = path.clone();
        menu.add_item("Copy Path/Reference...", None, None, move || {
            widget_for_clipboard
                .clipboard()
                .set_text(&path_for_copy.to_string_lossy());
        });

        menu.popup_below(&tab_box_for_popup);
    });

    tab_box.add_controller(gesture);
}

/// Descend from a notebook page's root widget to its actual TextView
/// (SourceView, which extends TextView). A tab page is
/// `Frame -> Overlay -> ScrolledWindow -> SourceView` (see TextEditor);
/// walk `first_child()` until a TextView turns up rather than hard-coding
/// the hop count.
fn text_view_for_page(page: &gtk::Widget) -> Option<TextView> {
    let mut widget = page.first_child();
    for _ in 0..6 {
        let current = widget?;
        if let Ok(view) = current.clone().downcast::<TextView>() {
            return Some(view);
        }
        widget = current.first_child();
    }
    None
}
