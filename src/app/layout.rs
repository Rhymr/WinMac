use crate::file::tree::FileTree;
use crate::rhyme::search::RhymeSearch;
use crate::workspace::Workspace;
use crate::workspace::controller::WorkspaceController;
#[cfg(target_os = "windows")]
use gtk::MenuButton;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Label, Orientation, Paned, glib};
use libadwaita::prelude::*;
use libadwaita::{Application, ApplicationWindow};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use libadwaita::{HeaderBar, ToolbarView, WindowTitle};
use std::cell::Cell;
use std::rc::Rc;

/// Shows the named bottom tool window (`"rhyme"` / `"git-log"`), or closes
/// the bottom dock when passed `None`.
type ShowBottom = Rc<dyn Fn(Option<&str>)>;

pub fn build_ui(app: &Application) -> (ApplicationWindow, Rc<WorkspaceController>) {
    let startup = crate::setting::Settings::load();

    // CSS is loaded once, up front, in main.rs — the welcome window needs it
    // too and is shown before this function ever runs.
    let main_window = ApplicationWindow::builder()
        .application(app)
        .title("Rhymr")
        .default_width((startup.window_width as i32).max(640))
        .default_height((startup.window_height as i32).max(480))
        .build();

    let (main_layout, workspace_controller) = create_main_layout();

    // Store the workspace controller in the window's data safely
    unsafe {
        main_window.set_data("workspace_controller", workspace_controller.clone());
    }

    // `setup_menu` wires up every File/Git/Help action (including their
    // keyboard accelerators) and sets the app's native menubar. GNOME/Linux
    // desktops can surface that menubar through shell integration and their
    // own window manager already gives the window a draggable native
    // titlebar, so neither needs any extra in-window chrome — content goes
    // straight into the window. Windows has no such shell integration at
    // all, so it gets a full in-window header bar with a hamburger menu
    // reaching the same actions. macOS's menubar integration covers
    // File/Git/Help (no hamburger needed), but an `AdwApplicationWindow`
    // still uses client-side decorations there with no native titlebar of
    // its own — without *something* in the header-bar role the window has
    // no draggable region at all — so it gets the same slim header bar as
    // Windows, just without the menu button.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let menu_model = crate::app::menu::setup_menu(app, workspace_controller.clone());
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    crate::app::menu::setup_menu(app, workspace_controller.clone());

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let header_bar = HeaderBar::new();
        header_bar.set_title_widget(Some(&WindowTitle::new("Rhymr", "")));

        #[cfg(target_os = "windows")]
        {
            let menu_button = MenuButton::builder()
                .icon_name("open-menu-symbolic")
                .menu_model(&menu_model)
                .tooltip_text("Main Menu")
                .build();
            header_bar.pack_end(&menu_button);
        }
        #[cfg(target_os = "macos")]
        let _ = &menu_model;

        let toolbar_view = ToolbarView::new();
        toolbar_view.add_top_bar(&header_bar);
        toolbar_view.set_content(Some(&main_layout));

        // `AdwApplicationWindow` doesn't support plain `GtkWindow::set_child`
        // (it aborts at runtime: "gtk_window_set_child() is not supported
        // for AdwApplicationWindow") — it manages its own internal child and
        // exposes `content` as its own property/method instead.
        main_window.set_content(Some(&toolbar_view));
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    main_window.set_content(Some(&main_layout));

    main_window.present();

    (main_window, workspace_controller)
}

