//! The Git Log tool window — a bottom-docked history panel, JetBrains "Git"
//! style: a paged commit list on the left, the selected commit's detail and
//! changed-file list on the right.
//!
//! All history reads ([`crate::git::ops::GitController::log`] /
//! `commit_detail` / `ref_labels`) run on a worker thread; results come back
//! to the UI over a channel, with a generation counter so a superseded load
//! can't overwrite a newer one. The panel is workspace-independent widget
//! state: `set_repo` re-points it, `refresh` re-reads.

use crate::app::icons::img;
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, DrawingArea, Frame, Label, ListBox, Orientation, PolicyType,
    ScrolledWindow, SelectionMode,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::git::ops::{
    ChangedFile, CommitDetail, CommitSummary, GitController, LogStart, RefLabel,
};

use futures_util::StreamExt;

/// Commits fetched per page — the first page loads on open, more append on
/// scroll to the bottom.
const PAGE: usize = 100;

type RefMap = HashMap<git2::Oid, Vec<RefLabel>>;

/// What a worker load hands back to the UI thread.
enum LoadMsg {
    /// A page of commits. `refs` is only populated (and applied) for the
    /// first page (`skip == 0`).
    Page {
        skip: usize,
        commits: Vec<CommitSummary>,
        refs: RefMap,
    },
    /// Detail for the selected commit.
    Detail(Box<CommitDetail>),
    /// The repo couldn't be read.
    Error(String),
}

/// The Git Log panel. Cheap to clone — every field is an `Rc`/GObject
/// handle, so closures capture clones rather than `self`.
#[derive(Clone)]
pub struct GitLogPanel {
    frame: Frame,
    collapsed: Rc<Cell<bool>>,
    repo_path: Rc<RefCell<Option<PathBuf>>>,
    commit_list: ListBox,
    detail_box: GtkBox,
    /// Trailing action slot in the header — the dock adds a minimise button.
    header_actions: GtkBox,
    commits: Rc<RefCell<Vec<CommitSummary>>>,
    refs: Rc<RefCell<RefMap>>,
    /// Whether a "load more" page request is already in flight.
    loading_more: Rc<Cell<bool>>,
    /// Bumped on every `refresh` so a slow, superseded load is dropped.
    generation: Rc<Cell<u64>>,
}

impl Default for GitLogPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl GitLogPanel {
    pub fn new() -> Self {
        let commit_list = ListBox::builder()
            .css_classes(["git-log-commits"])
            .selection_mode(SelectionMode::Single)
            .build();
        let commit_scroller = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .child(&commit_list)
            .build();

        let detail_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .css_classes(["git-log-detail"])
            .spacing(4)
            .build();
        let detail_scroller = ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .child(&detail_box)
            .build();

        let split = gtk::Paned::new(Orientation::Horizontal);
        split.set_start_child(Some(&commit_scroller));
        split.set_end_child(Some(&detail_scroller));
        split.set_position(460);
        split.set_resize_start_child(true);
        split.set_resize_end_child(true);
        split.set_shrink_start_child(false);
        split.set_shrink_end_child(false);

        let header = GtkBox::new(Orientation::Horizontal, 6);
        header.set_css_classes(&["git-log-header"]);
        let title = Label::new(Some("Git Log"));
        title.set_css_classes(&["git-log-title"]);
        let refresh_btn = Button::builder()
            .css_classes(["flat", "git-log-refresh"])
            .tooltip_text("Refresh")
            .child(&img("git-fetch", 14))
            .build();
        header.append(&title);
        let spacer = GtkBox::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);
        header.append(&refresh_btn);
        // Trailing action slot — the dock injects a minimise button here.
        let header_actions = GtkBox::new(Orientation::Horizontal, 2);
        header.append(&header_actions);

        let frame = Frame::builder()
            .child(&split)
            .css_classes(["git-log-container"])
            .build();
        frame.set_label_widget(Some(&header));

        let panel = Self {
            frame,
            collapsed: Rc::new(Cell::new(true)),
            repo_path: Rc::new(RefCell::new(None)),
            commit_list: commit_list.clone(),
            detail_box,
            header_actions,
            commits: Rc::new(RefCell::new(Vec::new())),
            refs: Rc::new(RefCell::new(HashMap::new())),
            loading_more: Rc::new(Cell::new(false)),
            generation: Rc::new(Cell::new(0)),
        };

