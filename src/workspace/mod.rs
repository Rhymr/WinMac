pub mod controller;
pub mod manager;
pub mod recent;
pub mod session;

use crate::app::context_menu::ContextMenu;
use crate::editor::TextEditor;
use crate::file::ops::FileOps;
use crate::file::tree::FileTree;
use controller::WorkspaceController;
use gtk::prelude::*;
use gtk::{
    Box, Button, EventSequenceState, Frame, GestureClick, Label, Notebook, TextBuffer, TextView,
    Widget, Window, gdk,
};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

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
        // Scrollable: when the open tabs don't fit, each tab's label
        // ellipsizes toward its `width_chars` minimum (see
        // `build_tab_widget`) and only once even those minimums overflow
        // does the header hand off to paging arrows — JetBrains-style. No
        // per-frame relayout: the label's own min/natural sizing does the
        // shrinking.
        let notebook = Notebook::builder()
            .scrollable(true)
            .show_border(false)
            .css_classes(vec!["workspace-notebook"])
            .build();

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

        match fs::read_to_string(&path) {
            Ok(content) => {
                log::debug!("opening tab for {path:?}");
                self.add_new_tab(&path, &content);
            }
            Err(e) => log::warn!("could not open {path:?}: {e}"),
        }
    }

    /// Open an external-source document (Apple Notes, …) in a **read-only**
    /// tab. The body is staged to a temp file so the existing tab / editor
    /// machinery can be reused unchanged; the editor is then locked so it
    /// can't be typed into and never autosaves.
    pub fn open_readonly(&self, title: &str, body: &str) {
        let mut dir = std::env::temp_dir();
        dir.push("rhymr-external");
        if fs::create_dir_all(&dir).is_err() {
            return;
        }
        let safe: String = title
            .chars()
            .map(|c| {
                if matches!(c, '/' | ':' | '\\') {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        let path = dir.join(format!("{safe}.txt"));
        if fs::write(&path, body).is_err() {
            return;
        }

        self.open_path(path.clone());

        let index = self.open_files.borrow().iter().position(|p| p == &path);
        if let Some(index) = index {
            if let Some(editor) = self.text_editors.borrow().get(index) {
                editor.set_editable(false);
            }
            // Swap the tab's icon for the read-only "documentation" glyph.
            if let Some(page) = self.notebook.nth_page(Some(index as u32))
                && let Some(tab) = self.notebook.tab_label(&page)
                && let Ok(tab_box) = tab.downcast::<Box>()
                && let Some(old_icon) = tab_box.first_child()
            {
                let icon = crate::app::icons::img("usage-documentation", 16);
                icon.set_css_classes(&["tab-icon"]);
                tab_box.remove(&old_icon);
                tab_box.prepend(&icon);
            }
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
                log::error!("failed to save {path:?}: {e}");
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
                Err(e) => log::error!("failed to reload {path:?}: {e}"),
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
    // every tab to its own size instead, so the strip stays left aligned,
    // JetBrains-style, with a plain empty band after the last tab.
    // `tab_fill` stays at its default (true): a tab sizes to its full
    // natural width when there's room and ellipsizes toward the label's
    // `width_chars` minimum once the strip is crowded (`scrollable(true)`).
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

        // Active tab's selection seeds the Rhyme Search box (when it's open).
        let notebook_for_sel = notebook.clone();
        let controller_for_sel = controller.clone();
        text_editor.connect_selection_notify(move |word| {
            if notebook_for_sel.current_page() == Some(page_num) {
                controller_for_sel.notify_selection(word);
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

/// Longest a tab name is shown before it's cut with an ellipsis.
const MAX_TAB_CHARS: usize = 18;

/// Cap a tab name at [`MAX_TAB_CHARS`], breaking on the last word boundary
/// within the limit where there is one so a title doesn't cut mid-word.
fn truncate_tab_name(name: &str) -> String {
    if name.chars().count() <= MAX_TAB_CHARS {
        return name.to_string();
    }
    let head: String = name.chars().take(MAX_TAB_CHARS).collect();
    let cut = match head.rfind(' ') {
        Some(sp) if sp >= MAX_TAB_CHARS / 2 => &head[..sp],
        _ => head.trim_end(),
    };
    format!("{}\u{2026}", cut.trim_end())
}

/// Icon + truncated label + close button for one tab — shared by the
/// initial tab creation and by rename/Save As updates (`set_tab_label`) so
/// both build an identical widget.
fn build_tab_widget(path: &Path) -> (Box, Button) {
    let display_path = path.to_string_lossy().into_owned();
    let tab_box = Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .css_classes(vec!["tab-box"])
        // Even gap between icon, name and × — the tab's own left/right
        // padding (notebook.scss) matches it so the whole tab reads evenly.
        .spacing(6)
        // Shown on hover so a truncated name still identifies itself.
        .tooltip_text(display_path)
        .build();

    // Same file icon the file tree uses for an "open" row, so a tab's icon
    // matches what the user sees in the tree.
    let icon = crate::app::icons::img("file", 16);
    icon.set_css_classes(&["tab-icon"]);

    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(crate::file::tree::strip_txt_extension)
        .unwrap_or("Untitled");

    // Tabs size to their content, capped at MAX_TAB_CHARS. A plain
    // non-ellipsizing label makes min == natural == text width, so
    // GtkNotebook (which allocates non-expand tabs their minimum) still
    // shows the whole name; the cap is applied here in Rust rather than via
    // Pango ellipsize, whose "natural width" would collapse to just "…".
    // The full name is always on the tab's tooltip.
    let shown_name = truncate_tab_name(display_name);

    let label = Label::new(Some(&shown_name));
    label.set_css_classes(&["tab-label"]);
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);

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

    // Explicit non-expand so GtkNotebook never stretches a tab past its
    // content width.
    tab_box.set_hexpand(false);
    tab_box.append(&icon);
    tab_box.append(&label);
    tab_box.append(&close_button);

    (tab_box, close_button)
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

        // Parent the popover to the notebook, not the tab's own box: a
        // popover parented into the horizontal `tab_box` gets counted by
        // `GtkBox::measure` and visibly balloons the tab while open.
        let menu = ContextMenu::new(&notebook);

        let c = controller.clone();
        menu.add_item(None, "Close", None, None, move || {
            if let Some(ws) = c.get_workspace() {
                ws.close_tab_at(index);
            }
        });

        let c = controller.clone();
        let other_btn = menu.add_item(None, "Close Other Tabs", None, None, move || {
            if let Some(ws) = c.get_workspace() {
                ws.close_other_tabs(index);
            }
        });
        other_btn.set_sensitive(tab_count > 1);

        let c = controller.clone();
        menu.add_item(None, "Close All Tabs", None, None, move || {
            if let Some(ws) = c.get_workspace() {
                ws.close_all_tabs();
            }
        });

        let c = controller.clone();
        let any_unmodified = controller
            .get_workspace()
            .map(|ws| ws.any_unmodified_tabs())
            .unwrap_or(false);
        let unmodified_btn = menu.add_item(None, "Close Unmodified Tabs", None, None, move || {
            if let Some(ws) = c.get_workspace() {
                ws.close_unmodified_tabs();
            }
        });
        unmodified_btn.set_sensitive(any_unmodified);

        let c = controller.clone();
        let left_btn = menu.add_item(None, "Close Tabs to the Left", None, None, move || {
            if let Some(ws) = c.get_workspace() {
                ws.close_tabs_to_left(index);
            }
        });
        left_btn.set_sensitive(index > 0);

        menu.add_separator();

        let widget_for_clipboard = tab_box_for_popup.clone();
        let path_for_copy = path.clone();
        menu.add_item(None, "Copy Path/Reference...", None, None, move || {
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