pub fn create_main_layout() -> (GtkBox, Rc<WorkspaceController>) {
    // Startup-only sizing (Settings → Appearance & Behavior → Window &
    // Startup). A per-workspace remembered size still wins over these.
    let startup = crate::setting::Settings::load();

    let workspace_controller = Rc::new(WorkspaceController::new());

    let mut file_tree = FileTree::new();
    let workspace = Rc::new(Workspace::new(
        Rc::clone(&workspace_controller),
        Some(file_tree.clone()),
    ));
    file_tree.set_workspace(workspace.clone());
    workspace_controller.set_workspace(workspace.clone());

    let file_tree_widget = file_tree.get_widget().clone();
    let rhyme_search = RhymeSearch::new();
    let rhyme_frame = rhyme_search.get_widget().clone();
    rhyme_search.set_expanded(true);

    // Git Log — the second bottom-docked tool window. Shares the bottom dock
    // with Rhyme Search (one visible at a time), toggled from the bottom
    // stripe / the `app.git-log` action.
    let git_log = crate::git::log_panel::GitLogPanel::new();
    let git_log_frame = git_log.get_widget().clone();
    git_log.set_expanded(true);

    // Selecting one word in the editor seeds the Rhyme Search box — but
    // only while the panel is open (no lookup runs; the user hits Enter).
    {
        let rhyme_search = rhyme_search.clone();
        workspace_controller.set_selection_listener(move |word| {
            if !rhyme_search.is_collapsed()
                && let Some(w) = word
            {
                rhyme_search.set_query(&w);
            }
        });
    }

    // Read-only "Apple Notes" (and future external sources) tree, stacked
    // under the project tree in the left column. Workspace-independent:
    // it's re-pointed at each project's `.rhymr/` cache via the controller.
    let source_panel = crate::app::source_panel::SourcePanel::new();
    {
        let ws = workspace.clone();
        source_panel.connect_open(move |title, body| ws.open_readonly(&title, &body));
    }
    source_panel.start();

    // The project tree and every source tree stack in one column that
    // scrolls as a single list (each inner tree grows to its content;
    // this outer scroller is the only one).
    let left_column = GtkBox::new(Orientation::Vertical, 0);
    file_tree_widget.set_vexpand(false);
    left_column.append(&file_tree_widget);
    left_column.append(source_panel.get_widget());

    let left_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .css_classes(["left-panel-scroller"])
        .child(&left_column)
        .build();

    // file tree column | editor, flush against each other (only the tree's
    // 1px right border separates them — no draggable "gap").
    let content_pane = create_horizontal_split(
        &left_scroller,
        workspace.get_widget(),
        (startup.left_panel_width as i32).clamp(120, 900),
    );
    content_pane.set_hexpand(true);

    // Left tool-window stripe (vertical "Project" label) toggles the column.
    let ft_for_stripe = left_scroller.clone();
    let left_stripe =
        crate::app::chrome::left_stripe(true, move |show| ft_for_stripe.set_visible(show));

    let work_row = GtkBox::new(Orientation::Horizontal, 0);
    work_row.set_vexpand(true);
    work_row.append(&left_stripe);
    work_row.append(&content_pane);

    // Rhyme Search and Git Log share one bottom dock — a plain vertical box
    // holding both frames, one shown at a time — docked at `outer_split`'s
    // end. Hidden until a bottom-stripe button is pressed.
    rhyme_frame.set_visible(false);
    git_log_frame.set_visible(false);
    let bottom_dock = GtkBox::new(Orientation::Vertical, 0);
    bottom_dock.append(&rhyme_frame);
    bottom_dock.append(&git_log_frame);

    let outer_split = Paned::new(Orientation::Vertical);
    outer_split.set_start_child(Some(&work_row));
    outer_split.set_end_child(Some(&bottom_dock));
    outer_split.set_resize_start_child(true);
    outer_split.set_resize_end_child(false);
    outer_split.set_shrink_start_child(false);
    outer_split.set_shrink_end_child(false);
    outer_split.set_vexpand(true);

    let default_rhyme_height = (startup.rhyme_panel_height as i32).max(80);
    let remembered = Rc::new(Cell::new(default_rhyme_height));

    // Show the named bottom panel (`"rhyme"` / `"git-log"`), or `None` to
    // close the dock, resizing the split and persisting the choice.
    let show_bottom: ShowBottom = {
        let outer_split = outer_split.clone();
        let rhyme_frame = rhyme_frame.clone();
        let git_log_frame = git_log_frame.clone();
        let rhyme_search = rhyme_search.clone();
        let git_log = git_log.clone();
        let remembered = remembered.clone();
        let controller = workspace_controller.clone();
        Rc::new(move |which: Option<&str>| {
            let total = outer_split.height();
            let opening = which.is_some();

            rhyme_frame.set_visible(which == Some("rhyme"));
            git_log_frame.set_visible(which == Some("git-log"));
            rhyme_search.set_expanded(which == Some("rhyme"));
            git_log.set_expanded(which == Some("git-log"));

            if opening {
                let total = total.max(400);
                outer_split.set_position((total - remembered.get()).max(120));
            } else if total > 120 {
                remembered.set((total - outer_split.position()).clamp(120, total - 60));
            }
            if let Some(root) = controller.get_root_path() {
                crate::workspace::session::update(&root, |s| {
                    s.rhyme_panel_height = Some(remembered.get());
                    s.bottom_panel = which.map(str::to_string);
                });
            }
        })
    };

    let (bottom_stripe, bottom_buttons) = crate::app::chrome::bottom_stripe(&[
        crate::app::chrome::BottomTool {
            icon: "search",
            label: "Rhyme Search",
        },
        crate::app::chrome::BottomTool {
            icon: "git-commit",
            label: "Git",
        },
    ]);
    let rhyme_btn = bottom_buttons[0].clone();
    let git_btn = bottom_buttons[1].clone();

    // The two stripe buttons are a radio pair: activating one deactivates the
    // other. `updating` breaks the re-entrant `toggled` that `set_active`
    // would otherwise cause.
    let updating = Rc::new(Cell::new(false));
    {
        let (other, show, updating) = (git_btn.clone(), show_bottom.clone(), updating.clone());
        rhyme_btn.connect_toggled(move |b| {
            if updating.get() {
                return;
            }
            updating.set(true);
            if b.is_active() {
                other.set_active(false);
                show(Some("rhyme"));
            } else if !other.is_active() {
                show(None);
            }
            updating.set(false);
        });
    }
    {
        let (other, show, updating) = (rhyme_btn.clone(), show_bottom.clone(), updating.clone());
        git_btn.connect_toggled(move |b| {
            if updating.get() {
                return;
            }
            updating.set(true);
            if b.is_active() {
                other.set_active(false);
                show(Some("git-log"));
            } else if !other.is_active() {
                show(None);
            }
            updating.set(false);
        });
    }

    // `app.git-log` action / a future dock affordance route through here.
    {
        let git_btn = git_btn.clone();
        workspace_controller.set_git_log_toggle_listener(move || {
            git_btn.set_active(!git_btn.is_active());
        });
    }
    // Reload the Git Log after an in-app commit / pull / fetch.
    {
        let git_log = git_log.clone();
        workspace_controller.set_git_changed_listener(move || git_log.refresh());
    }

    // Restore the left-panel width, remembered bottom-panel height and which
    // bottom panel was open, once the workspace root is known; keep the
    // source panel and Git Log pointed at it.
    {
        let sp = source_panel.clone();
        let content_pane = content_pane.clone();
        let remembered = remembered.clone();
        let git_log = git_log.clone();
        let rhyme_btn = rhyme_btn.clone();
        let git_btn = git_btn.clone();
        workspace_controller.set_root_listener(move |root| {
            if let Some(r) = root.as_deref() {
                let s = crate::workspace::session::load(r);
                if let Some(w) = s.left_panel_width {
                    content_pane.set_position(w.clamp(120, 900));
                }
                remembered.set(s.rhyme_panel_height.unwrap_or(default_rhyme_height));
                git_log.set_repo(root.clone());
                match s.bottom_panel.as_deref() {
                    Some("rhyme") => rhyme_btn.set_active(true),
                    Some("git-log") => git_btn.set_active(true),
                    _ => {}
                }
            } else {
                git_log.set_repo(None);
            }
            sp.set_workspace_root(root);
        });
    }

    // Persist the left-panel width on drag, debounced so a drag isn't a
    // burst of file writes.
    {
        let controller = workspace_controller.clone();
        let generation = Rc::new(Cell::new(0u64));
        content_pane.connect_position_notify(move |pane| {
            let width = pane.position();
            let this = generation.get() + 1;
            generation.set(this);
            let (generation, controller) = (generation.clone(), controller.clone());
            glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
                if generation.get() != this {
                    return;
                }
                if let Some(root) = controller.get_root_path() {
                    crate::workspace::session::update(&root, |s| {
                        s.left_panel_width = Some(width);
                    });
                }
            });
        });
    }

    let main_box = GtkBox::new(Orientation::Vertical, 0);
    main_box.append(&crate::app::chrome::main_toolbar(&workspace_controller));
    main_box.append(&outer_split);
    main_box.append(&bottom_stripe);
    main_box.append(&create_status_bar(&workspace_controller));

    (main_box, workspace_controller)
}

