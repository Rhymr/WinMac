use crate::git::ops::GitFileStatus;
use crate::workspace::Workspace;
use gtk::gdk;
use gtk::pango;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, DragSource, DropTarget, Frame, GestureClick, Image, Label, ListBox,
    ListBoxRow, Orientation, ScrolledWindow,
};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const IGNORED_ENTRIES: [&str; 2] = ["target", "node_modules"];
const INDENT_PX: i32 = 16;

/// The tree's rendering/data model. Context menus, keyboard shortcuts, and
/// every file-mutating operation (new/rename/cut/copy/paste/delete/move)
/// live in a second `impl FileTree` block in tree_menu.rs, which is
/// why several fields below are `pub(crate)` rather than private.
#[derive(Clone)]
pub struct FileTree {
    // pub(crate): popovers (tree_menu.rs) parent to this rather than to
    // a row's hbox, so a context menu's contents sit outside the `.file-list`
    // CSS subtree entirely — otherwise its unscoped `row:hover *` / `& label`
    // rules bleed into the popover and repaint menu items with row colors.
    pub(crate) frame: Frame,
    pub(crate) file_list: ListBox,
    // Shared via Rc<RefCell<..>> (rather than a plain Option) so that every
    // clone of FileTree — including the one Workspace holds, made before
    // set_workspace() ever runs — observes the same value once it's set.
    pub(crate) workspace: Rc<RefCell<Option<Rc<Workspace>>>>,
    pub(crate) root_path: Rc<RefCell<Option<PathBuf>>>,
    pub(crate) entries: Rc<RefCell<Vec<(PathBuf, bool)>>>,
    pub(crate) collapsed: Rc<RefCell<HashSet<PathBuf>>>,
    // Cut/Copy clipboard: the path, and whether it was a Cut (vs Copy).
    pub(crate) clipboard: Rc<RefCell<Option<(PathBuf, bool)>>>,
    // Uncommitted-changes status vs HEAD, refreshed alongside the tree.
    // Empty whenever the workspace isn't a git repo.
    git_statuses: Rc<RefCell<HashMap<PathBuf, GitFileStatus>>>,
    // Absolute paths git ignores (one per ignored dir). A row whose path
    // is, or is under, one of these is greyed goldenrod (`.file-ignored`).
    ignored: Rc<RefCell<HashSet<PathBuf>>>,
    // Decoded once and shared as *paintable data*, not as widgets: a GTK
    // widget can only ever have one parent, so reusing the same Image
    // widget instance across rows silently reparents it away from whichever
    // row last claimed it, leaving every other row blank. Each row instead
    // builds its own Image from the shared paintable, which is cheap and
    // avoids re-decoding the SVG per row.
    folder_icon: Option<gdk::Paintable>,
    file_icon: Option<gdk::Paintable>,
    json_icon: Option<gdk::Paintable>,
}

impl Default for FileTree {
    fn default() -> Self {
        Self::new()
    }
}

impl FileTree {
    pub fn new() -> Self {
        // Create a ListBox to display the workspace files
        let file_list = ListBox::builder()
            .css_classes(vec!["file-list"])
            .selection_mode(gtk::SelectionMode::Single)
            .build();

        // The project tree does NOT scroll on its own — it grows to its
        // content and the *left panel's* single outer scroller (see
        // `app::layout`) scrolls the project tree, the Apple Notes tree and
        // any other source tree together. Horizontal scrolling stays off:
        // wide content clips rather than shifting every row's icons.
        let scrolled_list = ScrolledWindow::builder()
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&file_list)
            .build();

        // Create a box to hold both the label and list with consistent background
        let inner_container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .css_classes(vec!["file-tree-inner"])
            .build();

        inner_container.append(&scrolled_list);

        // Panel header, above the tree itself (the root workspace folder is
        // its own row further down, inside the list)
        let panel_header = GtkBox::new(Orientation::Horizontal, 4);
        panel_header.set_css_classes(&["file-tree-panel-header"]);

        let panel_title = Label::new(Some("Project"));
        panel_title.set_css_classes(&["file-tree-panel-title"]);

