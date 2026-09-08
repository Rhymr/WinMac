use crate::file::ops::FileOps;
use crate::workspace::Workspace;
use gtk::Window;
use gtk::prelude::*;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

/// Boxed status-bar callbacks — factored out purely to keep the field
/// declarations under clippy's type-complexity threshold.
type WordCountListener = RefCell<Option<Box<dyn Fn(u32)>>>;
type CursorListener = RefCell<Option<Box<dyn Fn(i32, i32)>>>;
type BranchListener = RefCell<Option<Box<dyn Fn(Option<String>)>>>;
type NavListener = RefCell<Option<Box<dyn Fn(Vec<String>)>>>;
type GitListener = RefCell<Option<Box<dyn Fn(GitAvailability)>>>;
type RootListener = RefCell<Option<Box<dyn Fn(Option<PathBuf>)>>>;

/// What git actions the loaded workspace supports — drives the toolbar's
/// Git group (see `crate::app::chrome::main_toolbar`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GitAvailability {
    /// Not a git repository — no git actions.
    None,
    /// A repository with no `origin` remote — local actions only (commit).
    LocalOnly,
    /// A repository with an `origin` remote — every git action.
    Full,
}

pub struct WorkspaceController {
    pub(crate) workspace: RefCell<Option<Rc<Workspace>>>,
    root_path: RefCell<Option<PathBuf>>,
    word_count_listener: WordCountListener,
    cursor_listener: CursorListener,
    branch_listener: BranchListener,
    nav_listener: NavListener,
    git_listener: GitListener,
    root_listener: RootListener,
}

impl Default for WorkspaceController {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceController {
    pub fn new() -> Self {
        Self {
            workspace: RefCell::new(None),
            root_path: RefCell::new(None),
            word_count_listener: RefCell::new(None),
            cursor_listener: RefCell::new(None),
            branch_listener: RefCell::new(None),
            nav_listener: RefCell::new(None),
            git_listener: RefCell::new(None),
            root_listener: RefCell::new(None),
        }
    }

    /// Subscribe to the active tab's word count — called whenever the
    /// active tab switches or its text changes.
    pub fn set_word_count_listener(&self, listener: impl Fn(u32) + 'static) {
        self.word_count_listener.replace(Some(Box::new(listener)));
    }

    /// Recompute the active tab's word count and notify the listener.
    pub fn refresh_word_count(&self) {
        let count = self
            .get_workspace()
            .and_then(|workspace| workspace.get_current_buffer())
            .map(|(buffer, _)| {
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                crate::editor::stat::count_words(&text)
            })
            .unwrap_or(0);

        if let Some(listener) = self.word_count_listener.borrow().as_ref() {
            listener(count);
        }
    }

    /// Subscribe to the active tab's caret position (1-based line, column).
    pub fn set_cursor_listener(&self, listener: impl Fn(i32, i32) + 'static) {
        self.cursor_listener.replace(Some(Box::new(listener)));
    }

    /// Recompute the active tab's caret position and notify the listener.
    pub fn refresh_cursor(&self) {
        let (line, col) = self
            .get_workspace()
            .and_then(|workspace| workspace.get_current_buffer())
            .map(|(buffer, _)| {
                let iter = buffer.iter_at_offset(buffer.cursor_position());
                (iter.line() + 1, iter.line_offset() + 1)
            })
            .unwrap_or((1, 1));

        if let Some(listener) = self.cursor_listener.borrow().as_ref() {
            listener(line, col);
        }
    }

    /// Subscribe to the workspace's current git branch (`None` = detached /
    /// not a repo).
    pub fn set_branch_listener(&self, listener: impl Fn(Option<String>) + 'static) {
        self.branch_listener.replace(Some(Box::new(listener)));
    }

    /// Re-read the current branch name and notify the listener.
    pub fn refresh_branch(&self) {
        let branch = self.root_path.borrow().as_ref().and_then(|root| {
            if root.join(".git").is_dir() {
                crate::git::ops::GitController::new(root).current_branch_name()
            } else {
                None
            }
        });

        if let Some(listener) = self.branch_listener.borrow().as_ref() {
            listener(branch);
        }
    }