fn create_status_bar(workspace_controller: &Rc<WorkspaceController>) -> GtkBox {
    let status_bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(vec!["status-bar"])
        .build();

    // Left: free-text message, pushed hard-left. Everything after it is a
    // right-aligned group of 1px-fenced info segments, classic-IDE style.
    let status_label = Label::new(Some("Rhymr"));
    status_label.set_css_classes(&["status-text"]);
    status_label.set_hexpand(true);
    status_label.set_halign(gtk::Align::Start);
    status_bar.append(&status_label);

    let branch_label = Label::new(None);
    branch_label.set_css_classes(&["status-segment", "branch"]);
    branch_label.set_visible(false);
    status_bar.append(&branch_label);

    let cursor_label = Label::new(Some("1:1"));
    cursor_label.set_css_classes(&["status-segment", "cursor-pos"]);
    status_bar.append(&cursor_label);

    let word_count_label = Label::new(Some("0 words"));
    word_count_label.set_css_classes(&["status-segment", "word-count"]);
    status_bar.append(&word_count_label);

    workspace_controller.set_word_count_listener(move |count| {
        let label = if count == 1 {
            "1 word".to_string()
        } else {
            format!("{count} words")
        };
        word_count_label.set_text(&label);
    });
    workspace_controller.set_cursor_listener(move |line, col| {
        cursor_label.set_text(&format!("{line}:{col}"));
    });
    workspace_controller.set_branch_listener(move |branch| match branch {
        Some(name) => {
            branch_label.set_text(&name);
            branch_label.set_visible(true);
        }
        None => branch_label.set_visible(false),
    });
    workspace_controller.refresh_word_count();
    workspace_controller.refresh_cursor();
    workspace_controller.refresh_branch();

    status_bar
}
pub fn create_horizontal_split(
    left: &impl IsA<gtk::Widget>,
    right: &impl IsA<gtk::Widget>,
    position: i32,
) -> Paned {
    let horizontal_pane = Paned::new(Orientation::Horizontal);
    horizontal_pane.set_start_child(Some(left));
    horizontal_pane.set_end_child(Some(right));
    horizontal_pane.set_position(position);

    horizontal_pane.set_resize_start_child(true);
    horizontal_pane.set_resize_end_child(true);
    horizontal_pane.set_shrink_start_child(false);
    horizontal_pane.set_shrink_end_child(false);

    horizontal_pane
}