        panel_header.append(&panel_title);

        // Create the outer container with margin
        let outer_container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .css_classes(vec!["file-tree-outer"])
            .build();
        outer_container.append(&panel_header);
        outer_container.append(&inner_container);

        let frame = Frame::builder()
            .child(&outer_container)
            .css_classes(vec!["file-tree-container"])
            .build();

        let file_tree = Self {
            frame,
            file_list,
            workspace: Rc::new(RefCell::new(None)),
            root_path: Rc::new(RefCell::new(None)),
            entries: Rc::new(RefCell::new(Vec::new())),
            collapsed: Rc::new(RefCell::new(HashSet::new())),
            clipboard: Rc::new(RefCell::new(None)),
            git_statuses: Rc::new(RefCell::new(HashMap::new())),
            ignored: Rc::new(RefCell::new(HashSet::new())),
            folder_icon: crate::app::icons::paintable("folder"),
            file_icon: crate::app::icons::paintable("file"),
            json_icon: crate::app::icons::paintable("json"),
        };

        // F2/Delete/Cut/Copy/Paste for the selected row (see tree_menu.rs)
        file_tree.setup_keyboard_shortcuts();

        file_tree
    }

    pub fn set_workspace(&mut self, workspace: Rc<Workspace>) {
        self.workspace.replace(Some(workspace.clone()));

        // Note: opening a file is handled by an explicit left-click gesture
        // on the row in build_row(), not by GTK's row-selected signal.
        // row-selected also fires for programmatic `select_row` calls (e.g.
        // highlighting a file after a move) and, apparently, for a
        // right-click — neither of those should open anything.

        // Keep the tree selection in sync with the active editor tab
        let file_tree_for_sync = self.clone();
        workspace
            .notebook
            .connect_switch_page(move |_, _, page_num| {
                if let Some(workspace) = file_tree_for_sync.workspace.borrow().as_ref()
                    && let Some(path) = workspace
                        .open_files
                        .borrow()
                        .get(page_num as usize)
                        .cloned()
                {
                    file_tree_for_sync.select_path(&path);
                }
            });
    }

    /// Point the tree at a workspace folder and populate it from disk.
    pub fn set_root_path(&self, path: PathBuf) {
        self.root_path.replace(Some(path));
        self.collapsed.borrow_mut().clear();
        self.clipboard.replace(None);
        self.refresh();
    }

    /// Re-walk the workspace folder and rebuild the displayed rows.
    pub fn refresh(&self) {
        // Clear existing items
        let mut child = self.file_list.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            self.file_list.remove(&widget);
        }
        self.entries.borrow_mut().clear();

        let Some(root) = self.root_path.borrow().clone() else {
            return;
        };

        let (mut git_statuses, ignored) = if root.join(".git").is_dir() {
            let git = crate::git::ops::GitController::new(&root);
            (git.file_statuses(), git.ignored_paths())
        } else {
            (HashMap::new(), HashSet::new())
        };
        self.ignored.replace(ignored);

        // Propagate each file's status up to every ancestor folder (and the
        // root itself), keeping the highest-priority one where several
        // descendants disagree — same categories/colors as files.
        if !git_statuses.is_empty() {
            let leaf_statuses: Vec<(PathBuf, GitFileStatus)> =
                git_statuses.iter().map(|(p, s)| (p.clone(), *s)).collect();
            for (path, status) in leaf_statuses {
                let mut dir = path.parent();
                while let Some(current) = dir {
                    let entry = git_statuses.entry(current.to_path_buf()).or_insert(status);
                    if status_priority(status) > status_priority(*entry) {
                        *entry = status;
                    }
                    if current == root {
                        break;
                    }
                    dir = current.parent();
                }
            }
        }

        self.git_statuses.replace(git_statuses);

        // The workspace root itself is the first, always-visible row; its
        // children (the real entries) collapse/expand underneath it.
        let root_row = self.build_row(&root, true, 0, true);
        self.file_list.append(&root_row);
        self.entries.borrow_mut().push((root.clone(), true));

        let collapsed = self.collapsed.borrow();
        if !collapsed.contains(&root) {
            let mut collected = Vec::new();
            collect_entries(&root, 1, &collapsed, &mut collected);

            for (path, is_dir, depth) in collected {
                let row = self.build_row(&path, is_dir, depth, false);
                self.file_list.append(&row);
                self.entries.borrow_mut().push((path, is_dir));
            }
        }
    }

    /// Whether `path` is git-ignored, directly or via an ignored ancestor
    /// directory (git reports one entry per ignored dir, not its contents).
    fn is_ignored(&self, path: &Path) -> bool {
        let ignored = self.ignored.borrow();
        if ignored.is_empty() {
            return false;
        }
        let root = self.root_path.borrow().clone();
        let mut cur = Some(path);
        while let Some(p) = cur {
            if ignored.contains(p) {
                return true;
            }
            if root.as_deref() == Some(p) {
                break;
            }
            cur = p.parent();
        }
        false
    }

    /// Highlight the row backing `path`, if it is currently shown.
    pub fn select_path(&self, path: &Path) {
        // Extract into an owned value first: `select_row` below fires the
        // `row-selected` signal synchronously, whose handler can re-enter
        // `refresh()` (e.g. selecting a freshly moved, not-yet-open file
        // triggers `open_path` -> `add_new_tab` -> `refresh`). Holding this
        // borrow across that call (as an `if let` scrutinee would, via
        // temporary lifetime extension) panics with "already borrowed".
        let index = self.entries.borrow().iter().position(|(p, _)| p == path);
        if let Some(index) = index
            && let Some(row) = self.file_list.row_at_index(index as i32)
        {
            self.file_list.select_row(Some(&row));
        }
    }

    pub fn get_widget(&self) -> &Frame {
        &self.frame
    }

    pub(crate) fn build_row(
        &self,
        path: &Path,
        is_dir: bool,
        depth: usize,
        is_root: bool,
    ) -> ListBoxRow {
        let hbox = GtkBox::new(Orientation::Horizontal, 0);
        hbox.set_valign(Align::Center);
        hbox.set_css_classes(&["file-list-row"]);
        hbox.set_margin_start(INDENT_PX * depth as i32);
        if is_root {
            hbox.add_css_class("file-tree-header");
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Workspace")
            .to_string();

        // Folders pick up the highest-priority status of anything changed
        // underneath them (see refresh()), same categories/colors as files.
        let git_status = self.git_statuses.borrow().get(path).copied();

        let name_label = if is_dir {
            // Expand/collapse indicator, separate from the folder icon itself
            let expanded = !self.collapsed.borrow().contains(path);
            let glyph = if expanded { "\u{25BE}" } else { "\u{25B8}" };
            let chevron = Label::new(Some(glyph));
            chevron.set_css_classes(&["dir-chevron"]);
            hbox.append(&chevron);

            let icon = Image::from_paintable(self.folder_icon.as_ref());
            icon.set_pixel_size(16);
            icon.set_css_classes(&["file-icon"]);
            hbox.append(&icon);

            let label = Label::new(Some(&name));
            label.set_css_classes(&["dir-label"]);
            match git_status {
                Some(GitFileStatus::New) => label.add_css_class("file-new"),
                Some(GitFileStatus::Renamed) => label.add_css_class("file-renamed"),
                Some(GitFileStatus::Modified) => label.add_css_class("file-modified"),
                None => {}
            }
            hbox.append(&label);
            label
        } else {
            // Files have no expand/collapse chevron, but the currently open
            // file gets one anyway (as an "active" indicator) so its slot
            // stays reserved either way and icons line up in a column with
            // their sibling folders' icons.
            let is_open = self
                .workspace
                .borrow()
                .as_ref()
                .map(|w| w.open_files.borrow().iter().any(|p| p == path))
                .unwrap_or(false);

            // Open-file dot: a solid circle, white when the file is merely
            // open, otherwise following the file's VCS colour (orange
            // modified, etc.) — same classes as the name label below.
            let chevron = Label::new(if is_open { Some("\u{25CF}") } else { None });
            let mut chevron_classes = vec!["dir-chevron"];
            if is_open {
                chevron_classes.push("active-chevron");
                match git_status {
                    Some(GitFileStatus::New) => chevron_classes.push("file-new"),
                    Some(GitFileStatus::Renamed) => chevron_classes.push("file-renamed"),
                    Some(GitFileStatus::Modified) => chevron_classes.push("file-modified"),
                    None => {}
                }
            }
            chevron.set_css_classes(&chevron_classes);
            hbox.append(&chevron);

            // Per-extension leaf icon (just JSON for now — the `.rhymr/`
            // config/cache files); everything else is the generic file glyph.
            let leaf_icon = if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
            {
                self.json_icon.as_ref()
            } else {
                self.file_icon.as_ref()
            };
            let icon = Image::from_paintable(leaf_icon);
            icon.set_pixel_size(16);
            icon.set_css_classes(&["file-icon"]);
            hbox.append(&icon);

            // .txt is the default/implied extension for a lyrics workspace,
            // so hide it in the tree — commit_rename() adds it back
            // automatically if the user doesn't type their own extension.
            let label = Label::new(Some(strip_txt_extension(&name)));
            match git_status {
                Some(GitFileStatus::New) => label.add_css_class("file-new"),
                Some(GitFileStatus::Renamed) => label.add_css_class("file-renamed"),
                Some(GitFileStatus::Modified) => label.add_css_class("file-modified"),
                None => {}
            }
            hbox.append(&label);
            label
        };

        // Git-ignored entries (and anything under an ignored dir) go
        // goldenrod, JetBrains-style — but not the workspace root itself.
        if !is_root && self.is_ignored(path) {
            name_label.add_css_class("file-ignored");
        }

        if is_root && let Some(path_str) = path.to_str() {
            let path_label = Label::new(Some(path_str));
            path_label.set_css_classes(&["dir-path"]);
            path_label.set_hexpand(true);
            path_label.set_halign(Align::End);
            path_label.set_ellipsize(pango::EllipsizeMode::Start);
            hbox.append(&path_label);
        }

        // Left click on a directory (including the root) toggles it collapsed/expanded
        if is_dir {
            let toggle_click = GestureClick::new();
            toggle_click.set_button(1);
            let file_tree_ref = self.clone();
            let path_owned = path.to_path_buf();
            toggle_click.connect_released(move |_, _, _, _| {
                let mut collapsed = file_tree_ref.collapsed.borrow_mut();
                if !collapsed.remove(&path_owned) {
                    collapsed.insert(path_owned.clone());
                }
                drop(collapsed);
                file_tree_ref.refresh();
            });
            hbox.add_controller(toggle_click);
        } else {
            // Left click on a file opens it. This is deliberately its own
            // gesture rather than piggybacking on the ListBox's
            // `row-selected` signal — that signal also fires for
            // programmatic `select_row` calls (e.g. highlighting a file
            // after a drag/paste move) and, apparently, on right-click too,
            // both of which would otherwise open the file as a side effect.
            let open_click = GestureClick::new();
            open_click.set_button(1);
            let file_tree_ref = self.clone();
            let path_owned = path.to_path_buf();
            open_click.connect_released(move |_, _, _, _| {
                if let Some(workspace) = file_tree_ref.workspace.borrow().as_ref() {
                    workspace.open_path(path_owned.clone());
                }
            });
            hbox.add_controller(open_click);
        }

        // Right click opens a full context menu (New is offered everywhere,
        // including the workspace root; Cut/Copy/Rename/Delete are not,
        // since they don't make sense for the whole open workspace).
        let context_click = GestureClick::new();
        context_click.set_button(3);
        let file_tree_ref = self.clone();
        let path_owned = path.to_path_buf();
        let hbox_ref = hbox.clone();
        let label_ref = name_label.clone();
        context_click.connect_pressed(move |_, _, _, _| {
            file_tree_ref.show_context_menu(
                &hbox_ref,
                &label_ref,
                path_owned.clone(),
                is_dir,
                is_root,
            );
        });
        hbox.add_controller(context_click);

        // Drag to move files/folders into a different folder (not the
        // workspace root itself — there's nowhere else to drop that).
        if !is_root {
            let drag_source = DragSource::new();
            drag_source.set_actions(gdk::DragAction::MOVE);
            let path_for_drag = path.to_path_buf();
            let hbox_for_drag = hbox.clone();
            drag_source.connect_prepare(move |_, _, _| {
                let value = path_for_drag.to_string_lossy().to_string().to_value();
                Some(gdk::ContentProvider::for_value(&value))
            });
            // Dim the row while it's being dragged, so it's clear what's moving
            drag_source.connect_drag_begin(move |_, _| {
                hbox_for_drag.add_css_class("dragging");
            });
            let hbox_for_drag_end = hbox.clone();
            drag_source.connect_drag_end(move |_, _, _| {
                hbox_for_drag_end.remove_css_class("dragging");
            });
            hbox.add_controller(drag_source);
        }

        // Directories (including the root) accept drops to move items into them
        if is_dir {
            let drop_target =
                DropTarget::new(gtk::glib::types::Type::STRING, gdk::DragAction::MOVE);

            // Highlight the folder currently under the pointer during a drag,
            // so it's obvious where a drop will land.
            let hbox_for_enter = hbox.clone();
            drop_target.connect_enter(move |_, _, _| {
                hbox_for_enter.add_css_class("drop-target-active");
                gdk::DragAction::MOVE
            });
            let hbox_for_leave = hbox.clone();
            drop_target.connect_leave(move |_| {
                hbox_for_leave.remove_css_class("drop-target-active");
            });

            let file_tree_ref = self.clone();
            let target_dir = path.to_path_buf();
            let hbox_for_drop = hbox.clone();
            drop_target.connect_drop(move |_, value, _, _| {
                hbox_for_drop.remove_css_class("drop-target-active");
                if let Ok(src) = value.get::<String>() {
                    file_tree_ref.move_into(PathBuf::from(src), target_dir.clone());
                    true
                } else {
                    false
                }
            });
            hbox.add_controller(drop_target);
        }

        let row = ListBoxRow::new();
        row.set_selectable(!is_dir);
        row.set_child(Some(&hbox));
        row
    }
}