        {
            let panel = panel.clone();
            refresh_btn.connect_clicked(move |_| panel.refresh());
        }
        {
            let panel = panel.clone();
            commit_list.connect_row_selected(move |_, row| {
                if let Some(row) = row {
                    panel.show_detail(row.index());
                }
            });
        }
        // Append the next page when the list is scrolled near its end.
        {
            let panel = panel.clone();
            commit_scroller
                .vadjustment()
                .connect_value_changed(move |adj| {
                    if adj.value() + adj.page_size() >= adj.upper() - 1.0 {
                        panel.load_more();
                    }
                });
        }

        panel
    }

    /// The outer widget for the layout to dock.
    pub fn get_widget(&self) -> &Frame {
        &self.frame
    }

    /// The trailing action area of the header, for the dock to drop a
    /// minimise button into.
    pub fn header_actions(&self) -> GtkBox {
        self.header_actions.clone()
    }

    /// Whether the panel body is currently hidden.
    pub fn is_collapsed(&self) -> bool {
        self.collapsed.get()
    }

    /// Show or hide the panel body (its whole frame is toggled by the
    /// bottom stripe; this tracks that state and refreshes on first open).
    pub fn set_expanded(&self, expanded: bool) {
        let was_collapsed = self.collapsed.replace(!expanded);
        if let Some(child) = self.frame.child() {
            child.set_visible(expanded);
        }
        if expanded
            && was_collapsed
            && self.commits.borrow().is_empty()
            && self.repo_root().is_some()
        {
            self.refresh();
        }
    }

    /// Point the panel at a new repository root (the workspace root, or
    /// `None` when no project is open). Clears the view; the next open or
    /// an explicit `refresh` reloads.
    pub fn set_repo(&self, root: Option<PathBuf>) {
        let changed = *self.repo_path.borrow() != root;
        *self.repo_path.borrow_mut() = root;
        if changed {
            self.commits.borrow_mut().clear();
            self.refs.borrow_mut().clear();
            clear_list(&self.commit_list);
            clear_box(&self.detail_box);
            if !self.collapsed.get() {
                self.refresh();
            }
        }
    }

    /// Whether `repo_path` currently names a git repository.
    fn repo_root(&self) -> Option<PathBuf> {
        let root = self.repo_path.borrow().clone()?;
        root.join(".git").exists().then_some(root)
    }

    /// Reload the first page of history from scratch.
    pub fn refresh(&self) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.loading_more.set(false);
        self.commits.borrow_mut().clear();
        clear_list(&self.commit_list);
        clear_box(&self.detail_box);

        let Some(root) = self.repo_root() else {
            self.commit_list
                .append(&placeholder("Not a git repository"));
            return;
        };

        self.commit_list
            .append(&placeholder("Loading history\u{2026}"));
        self.spawn_page(root, 0, generation);
    }

    /// Fetch and append the next page, unless one is already in flight or
    /// the last page was short (history exhausted).
    fn load_more(&self) {
        if self.loading_more.get() {
            return;
        }
        let loaded = self.commits.borrow().len();
        if loaded == 0 || !loaded.is_multiple_of(PAGE) {
            return;
        }
        let Some(root) = self.repo_root() else {
            return;
        };
        self.loading_more.set(true);
        self.spawn_page(root, loaded, self.generation.get());
    }

    fn spawn_page(&self, root: PathBuf, skip: usize, generation: u64) {
        let want_refs = skip == 0;
        let (tx, mut rx) = futures_channel::mpsc::unbounded::<LoadMsg>();
        std::thread::spawn(move || {
            let git = GitController::new(&root);
            let msg = match git.log(LogStart::Head, PAGE, skip) {
                Ok(commits) => {
                    let refs = if want_refs {
                        git.ref_labels().unwrap_or_default()
                    } else {
                        RefMap::new()
                    };
                    LoadMsg::Page {
                        skip,
                        commits,
                        refs,
                    }
                }
                Err(e) => LoadMsg::Error(e),
            };
            let _ = tx.unbounded_send(msg);
        });

        let panel = self.clone();
        glib::MainContext::default().spawn_local(async move {
            let Some(msg) = rx.next().await else { return };
            if panel.generation.get() != generation {
                return;
            }
            panel.loading_more.set(false);
            match msg {
                LoadMsg::Error(e) => {
                    clear_list(&panel.commit_list);
                    panel
                        .commit_list
                        .append(&placeholder(&format!("Git error: {e}")));
                }
                LoadMsg::Page {
                    skip,
                    commits,
                    refs,
                } => {
                    if skip == 0 {
                        clear_list(&panel.commit_list);
                        *panel.refs.borrow_mut() = refs;
                        if commits.is_empty() {
                            panel.commit_list.append(&placeholder("No commits yet"));
                            return;
                        }
                    }
                    let refs = panel.refs.borrow();
                    for commit in &commits {
                        panel
                            .commit_list
                            .append(&commit_row(commit, refs.get(&commit.id)));
                    }
                    drop(refs);
                    panel.commits.borrow_mut().extend(commits);
                }
                LoadMsg::Detail(_) => {}
            }
        });
    }

    fn show_detail(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(commit) = self.commits.borrow().get(index).cloned() else {
            return;
        };
        let Some(root) = self.repo_root() else {
            return;
        };
        let generation = self.generation.get();

        let (tx, mut rx) = futures_channel::mpsc::unbounded::<LoadMsg>();
        std::thread::spawn(move || {
            let git = GitController::new(&root);
            let msg = match git.commit_detail(commit.id) {
                Ok(detail) => LoadMsg::Detail(Box::new(detail)),
                Err(e) => LoadMsg::Error(e),
            };
            let _ = tx.unbounded_send(msg);
        });

        let panel = self.clone();
        glib::MainContext::default().spawn_local(async move {
            let Some(msg) = rx.next().await else { return };
            if panel.generation.get() != generation {
                return;
            }
            clear_box(&panel.detail_box);
            match msg {
                LoadMsg::Detail(detail) => fill_detail(&panel.detail_box, &detail),
                LoadMsg::Error(e) => panel
                    .detail_box
                    .append(&placeholder(&format!("Git error: {e}"))),
                LoadMsg::Page { .. } => {}
            }
        });
    }
}

