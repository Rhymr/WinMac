use crate::file::tree::FileTree;
use crate::rhyme::search::RhymeSearch;
use crate::workspace::Workspace;
use crate::workspace::controller::WorkspaceController;
#[cfg(target_os = "windows")]
use gtk::MenuButton;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Label, Orientation};
use libadwaita::prelude::*;
use libadwaita::{Application, ApplicationWindow};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use libadwaita::{HeaderBar, ToolbarView, WindowTitle};
use std::rc::Rc;

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

    // Git Log — a bottom-docked tool window alongside Rhyme Search.
    let git_log = crate::git::log_panel::GitLogPanel::new();
    let git_log_frame = git_log.get_widget().clone();
    git_log.set_expanded(true);

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

    rhyme_frame.set_visible(true);
    git_log_frame.set_visible(true);

    // The dock manager owns the Paned tree + per-edge stripes/headers. The
    // editor notebook is the centre; Project / Rhyme Search / Git Log are
    // registered as tool windows.
    let dock = crate::app::dock::DockArea::new(workspace.get_widget());
    let left_default = (startup.left_panel_width as i32).clamp(120, 900);
    let bottom_default = (startup.rhyme_panel_height as i32).max(120);
    dock.register(crate::app::tool_window::ToolWindow {
        id: "project",
        title: "Project",
        icon: "folder",
        default_anchor: crate::app::tool_window::Anchor::Left,
        default_size: left_default,
        default_open: true,
        content: left_scroller.clone().upcast(),
        header_actions: file_tree.header_actions(),
    });
    dock.register(crate::app::tool_window::ToolWindow {
        id: "rhyme-search",
        title: "Rhyme Search",
        icon: "search",
        default_anchor: crate::app::tool_window::Anchor::Bottom,
        default_size: bottom_default,
        default_open: false,
        content: rhyme_frame.clone().upcast(),
        header_actions: rhyme_search.header_actions(),
    });
    dock.register(crate::app::tool_window::ToolWindow {
        id: "git-log",
        title: "Git Log",
        icon: "git-commit",
        default_anchor: crate::app::tool_window::Anchor::Bottom,
        default_size: bottom_default,
        default_open: false,
        content: git_log_frame.clone().upcast(),
        header_actions: git_log.header_actions(),
    });

    // Route the `app.*` tool-window actions through the dock.
    {
        let dock = dock.clone();
        workspace_controller.set_tool_toggle_listener(move |id| dock.toggle(id));
    }
    // Selecting one word in the editor seeds the Rhyme Search box — but only
    // while that panel is open (no lookup runs; the user hits Enter).
    {
        let rhyme_search = rhyme_search.clone();
        let dock = dock.clone();
        workspace_controller.set_selection_listener(move |word| {
            if dock.is_open("rhyme-search")
                && let Some(w) = word
            {
                rhyme_search.set_query(&w);
            }
        });
    }
    {
        let dock = dock.clone();
        workspace_controller.set_restore_layout_listener(move || dock.restore_default_layout());
    }
    // Reload the Git Log after an in-app commit / pull / fetch.
    {
        let git_log = git_log.clone();
        workspace_controller.set_git_changed_listener(move || git_log.refresh());
    }

    // Keep the source panel and Git Log pointed at the workspace root.
    {
        let sp = source_panel.clone();
        let git_log = git_log.clone();
        workspace_controller.set_root_listener(move |root| {
            git_log.set_repo(root.clone());
            sp.set_workspace_root(root);
        });
    }

    dock.restore();

    let main_box = GtkBox::new(Orientation::Vertical, 0);
    main_box.append(&crate::app::chrome::main_toolbar(&workspace_controller));
    main_box.append(dock.widget());
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