/// Strip a trailing ".txt" (case-insensitively) — the implied default
/// extension in a plain-text lyrics workspace, hidden everywhere a file
/// name is shown or edited.
pub(crate) fn strip_txt_extension(name: &str) -> &str {
    if name.len() > 4 && name[name.len() - 4..].eq_ignore_ascii_case(".txt") {
        &name[..name.len() - 4]
    } else {
        name
    }
}

/// Higher wins when a folder's descendants disagree on status.
fn status_priority(status: GitFileStatus) -> u8 {
    match status {
        GitFileStatus::New => 3,
        GitFileStatus::Renamed => 2,
        GitFileStatus::Modified => 1,
    }
}

fn collect_entries(
    dir: &Path,
    depth: usize,
    collapsed: &HashSet<PathBuf>,
    entries: &mut Vec<(PathBuf, bool, usize)>,
) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    let mut items: Vec<_> = read_dir.filter_map(|entry| entry.ok()).collect();
    items.sort_by(|a, b| {
        let a_is_dir = a.path().is_dir();
        let b_is_dir = b.path().is_dir();
        match (a_is_dir, b_is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => a
                .file_name()
                .to_ascii_lowercase()
                .cmp(&b.file_name().to_ascii_lowercase()),
        }
    });

    for item in items {
        let name = item.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') || IGNORED_ENTRIES.contains(&name_str.as_ref()) {
            continue;
        }

        let path = item.path();
        let is_dir = path.is_dir();
        entries.push((path.clone(), is_dir, depth));

        if is_dir && !collapsed.contains(&path) {
            collect_entries(&path, depth + 1, collapsed, entries);
        }
    }
}