/// A dim, non-interactive status line ("Not a git repository", "Loading…").
fn placeholder(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_halign(Align::Start);
    label.set_margin_start(10);
    label.set_margin_end(10);
    label.set_margin_top(10);
    label.set_margin_bottom(10);
    label.add_css_class("dim-label");
    label
}

/// Remove every row from a `ListBox`.
fn clear_list(list: &ListBox) {
    let mut child = list.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        list.remove(&widget);
    }
}

/// Remove every child from a `Box`.
fn clear_box(bx: &GtkBox) {
    let mut child = bx.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        bx.remove(&widget);
    }
}

/// One commit row: graph tick, subject, ref chips, author, relative date.
fn commit_row(commit: &CommitSummary, refs: Option<&Vec<RefLabel>>) -> gtk::ListBoxRow {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.set_css_classes(&["git-log-commit"]);
    row.set_margin_start(2);
    row.set_margin_end(6);

    let is_merge = commit.parent_ids.len() > 1;
    let rail = DrawingArea::new();
    rail.set_content_width(14);
    rail.set_css_classes(&["git-log-rail"]);
    rail.set_draw_func(move |_area, ctx, _w, h| {
        let h = f64::from(h);
        let x = 7.0;
        ctx.set_source_rgb(0.35, 0.55, 0.85);
        ctx.set_line_width(1.5);
        ctx.move_to(x, 0.0);
        ctx.line_to(x, h);
        let _ = ctx.stroke();
        let radius = if is_merge { 4.0 } else { 3.0 };
        ctx.arc(x, h / 2.0, radius, 0.0, std::f64::consts::TAU);
        let _ = ctx.fill();
    });
    row.append(&rail);

    let subject = Label::new(Some(if commit.summary.is_empty() {
        "(no message)"
    } else {
        &commit.summary
    }));
    subject.set_halign(Align::Start);
    subject.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subject.set_hexpand(true);
    subject.set_xalign(0.0);
    subject.set_css_classes(&["git-log-subject"]);
    row.append(&subject);

    if let Some(refs) = refs {
        for label in refs {
            let chip = Label::new(Some(&label.name));
            chip.set_css_classes(&["git-log-ref-chip", ref_chip_class(label)]);
            row.append(&chip);
        }
    }

    let author = Label::new(Some(&commit.author_name));
    author.set_css_classes(&["git-log-author"]);
    author.set_ellipsize(gtk::pango::EllipsizeMode::End);
    author.set_width_chars(14);
    author.set_xalign(1.0);
    row.append(&author);

    let date = Label::new(Some(&relative_time(commit.time)));
    date.set_css_classes(&["git-log-date"]);
    date.set_width_chars(7);
    date.set_xalign(1.0);
    row.append(&date);

    let list_row = gtk::ListBoxRow::new();
    list_row.set_child(Some(&row));
    list_row
}

