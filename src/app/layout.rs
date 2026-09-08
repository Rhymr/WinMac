use crate::file::tree::FileTree;
use crate::rhyme::search::RhymeSearch;
use crate::workspace::Workspace;
use crate::workspace::controller::WorkspaceController;
use gtk::prelude::*;
#[cfg(target_os = "windows")]
use gtk::MenuButton;
use gtk::{Box as GtkBox, Label, Orientation, Paned};
use libadwaita::prelude::*;
use libadwaita::{Application, ApplicationWindow};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use libadwaita::{HeaderBar, ToolbarView, WindowTitle};
use std::cell::Cell;
use std::rc::Rc;

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
    // Create the workspace controller
    let workspace_controller = Rc::new(WorkspaceController::new());

    // Create the main content area: file tree on the left, editor on the right
    let (content_pane, _file_tree, _workspace) = create_content_layout(&workspace_controller);

    let main_box = GtkBox::new(Orientation::Vertical, 0);
    main_box.append(&content_pane);
    main_box.append(&create_status_bar(&workspace_controller));

    (main_box, workspace_controller)
}

fn create_status_bar(workspace_controller: &Rc<WorkspaceController>) -> GtkBox {
    let status_bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .css_classes(vec!["status-bar"])
        .build();

    let status_label = Label::new(Some("Rhymr"));
    status_label.set_css_classes(&["status-text"]);
    status_bar.append(&status_label);

    let word_count_label = Label::new(Some("0 words"));
    word_count_label.set_css_classes(&["status-text", "word-count"]);
    word_count_label.set_hexpand(true);
    word_count_label.set_halign(gtk::Align::End);
    status_bar.append(&word_count_label);

    workspace_controller.set_word_count_listener(move |count| {
        let label = if count == 1 {
            "1 word".to_string()
        } else {
            format!("{count} words")
        };
        word_count_label.set_text(&label);
    });
    workspace_controller.refresh_word_count();

    status_bar
}

// Create the file tree / rhyme search / editor split
fn create_content_layout(
    workspace_controller: &Rc<WorkspaceController>,
) -> (Paned, FileTree, Rc<Workspace>) {
    // Create the FileTree component
    let mut file_tree = FileTree::new();

    // Create the Workspace instance with the FileTree
    let workspace = Rc::new(Workspace::new(
        Rc::clone(workspace_controller),
        Some(file_tree.clone()),
    ));

    // Set the workspace reference in the file tree
    file_tree.set_workspace(workspace.clone());

    workspace_controller.set_workspace(workspace.clone());

    // Rhyme search sits below the file tree on the left, starting collapsed
    let rhyme_search = RhymeSearch::new();
    let rhyme_search_widget = rhyme_search.get_widget();
    rhyme_search_widget.add_css_class("bottom-section");

    let file_tree_widget = file_tree.get_widget();
    file_tree_widget.add_css_class("left-edge");
    let left_split = create_vertical_split(file_tree_widget, rhyme_search_widget, 360);

    // Collapsing the panel hides its content, but a Paned doesn't
    // automatically resize the split just because a child got smaller, and
    // `shrink_end_child(false)` only limits how far a user *drag* can go —
    // it does NOT stop a plain `set_position()` call from squeezing the end
    // child below its minimum (which was swallowing the header entirely).
    // So the collapsed position has to be computed explicitly: total height
    // minus the header's own minimum height.
    let rhyme_search_widget_owned = rhyme_search_widget.clone();
    let collapsed_position = move |paned: &Paned| -> i32 {
        let total = paned.height();
        let (_, header_height, _, _) = rhyme_search_widget_owned.measure(Orientation::Vertical, -1);
        (total - header_height).max(0)
    };

    // The window isn't realized yet at construction time, so `paned.height()`
    // would read 0 — defer the initial collapse to the next main-loop tick,
    // by which point the first real allocation has happened.
    let paned_for_init = left_split.clone();
    let collapsed_position_for_init = collapsed_position.clone();
    glib::idle_add_local_once(move || {
        paned_for_init.set_position(collapsed_position_for_init(&paned_for_init));
    });

    // Lock the divider while collapsed: a drag attempt still moves
    // `position` internally, so snap it straight back instead of letting
    // the user resize a panel with nothing visible in it.
    let rhyme_search_for_lock = rhyme_search.clone();
    let collapsed_position_for_lock = collapsed_position.clone();
    left_split.connect_position_notify(move |paned| {
        if rhyme_search_for_lock.is_collapsed() {
            let desired = collapsed_position_for_lock(paned);
            if paned.position() != desired {
                paned.set_position(desired);
            }
        }
    });

    // Restore this height when the panel is expanded again.
    let expanded_position = Rc::new(Cell::new(360));
    let paned_for_toggle = left_split.clone();
    let expanded_position_for_toggle = expanded_position.clone();
    rhyme_search.connect_toggle(move |collapsed| {
        if collapsed {
            expanded_position_for_toggle.set(paned_for_toggle.position());
            paned_for_toggle.set_position(collapsed_position(&paned_for_toggle));
        } else {
            paned_for_toggle.set_position(expanded_position_for_toggle.get());
        }
    });

    // Horizontal split between the left column and the editor
    let main_pane = create_horizontal_split(&left_split, workspace.get_widget(), 320);

    (main_pane, file_tree, workspace)
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

pub fn create_vertical_split(
    top: &impl IsA<gtk::Widget>,
    bottom: &impl IsA<gtk::Widget>,
    position: i32,
) -> Paned {
    let vertical_pane = Paned::new(Orientation::Vertical);
    vertical_pane.set_start_child(Some(top));
    vertical_pane.set_end_child(Some(bottom));
    vertical_pane.set_position(position);

    vertical_pane.set_resize_start_child(true);
    vertical_pane.set_resize_end_child(true);
    vertical_pane.set_shrink_start_child(false);
    vertical_pane.set_shrink_end_child(false);

    vertical_pane
}