    /// Subscribe to whether the workspace supports git actions — notified
    /// on every `set_root_path`.
    pub fn set_git_listener(&self, listener: impl Fn(GitAvailability) + 'static) {
        self.git_listener.replace(Some(Box::new(listener)));
    }

    /// Re-derive git availability from the workspace root and notify the
    /// listener.
    pub fn refresh_git_availability(&self) {
        let availability = match self.root_path.borrow().as_ref() {
            Some(root) if root.join(".git").is_dir() => {
                if crate::git::ops::GitController::new(root).has_remote("origin") {
                    GitAvailability::Full
                } else {
                    GitAvailability::LocalOnly
                }
            }
            _ => GitAvailability::None,
        };

        if let Some(listener) = self.git_listener.borrow().as_ref() {
            listener(availability);
        }
    }

    /// Subscribe to the active tab's location as breadcrumb segments
    /// (`["src", "app", "main.rs"]`) — empty when no file is open.
    pub fn set_nav_listener(&self, listener: impl Fn(Vec<String>) + 'static) {
        self.nav_listener.replace(Some(Box::new(listener)));
    }

    /// Recompute the active tab's breadcrumb and notify the listener. The
    /// workspace folder is always the first segment (when a workspace is
    /// loaded); the rest are the active file's path relative to it.
    pub fn refresh_nav(&self) {
        let mut segments: Vec<String> = self
            .root_path
            .borrow()
            .as_ref()
            .and_then(|root| root.file_name().map(|n| n.to_string_lossy().into_owned()))
            .into_iter()
            .collect();

        if let Some(path) = self
            .get_workspace()
            .and_then(|w| w.get_current_buffer())
            .and_then(|(_, path)| path)
        {
            // A file outside the workspace (an opened external file, or a
            // read-only source document staged to a temp file) shows just
            // its name, not its whole absolute path.
            let rel = self
                .root_path
                .borrow()
                .as_ref()
                .and_then(|root| path.strip_prefix(root).ok().map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from(path.file_name().unwrap_or(path.as_os_str())));
            let mut segs: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            // Match the tab label — hide the implied `.txt`.
            if let Some(last) = segs.last_mut()
                && let Some(stripped) = last.strip_suffix(".txt")
            {
                *last = stripped.to_string();
            }
            segments.extend(segs);
        }

        if let Some(listener) = self.nav_listener.borrow().as_ref() {
            listener(segments);
        }
    }

    /// Live-apply `settings` to every open tab, if a workspace is loaded.
    pub fn apply_settings(&self, settings: &crate::setting::Settings) {
        if let Some(workspace) = self.get_workspace() {
            workspace.apply_settings_to_open_tabs(settings);
        }
    }

    pub fn set_workspace(&self, workspace: Rc<Workspace>) {
        // Store the controller for use in tab operations
        self.workspace.replace(Some(workspace));
    }

    pub fn get_workspace(&self) -> Option<Rc<Workspace>> {
        self.workspace.borrow().clone()
    }

    /// Subscribe to the loaded workspace folder — notified on every
    /// `set_root_path`. The external-sources panel uses this to point its
    /// per-workspace cache at the new project's `.rhymr/`.
    pub fn set_root_listener(&self, listener: impl Fn(Option<PathBuf>) + 'static) {
        self.root_listener.replace(Some(Box::new(listener)));
    }

    /// Point the file tree at the loaded workspace folder.
    pub fn set_root_path(&self, path: PathBuf) {
        self.root_path.replace(Some(path.clone()));
        if let Some(workspace) = self.get_workspace()
            && let Some(ref file_tree) = workspace.file_tree
        {
            file_tree.set_root_path(path.clone());
        }
        self.refresh_branch();
        self.refresh_git_availability();
        if let Some(listener) = self.root_listener.borrow().as_ref() {
            listener(Some(path));
        }
    }

    pub fn get_root_path(&self) -> Option<PathBuf> {
        self.root_path.borrow().clone()
    }

