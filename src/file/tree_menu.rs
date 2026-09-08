//! Context menus, keyboard shortcuts, and every file-mutating operation
//! (New/Cut/Copy/Paste/Rename/Delete/drag-move) for the file tree. Kept
//! separate from tree.rs (which owns the tree's rendering/data model) since
//! this is the half of `FileTree` that keeps growing.
use super::tree::FileTree;
use crate::app::context_menu::{ContextMenu, hint};
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, Entry, EventControllerFocus, EventControllerKey, Label,
    Orientation, Window,
};
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

// GTK's own "<Primary>" accelerator alias only applies to app-level actions
// (see ui/menu.rs); for a raw EventControllerKey we have to check the
// platform's actual primary modifier ourselves: Cmd on macOS, Ctrl elsewhere.
#[cfg(target_os = "macos")]
const PRIMARY_MASK: gdk::ModifierType = gdk::ModifierType::META_MASK;
#[cfg(not(target_os = "macos"))]
const PRIMARY_MASK: gdk::ModifierType = gdk::ModifierType::CONTROL_MASK;

impl FileTree {
    /// F2 to rename, Delete/Backspace to delete, and Cut/Copy/Paste — all
    /// acting on whichever file is currently selected (only files are
    /// selectable; folders use the context menu). Platform-appropriate
    /// modifier via `PRIMARY_MASK`.
    pub(crate) fn setup_keyboard_shortcuts(&self) {
        let file_tree_ref = self.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, keyval, _, state| {
            let Some(row) = file_tree_ref.file_list.selected_row() else {
                return glib::Propagation::Proceed;
            };
            let index = row.index();
            let Some((path, _)) = file_tree_ref.entries.borrow().get(index as usize).cloned()
            else {
                return glib::Propagation::Proceed;
            };
            let Some(hbox) = row.child().and_then(|w| w.downcast::<GtkBox>().ok()) else {
                return glib::Propagation::Proceed;
            };
            let Some(label) = hbox.last_child().and_then(|w| w.downcast::<Label>().ok()) else {
                return glib::Propagation::Proceed;
            };

            let primary = state.contains(PRIMARY_MASK);

            match keyval {
                gdk::Key::F2 => {
                    file_tree_ref.start_rename(&hbox, &label, path);
                    glib::Propagation::Stop
                }
                // BackSpace covers the Mac keyboard's only "delete" key;
                // Delete covers Windows/Linux (and Mac's forward-delete).
                gdk::Key::Delete | gdk::Key::BackSpace if !primary => {
                    file_tree_ref.confirm_delete(&hbox, path, false);
                    glib::Propagation::Stop
                }
                gdk::Key::c | gdk::Key::C if primary => {
                    file_tree_ref.clipboard.replace(Some((path, false)));
                    glib::Propagation::Stop
                }
                gdk::Key::x | gdk::Key::X if primary => {
                    file_tree_ref.clipboard.replace(Some((path, true)));
                    glib::Propagation::Stop
                }
                gdk::Key::v | gdk::Key::V if primary => {
                    let parent_dir = path.parent().map(Path::to_path_buf).unwrap_or(path);
                    file_tree_ref.paste_into(parent_dir);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.file_list.add_controller(key_controller);
    }

    pub(crate) fn show_context_menu(
        &self,
        hbox: &GtkBox,
        label: &Label,
        path: PathBuf,
        is_dir: bool,
        is_root: bool,
    ) {
        // Parented to the tree's outer `frame` rather than `hbox` itself —
        // see `ContextMenu::new`'s doc comment for why.
        let menu = ContextMenu::new(&self.frame);

        let target_dir = if is_dir {
            path.clone()
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| path.clone())
        };

        // New (opens a File/Folder submenu)
        let menu_ref = menu.clone();
        let file_tree_ref = self.clone();
        let hbox_ref = hbox.clone();
        let target_dir_for_new = target_dir.clone();
        menu.add_submenu_item(Some("new-file"), "New", move || {
            menu_ref.popdown();
            file_tree_ref.show_new_submenu(&hbox_ref, target_dir_for_new.clone());
        });

        if !is_root {
            menu.add_separator();

            let clipboard_ref = self.clipboard.clone();
            let path_for_cut = path.clone();
            menu.add_item(None, "Cut", Some(hint::CUT), None, move || {
                clipboard_ref.replace(Some((path_for_cut.clone(), true)));
            });

            let clipboard_ref = self.clipboard.clone();
            let path_for_copy = path.clone();
            menu.add_item(Some("copy"), "Copy", Some(hint::COPY), None, move || {
                clipboard_ref.replace(Some((path_for_copy.clone(), false)));
            });

            let path_for_copy_path = path.clone();
            menu.add_item(
                Some("copy"),
                "Copy Path",
                Some(hint::COPY_PATH),
                None,
                move || {
                    if let Some(display) = gdk::Display::default() {
                        display
                            .clipboard()
                            .set_text(&path_for_copy_path.to_string_lossy());
                    }
                },
            );
        }

        // Available for files too, not just folders — pastes as a sibling,
        // into the file's parent (target_dir already resolves that way).
        if self.clipboard.borrow().is_some() {
            let file_tree_ref = self.clone();
            let target_dir_for_paste = target_dir.clone();
            menu.add_item(Some("paste"), "Paste", Some(hint::PASTE), None, move || {
                file_tree_ref.paste_into(target_dir_for_paste.clone());
            });
        }

        if !is_root {
            // No popdown()-then-refresh() race here: renaming edits the
            // row's existing children in place rather than destroying the
            // row, so the popover's parent widget stays alive.
            let file_tree_ref = self.clone();
            let hbox_ref = hbox.clone();
            let label_ref = label.clone();
            let path_for_rename = path.clone();
            menu.add_item(Some("rename"), "Rename", None, None, move || {
                file_tree_ref.start_rename(&hbox_ref, &label_ref, path_for_rename.clone());
            });

            menu.add_separator();

            let file_tree_ref = self.clone();
            let hbox_ref = hbox.clone();
            let path_for_delete = path.clone();
            menu.add_item(
                Some("delete"),
                "Delete\u{2026}",
                Some(hint::DELETE),
                Some("destructive-menu-item"),
                move || {
                    file_tree_ref.confirm_delete(&hbox_ref, path_for_delete.clone(), is_dir);
                },
            );
        }

        menu.popup_below(hbox);
    }

    fn show_new_submenu(&self, hbox: &GtkBox, parent_dir: PathBuf) {
        let menu = ContextMenu::new(&self.frame);

        let file_tree_ref = self.clone();
        let dir_for_file = parent_dir.clone();
        menu.add_item(Some("new-file"), "New File", None, None, move || {
            file_tree_ref.create_new_entry(dir_for_file.clone(), false);
        });

        let file_tree_ref = self.clone();
        menu.add_item(Some("new-folder"), "New Folder", None, None, move || {
            file_tree_ref.create_new_entry(parent_dir.clone(), true);
        });

        menu.popup_below(hbox);
    }

    /// Create a new file or folder inside `parent_dir` with a default,
    /// de-duplicated name, then immediately start renaming it in place so
    /// the user can type the real name right away.
    fn create_new_entry(&self, parent_dir: PathBuf, is_dir: bool) {
        let mut candidate = parent_dir.join(if is_dir { "New Folder" } else { "Untitled.txt" });
        let mut n = 1;
        while candidate.exists() {
            n += 1;
            candidate = parent_dir.join(if is_dir {
                format!("New Folder {n}")
            } else {
                format!("Untitled {n}.txt")
            });
        }

        let result = if is_dir {
            fs::create_dir(&candidate)
        } else {
            fs::write(&candidate, "")
        };
        if let Err(e) = result {
            eprintln!("Failed to create {candidate:?}: {e}");
            return;
        }

        self.collapsed.borrow_mut().remove(&parent_dir);
        self.stage_git();

        // Deferred: this handler fires from a Popover menu-item click, and
        // refresh() destroys the row that popover is parented to — doing
        // that synchronously, before the popover has finished closing,
        // crashes GTK. Let the close finish first, then rebuild and start
        // the rename.
        let file_tree_ref = self.clone();
        glib::idle_add_local_once(move || {
            file_tree_ref.refresh();
            file_tree_ref.select_path(&candidate);
            file_tree_ref.begin_rename_at(&candidate);
        });
    }

    /// Locate the currently displayed row for `path` and start inline
    /// renaming on it, reusing the same flow as the context menu's Rename.
    fn begin_rename_at(&self, path: &Path) {
        let Some(index) = self.entries.borrow().iter().position(|(p, _)| p == path) else {
            return;
        };
        let Some(row) = self.file_list.row_at_index(index as i32) else {
            return;
        };
        let Some(hbox) = row.child().and_then(|w| w.downcast::<GtkBox>().ok()) else {
            return;
        };
        let Some(label) = hbox.last_child().and_then(|w| w.downcast::<Label>().ok()) else {
            return;
        };
        self.start_rename(&hbox, &label, path.to_path_buf());
    }

    fn start_rename(&self, hbox: &GtkBox, label: &Label, path: PathBuf) {
        let current_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        // Same .txt-hiding as the tree display — commit_rename() restores
        // it if the user doesn't type a different extension.
        let prefill = if path.is_dir() {
            current_name
        } else {
            super::tree::strip_txt_extension(&current_name).to_string()
        };

        let entry = Entry::new();
        entry.set_text(&prefill);
        entry.set_hexpand(true);
        entry.set_css_classes(&["rename-entry"]);

        hbox.insert_child_after(&entry, Some(label));
        hbox.remove(label);

        entry.grab_focus();
        entry.select_region(0, -1);

        // Guards against committing twice (e.g. Enter immediately followed by focus-out)
        let committed = Rc::new(Cell::new(false));

        let file_tree_ref = self.clone();
        let path_ref = path.clone();
        let committed_ref = committed.clone();
        entry.connect_activate(move |entry| {
            if committed_ref.replace(true) {
                return;
            }
            file_tree_ref.commit_rename(path_ref.clone(), entry.text().to_string());
        });

        let file_tree_ref = self.clone();
        let path_ref = path.clone();
        let committed_ref = committed.clone();
        let focus_controller = EventControllerFocus::new();
        focus_controller.connect_leave(move |controller| {
            if committed_ref.replace(true) {
                return;
            }
            if let Some(entry) = controller.widget().and_downcast::<Entry>() {
                file_tree_ref.commit_rename(path_ref.clone(), entry.text().to_string());
            }
        });
        entry.add_controller(focus_controller);

        let file_tree_ref = self.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gdk::Key::Escape {
                committed.set(true);
                file_tree_ref.refresh();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        entry.add_controller(key_controller);
    }

    fn commit_rename(&self, old_path: PathBuf, new_name: String) {
        let new_name = new_name.trim();
        if !new_name.is_empty()
            && !new_name.contains('/')
            && !new_name.contains('\\')
            && let Some(parent) = old_path.parent()
        {
            // No extension typed for a file → assume .txt, matching what
            // the tree display and rename prefill both hide.
            let final_name = if !old_path.is_dir() && !new_name.contains('.') {
                format!("{new_name}.txt")
            } else {
                new_name.to_string()
            };
            let new_path = parent.join(&final_name);
            if new_path != old_path {
                match fs::rename(&old_path, &new_path) {
                    Ok(()) => {
                        if let Some(workspace) = self.workspace.borrow().as_ref() {
                            workspace.rename_path(&old_path, &new_path);
                        }

                        let mut collapsed = self.collapsed.borrow_mut();
                        if collapsed.remove(&old_path) {
                            collapsed.insert(new_path);
                        }
                        drop(collapsed);
                        self.stage_git();
                    }
                    Err(e) => eprintln!("Failed to rename {old_path:?} to {new_path:?}: {e}"),
                }
            }
        }

        self.refresh();
    }

    /// Move `src` into `target_dir` — the single move implementation shared
    /// by drag-and-drop and Cut+Paste.
    pub(crate) fn move_into(&self, src: PathBuf, target_dir: PathBuf) {
        if !src.exists() || src.parent() == Some(target_dir.as_path()) {
            return;
        }
        // Refuse to move a directory into itself or one of its own descendants
        if target_dir == src || target_dir.starts_with(&src) {
            return;
        }
        let Some(name) = src.file_name() else {
            return;
        };
        let dest = target_dir.join(name);
        if dest.exists() {
            return;
        }

        match fs::rename(&src, &dest) {
            Ok(()) => {
                if let Some(workspace) = self.workspace.borrow().as_ref() {
                    workspace.rename_path(&src, &dest);
                }
                self.collapsed.borrow_mut().remove(&target_dir);
                self.stage_git();

                // Deferred for the same reason as create_new_entry: this can
                // run from a drag-and-drop completing on a Popover-adjacent
                // widget or from Paste (a Popover menu-item click), and
                // refresh() would otherwise destroy widgets mid-transition.
                let file_tree_ref = self.clone();
                glib::idle_add_local_once(move || {
                    file_tree_ref.refresh();
                    file_tree_ref.select_path(&dest);
                });
            }
            Err(e) => eprintln!("Failed to move {src:?} to {dest:?}: {e}"),
        }
    }

    fn paste_into(&self, target_dir: PathBuf) {
        let Some((src, is_cut)) = self.clipboard.borrow().clone() else {
            return;
        };
        if !src.exists() {
            return;
        }
        if target_dir == src || target_dir.starts_with(&src) {
            return;
        }

        if is_cut {
            self.clipboard.replace(None);
            self.move_into(src, target_dir);
            return;
        }

        let Some(name) = src.file_name() else {
            return;
        };
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("item");
        let ext = src.extension().and_then(|s| s.to_str());

        let mut dest = target_dir.join(name);
        let mut n = 1;
        while dest.exists() {
            n += 1;
            let candidate_name = match ext {
                Some(ext) => format!("{stem} {n}.{ext}"),
                None => format!("{stem} {n}"),
            };
            dest = target_dir.join(candidate_name);
        }

        match copy_recursive(&src, &dest) {
            Ok(()) => {
                self.collapsed.borrow_mut().remove(&target_dir);
                self.stage_git();

                // Deferred: Paste is invoked from a Popover menu-item click.
                let file_tree_ref = self.clone();
                glib::idle_add_local_once(move || {
                    file_tree_ref.refresh();
                    file_tree_ref.select_path(&dest);
                });
            }
            Err(e) => eprintln!("Failed to copy {src:?} to {dest:?}: {e}"),
        }
    }

    fn confirm_delete(&self, hbox: &GtkBox, path: PathBuf, is_dir: bool) {
        let Some(window) = hbox.root().and_then(|r| r.downcast::<Window>().ok()) else {
            return;
        };
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("this item")
            .to_string();
        let kind = if is_dir { "folder" } else { "file" };

        let dialog = Window::builder()
            .title("Delete")
            .transient_for(&window)
            .modal(true)
            .resizable(false)
            .default_width(360)
            .build();

        let content = GtkBox::new(Orientation::Vertical, 16);
        content.set_margin_top(20);
        content.set_margin_bottom(20);
        content.set_margin_start(20);
        content.set_margin_end(20);

        let message = Label::new(Some(&format!(
            "Delete {kind} \u{201C}{name}\u{201D}? This can\u{2019}t be undone."
        )));
        message.set_wrap(true);
        message.set_halign(Align::Start);

        let action_box = GtkBox::new(Orientation::Horizontal, 12);
        action_box.set_halign(Align::End);
        action_box.set_margin_top(8);

        let cancel_btn = Button::builder().label("Cancel").build();
        let delete_btn = Button::builder()
            .label("Delete")
            .css_classes(vec!["destructive-action"])
            .build();
        action_box.append(&cancel_btn);
        action_box.append(&delete_btn);

        content.append(&message);
        content.append(&action_box);
        dialog.set_child(Some(&content));

        let dialog_for_cancel = dialog.clone();
        cancel_btn.connect_clicked(move |_| {
            dialog_for_cancel.close();
        });

        let dialog_for_delete = dialog.clone();
        let file_tree_ref = self.clone();
        delete_btn.connect_clicked(move |_| {
            dialog_for_delete.close();
            file_tree_ref.delete_path(path.clone());
        });

        dialog.present();
    }

    fn delete_path(&self, path: PathBuf) {
        let result = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match result {
            Ok(()) => {
                if let Some(workspace) = self.workspace.borrow().as_ref() {
                    workspace.close_paths_under(&path);
                }
                self.collapsed.borrow_mut().remove(&path);
                self.stage_git();
                self.refresh_deferred();
            }
            Err(e) => eprintln!("Failed to delete {path:?}: {e}"),
        }
    }

    /// Same as `refresh`, but deferred to the next main-loop iteration.
    /// Needed whenever the call originates from a Popover menu-item click:
    /// the popover is parented to the very row `refresh()` is about to
    /// destroy, and tearing that row down before the popover has finished
    /// its own close transition crashes GTK.
    fn refresh_deferred(&self) {
        let file_tree_ref = self.clone();
        glib::idle_add_local_once(move || {
            file_tree_ref.refresh();
        });
    }

    /// Stage every change in the workspace (a no-op if it isn't a git repo).
    fn stage_git(&self) {
        if let Some(root) = self.root_path.borrow().clone() {
            crate::git::ops::stage_all_changes(&root);
        }
    }
}

fn copy_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dest).map(|_| ())
    }
}
