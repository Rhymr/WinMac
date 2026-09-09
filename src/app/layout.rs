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

/// Height (px) the bottom Rhyme Search panel opens to.
const RHYME_PANEL_HEIGHT: i32 = 240;

pub fn build_ui(app: &Application) -> (ApplicationWindow, Rc<WorkspaceController>) {
    // CSS is loaded once, up front, in main.rs — the welcome window needs it
    // too and is shown before this function ever runs.
    let main_window = ApplicationWindow::builder()
        .application(app)
        .title("Rhymr")
        .default_width(1280)
        .default_height(720)
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
    let content_pane = create_horizontal_split(&left_scroller, workspace.get_widget(), 300);
    content_pane.set_hexpand(true);

    // Left tool-window stripe (vertical "Project" label) toggles the column.
    let ft_for_stripe = left_scroller.clone();
    let left_stripe =
        crate::app::chrome::left_stripe(true, move |show| ft_for_stripe.set_visible(show));

    let work_row = GtkBox::new(Orientation::Horizontal, 0);
    work_row.set_vexpand(true);
    work_row.append(&left_stripe);
    work_row.append(&content_pane);

    // Rhyme Search is docked at the bottom like a terminal panel — hidden
    // until its bottom-stripe button is pressed.
    rhyme_frame.set_visible(false);
    let outer_split = Paned::new(Orientation::Vertical);
    outer_split.set_start_child(Some(&work_row));
    outer_split.set_end_child(Some(&rhyme_frame));
    outer_split.set_resize_start_child(true);
    outer_split.set_resize_end_child(false);
    outer_split.set_shrink_start_child(false);
    outer_split.set_shrink_end_child(false);
    outer_split.set_vexpand(true);

    let remembered = Rc::new(Cell::new(RHYME_PANEL_HEIGHT));
    let bottom_stripe = {
        let outer_split = outer_split.clone();
        let rhyme_frame = rhyme_frame.clone();
        let remembered = remembered.clone();
        let controller = workspace_controller.clone();
        crate::app::chrome::bottom_stripe(false, move |show| {
            let total = outer_split.height();
            if show {
                rhyme_frame.set_visible(true);
                let total = total.max(400);
                outer_split.set_position((total - remembered.get()).max(120));
            } else {
                if total > 120 {
                    remembered.set((total - outer_split.position()).clamp(120, total - 60));
                }
                rhyme_frame.set_visible(false);
            }
            if let Some(root) = controller.get_root_path() {
                crate::workspace::session::update(&root, |s| {
                    s.rhyme_panel_height = Some(remembered.get());
                    s.rhyme_panel_visible = Some(show);
                });
            }
        })
    };

    // Restore the left-panel width and remembered Rhyme-panel height once
    // the workspace root is known, and keep the source panel pointed at it.
    {
        let sp = source_panel.clone();
        let content_pane = content_pane.clone();
        let remembered = remembered.clone();
        workspace_controller.set_root_listener(move |root| {
            if let Some(r) = root.as_deref() {
                let s = crate::workspace::session::load(r);
                if let Some(w) = s.left_panel_width {
                    content_pane.set_position(w.clamp(120, 900));
                }
                remembered.set(s.rhyme_panel_height.unwrap_or(RHYME_PANEL_HEIGHT));
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