    pub fn handle_new_file(&self) {
        let Some(workspace) = self.get_workspace() else {
            return;
        };
        let Some(root) = self.get_root_path() else {
            return;
        };

        // Create the file directly on disk in the workspace root — same
        // de-duplicated naming as the file tree's own "New File" — instead
        // of a disconnected "Untitled" tab that isn't a real file (and
        // doesn't show up in the tree) until an eventual Save As.
        let mut candidate = root.join("Untitled.txt");
        let mut n = 1;
        while candidate.exists() {
            n += 1;
            candidate = root.join(format!("Untitled {n}.txt"));
        }

        if let Err(e) = fs::write(&candidate, "") {
            eprintln!("Failed to create {candidate:?}: {e}");
            return;
        }

        workspace.add_new_tab(&candidate, "");
        if let Some(ref file_tree) = workspace.file_tree {
            file_tree.refresh();
            file_tree.select_path(&candidate);
        }
        self.stage_git(&root);
    }

    pub fn handle_new_folder(&self) {
        let Some(workspace) = self.get_workspace() else {
            return;
        };
        let Some(root) = self.get_root_path() else {
            return;
        };

        let mut candidate = root.join("New Folder");
        let mut n = 1;
        while candidate.exists() {
            n += 1;
            candidate = root.join(format!("New Folder {n}"));
        }

        if let Err(e) = fs::create_dir(&candidate) {
            eprintln!("Failed to create {candidate:?}: {e}");
            return;
        }

        if let Some(ref file_tree) = workspace.file_tree {
            file_tree.refresh();
            file_tree.select_path(&candidate);
        }
        self.stage_git(&root);
    }

    fn stage_git(&self, root: &std::path::Path) {
        if crate::setting::Settings::load().git_autostage {
            crate::git::ops::stage_all_changes(root);
        }
    }

    /// Close every open tab and re-point the workspace at a different
    /// folder — used by the Recent Projects menu.
    pub fn switch_workspace(&self, path: PathBuf) {
        if let Some(workspace) = self.get_workspace() {
            while !workspace.open_files.borrow().is_empty() {
                workspace.remove_tab(0);
            }
        }
        crate::workspace::recent::record_recent_workspace(&path);
        self.set_root_path(path);
    }

    pub fn handle_open_file(&self, window: &Window) {
        if let Some((path, content)) = FileOps::open_file(Some(window.clone()))
            && let Some(workspace) = self.workspace.borrow().as_ref()
        {
            workspace.add_new_tab(&path, &content);
        }
    }

    pub fn handle_save_file(&self, window: &Window) {
        let Some(workspace) = self.get_workspace() else {
            return;
        };
        let Some((buffer, path)) = workspace.get_current_buffer() else {
            return;
        };

        let content = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        let saved_path = if let Some(existing_path) = path {
            let ok = FileOps::save_file(
                content.to_string(),
                Some(existing_path.clone()),
                Some(window.clone()),
            )
            .is_some();
            ok.then_some(existing_path)
        } else if let Some(new_path) =
            FileOps::save_file(content.to_string(), None, Some(window.clone()))
        {
            workspace.update_current_tab_path(new_path.clone());
            Some(new_path)
        } else {
            None
        };

        // A plain content edit+save doesn't rename/create/delete anything,
        // so nothing else would otherwise ever refresh the tree — without
        // this, a file's git-modified color only ever reflected whatever
        // status was true the last time some other operation refreshed it.
        if let Some(root) = self.get_root_path() {
            self.stage_git(&root);
        }
        if let Some(ref file_tree) = workspace.file_tree {
            file_tree.refresh();
            if let Some(path) = saved_path {
                file_tree.select_path(&path);
            }
        }
    }

    pub fn handle_save_as_file(&self, window: &Window) {
        let Some(workspace) = self.get_workspace() else {
            return;
        };
        let Some((buffer, _)) = workspace.get_current_buffer() else {
            return;
        };

        let content = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        let saved_path = FileOps::save_file(content.to_string(), None, Some(window.clone()));
        if let Some(ref new_path) = saved_path {
            workspace.update_current_tab_path(new_path.clone());
        }

        if let Some(root) = self.get_root_path() {
            self.stage_git(&root);
        }
        if let Some(ref file_tree) = workspace.file_tree {
            file_tree.refresh();
            if let Some(path) = saved_path {
                file_tree.select_path(&path);
            }
        }
    }

    pub fn handle_close_tab(&self, _window: &Window) {
        if let Some(workspace) = self.get_workspace()
            && let Some(current_page) = workspace.notebook.current_page()
        {
            workspace.remove_tab(current_page as usize);
        }
    }
}