/// CSS modifier class for a ref chip by kind.
fn ref_chip_class(label: &RefLabel) -> &'static str {
    use crate::git::ops::RefKind::*;
    match label.kind {
        Head => "ref-head",
        LocalBranch => "ref-local",
        RemoteBranch => "ref-remote",
        Tag => "ref-tag",
    }
}

/// Populate the right-hand detail pane for one commit.
fn fill_detail(container: &GtkBox, detail: &CommitDetail) {
    let s = &detail.summary;

    let hash = Label::new(Some(&format!("commit {}", s.id)));
    hash.set_halign(Align::Start);
    hash.set_selectable(true);
    hash.set_css_classes(&["git-log-detail-hash"]);
    container.append(&hash);

    let author = Label::new(Some(&format!(
        "Author:    {} <{}>",
        s.author_name, s.author_email
    )));
    author.set_halign(Align::Start);
    author.set_selectable(true);
    container.append(&author);

    if detail.committer_email != s.author_email || detail.committer_name != s.author_name {
        let committer = Label::new(Some(&format!(
            "Committer: {} <{}>",
            detail.committer_name, detail.committer_email
        )));
        committer.set_halign(Align::Start);
        committer.set_selectable(true);
        container.append(&committer);
    }

    let date = Label::new(Some(&format!(
        "Date:      {}",
        absolute_time(detail.commit_time)
    )));
    date.set_halign(Align::Start);
    date.set_selectable(true);
    container.append(&date);

    let message = Label::new(Some(detail.body.trim()));
    message.set_halign(Align::Start);
    message.set_xalign(0.0);
    message.set_wrap(true);
    message.set_selectable(true);
    message.set_margin_top(8);
    message.set_margin_bottom(8);
    message.set_css_classes(&["git-log-detail-message"]);
    container.append(&message);

    let files_title = Label::new(Some(&format!(
        "{} file{} changed",
        detail.files.len(),
        if detail.files.len() == 1 { "" } else { "s" }
    )));
    files_title.set_halign(Align::Start);
    files_title.set_css_classes(&["git-log-files-title"]);
    container.append(&files_title);

    for file in &detail.files {
        container.append(&changed_file_row(file));
    }
}

/// One "A path/to/file" row in the detail pane.
fn changed_file_row(file: &ChangedFile) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 6);
    row.set_css_classes(&["git-log-file"]);

    let (marker, class) = match file.status {
        git2::Delta::Added => ("A", "file-added"),
        git2::Delta::Deleted => ("D", "file-deleted"),
        git2::Delta::Renamed => ("R", "file-renamed"),
        git2::Delta::Copied => ("C", "file-renamed"),
        _ => ("M", "file-modified"),
    };
    let badge = Label::new(Some(marker));
    badge.set_css_classes(&["git-log-file-badge", class]);
    badge.set_width_chars(1);
    row.append(&badge);

    let path = Label::new(Some(&file.path.to_string_lossy()));
    path.set_halign(Align::Start);
    path.set_xalign(0.0);
    path.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    path.set_hexpand(true);
    row.append(&path);

    row
}

/// Seconds-since-epoch → a compact relative label ("3d", "5h", "just now").
fn relative_time(secs: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(secs);
    let delta = (now - secs).max(0);
    match delta {
        0..=59 => "now".to_string(),
        60..=3599 => format!("{}m", delta / 60),
        3600..=86_399 => format!("{}h", delta / 3600),
        86_400..=2_591_999 => format!("{}d", delta / 86_400),
        2_592_000..=31_535_999 => format!("{}mo", delta / 2_592_000),
        _ => format!("{}y", delta / 31_536_000),
    }
}

/// Seconds-since-epoch → `YYYY-MM-DD HH:MM` in UTC (no chrono dependency).
fn absolute_time(secs: i64) -> String {
    // Days since the Unix epoch, then civil date via Howard Hinnant's algorithm.
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hh, mm) = (rem / 3600, (rem % 3600) / 60);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02} UTC")
}
