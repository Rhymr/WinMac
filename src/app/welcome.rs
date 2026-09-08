use crate::platform::fetch_apple_notes;
use crate::workspace::manager::WorkspaceManager;
use crate::workspace::recent::load_recent_workspaces;
use gio::prelude::FileExt;
use gtk::prelude::*;
use gtk::{
    Align, ApplicationWindow, Box, Button, CheckButton, Entry, FileDialog, Grid, Image, Label,
    ListBox, ListBoxRow, MenuButton, Orientation, Popover, SearchEntry, Window,
};
use libadwaita::Application;
use std::path::PathBuf;
use std::rc::Rc;

/// Shows the workspace picker as the app's initial ApplicationWindow.
/// The main editor layout isn't built until a workspace is actually chosen.
pub fn show_welcome_dialog<F>(app: &Application, on_workspace_ready: F)
where
    F: Fn(PathBuf) + 'static,
{
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Welcome to Rhymr")
        .default_width(850)
        .default_height(540)
        .resizable(false)
        .build();

    let main_box = Box::new(Orientation::Horizontal, 0);

    // ==========================================
    // 1. LEFT SIDEBAR
    // ==========================================
    let sidebar = Box::new(Orientation::Vertical, 0);
    sidebar.set_width_request(200);
    sidebar.set_css_classes(&["welcome-sidebar"]);

    // App Branding Header
    let brand_box = Box::new(Orientation::Horizontal, 10);
    brand_box.set_css_classes(&["brand-box"]);
    brand_box.set_margin_top(16);
    brand_box.set_margin_bottom(20);
    brand_box.set_margin_start(16);
    brand_box.set_margin_end(16);

    let logo_icon = Image::from_resource("/org/gtk_rs/rhymr/icons/clipboard.svg");
    logo_icon.set_css_classes(&["brand-icon"]);
    logo_icon.set_pixel_size(28);

    let title_vbox = Box::new(Orientation::Vertical, 0);
    let app_title = Label::builder()
        .label("Rhymr")
        .halign(Align::Start)
        .css_classes(vec!["title-3", "bold"])
        .build();
    let app_version = Label::builder()
        .label("2026.1")
        .halign(Align::Start)
        .css_classes(vec!["caption", "dim-label"])
        .build();

    title_vbox.append(&app_title);
    title_vbox.append(&app_version);
    brand_box.append(&logo_icon);
    brand_box.append(&title_vbox);

    // Sidebar Nav List — just Projects for now, no dev-tooling sections
    let nav_list = ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .css_classes(vec!["sidebar-nav"])
        .vexpand(true)
        .build();

    let projects_label = Label::builder()
        .label("Projects")
        .halign(Align::Start)
        .margin_start(16)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    nav_list.append(&projects_label);
    nav_list.select_row(nav_list.row_at_index(0).as_ref());

    // Configure / Help dropdowns — pinned to the bottom of the sidebar
    // (nav_list above is vexpand), matching the JetBrains welcome window.
    let footer = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(2)
        .css_classes(vec!["welcome-sidebar-footer"])
        .margin_start(8)
        .margin_end(8)
        .margin_top(6)
        .margin_bottom(10)
        .build();

    let app_for_settings = app.clone();
    let configure_btn = sidebar_dropdown(
        "Configure",
        vec![(
            "Settings\u{2026}",
            menu_action(move || {
                crate::setting::dialog::show_settings_dialog(&app_for_settings, None);
            }),
        )],
    );

    let window_for_docs = window.clone();
    let window_for_issue = window.clone();
    let window_for_about = window.clone();
    let help_btn = sidebar_dropdown(
        "Help",
        vec![
            (
                "Documentation",
                menu_action(move || open_uri(&window_for_docs, "https://github.com/rhymr/win-mac")),
            ),
            (
                "Report an Issue",
                menu_action(move || {
                    open_uri(&window_for_issue, "https://github.com/rhymr/win-mac/issues")
                }),
            ),
            (
                "About Rhymr",
                menu_action(move || show_about(&window_for_about)),
            ),
        ],
    );

    footer.append(&configure_btn);
    footer.append(&help_btn);

    sidebar.append(&brand_box);
    sidebar.append(&nav_list);
    sidebar.append(&footer);

    // ==========================================
    // 2. MAIN CONTENT AREA (Projects View)
    // ==========================================
    let content_vbox = Box::new(Orientation::Vertical, 0);
    content_vbox.set_hexpand(true);
    content_vbox.set_css_classes(&["welcome-content"]);

    // Top Header Bar: Search + Actions
    let top_bar = Box::new(Orientation::Horizontal, 8);
    top_bar.set_margin_top(16);
    top_bar.set_margin_bottom(16);
    top_bar.set_margin_start(20);
    top_bar.set_margin_end(20);

    let search_entry = SearchEntry::builder()
        .placeholder_text("Search projects")
        .hexpand(true)
        .build();

    let new_btn = Button::builder().label("New Project").build();
    let open_btn = Button::builder().label("Open").build();

    top_bar.append(&search_entry);
    top_bar.append(&new_btn);
    top_bar.append(&open_btn);

    // Recent Projects List
    let projects_list = ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .margin_start(20)
        .margin_end(20)
        .margin_bottom(20)
        .vexpand(true)
        .css_classes(vec!["recent-projects-list"])
        .build();

    let on_workspace_ready = Rc::new(on_workspace_ready);

    let recent_workspaces = load_recent_workspaces();
    if recent_workspaces.is_empty() {
        let empty_label = Label::builder()
            .label("No recent workspaces yet")
            .halign(Align::Start)
            .margin_top(12)
            .css_classes(vec!["dim-label"])
            .build();
        projects_list.append(&empty_label);
        projects_list.set_selection_mode(gtk::SelectionMode::None);
    } else {
        for workspace_path in &recent_workspaces {
            let name = workspace_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Workspace")
                .to_string();
            let path_str = workspace_path.to_string_lossy().to_string();

            let row = ListBoxRow::new();
            let hbox = Box::new(Orientation::Horizontal, 12);
            hbox.set_margin_top(10);
            hbox.set_margin_bottom(10);
            hbox.set_margin_start(12);
            hbox.set_margin_end(12);

            // Avatar icon with first letter
            let initial = name.chars().next().unwrap_or('W').to_string();
            let avatar_label = Label::builder()
                .label(&initial)
                .css_classes(vec!["project-avatar"])
                .width_request(32)
                .height_request(32)
                .build();

            let details_vbox = Box::new(Orientation::Vertical, 2);
            details_vbox.set_hexpand(true);
            let name_label = Label::builder()
                .label(&name)
                .halign(Align::Start)
                .css_classes(vec!["bold"])
                .build();
            let path_label = Label::builder()
                .label(&path_str)
                .halign(Align::Start)
                .css_classes(vec!["dim-label", "caption"])
                .build();

            details_vbox.append(&name_label);
            details_vbox.append(&path_label);

            let menu_btn = Button::builder()
                .label("\u{22EE}")
                .valign(Align::Center)
                .css_classes(vec!["flat", "project-menu-button"])
                .build();

            hbox.append(&avatar_label);
            hbox.append(&details_vbox);
            hbox.append(&menu_btn);
            row.set_child(Some(&hbox));

            unsafe {
                row.set_data("workspace_path", workspace_path.clone());
            }

            projects_list.append(&row);

            let path_for_menu = workspace_path.clone();
            let row_for_menu = row.clone();
            let projects_list_for_menu = projects_list.clone();
            let window_for_menu = window.clone();
            let callback_for_menu = on_workspace_ready.clone();
            menu_btn.connect_clicked(move |button| {
                show_project_menu(
                    button,
                    path_for_menu.clone(),
                    &row_for_menu,
                    &projects_list_for_menu,
                    &window_for_menu,
                    callback_for_menu.clone(),
                );
            });
        }
    }

    content_vbox.append(&top_bar);
    content_vbox.append(&projects_list);

    main_box.append(&sidebar);
    main_box.append(&content_vbox);

    window.set_child(Some(&main_box));

    // ==========================================
    // 3. ACTION HANDLERS & LOGIC
    // ==========================================

    // Open Workspace (Choose existing folder via FileDialog)
    let callback_open = on_workspace_ready.clone();
    let window_for_open = window.clone();
    open_btn.connect_clicked(move |_| {
        let file_dialog = FileDialog::new();
        file_dialog.set_title("Open Workspace Folder");

        let dlg_inner = window_for_open.clone();
        let cb_inner = callback_open.clone();

        file_dialog.select_folder(
            Some(&window_for_open),
            gio::Cancellable::NONE,
            move |result| {
                if let Ok(folder) = result
                    && let Some(path) = folder.path()
                {
                    dlg_inner.close();
                    cb_inner(path);
                }
            },
        );
    });

    // New Project Button Handler — opens the New Project dialog instead of
    // creating a workspace immediately
    let callback_new = on_workspace_ready.clone();
    let window_for_new = window.clone();
    new_btn.connect_clicked(move |_| {
        show_new_project_dialog(&window_for_new, callback_new.clone());
    });

    // Recent Projects Row Selection Handler
    let callback_row = on_workspace_ready.clone();
    let dlg_row = window.clone();
    projects_list.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            unsafe {
                if let Some(path_ptr) = row.data::<PathBuf>("workspace_path") {
                    let path = path_ptr.as_ref().clone();
                    dlg_row.close();
                    callback_row(path);
                }
            }
        }
    });

    window.present();
}

/// Right-click-style menu (opened from the row's "⋮" button) for a single
/// recent-project row: open it, reveal it on disk, copy its path, or drop
/// it from the recent list.
fn show_project_menu<F>(
    anchor: &Button,
    workspace_path: PathBuf,
    row: &ListBoxRow,
    projects_list: &ListBox,
    window: &ApplicationWindow,
    on_workspace_ready: Rc<F>,
) where
    F: Fn(PathBuf) + 'static,
{
    let menu = crate::app::context_menu::ContextMenu::new(anchor);

    let window_for_open = window.clone();
    let path_for_open = workspace_path.clone();
    let callback_for_open = on_workspace_ready.clone();
    menu.add_item("Open Selected", None, None, move || {
        window_for_open.close();
        callback_for_open(path_for_open.clone());
    });

    menu.add_separator();

    let path_for_reveal = workspace_path.clone();
    menu.add_item("Reveal in Finder", None, None, move || {
        let uri = format!("file://{}", path_for_reveal.display());
        let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
    });

    let path_for_copy = workspace_path.to_string_lossy().to_string();
    menu.add_item("Copy Path", None, None, move || {
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&path_for_copy);
        }
    });

    menu.add_separator();

    let path_for_remove = workspace_path.clone();
    let row_for_remove = row.clone();
    let projects_list_for_remove = projects_list.clone();
    menu.add_item(
        "Remove from Recent Projects\u{2026}",
        None,
        Some("destructive-menu-item"),
        move || {
            crate::workspace::recent::remove_recent_workspace(&path_for_remove);
            projects_list_for_remove.remove(&row_for_remove);

            if projects_list_for_remove.row_at_index(0).is_none() {
                let empty_label = Label::builder()
                    .label("No recent workspaces yet")
                    .halign(Align::Start)
                    .margin_top(12)
                    .css_classes(vec!["dim-label"])
                    .build();
                projects_list_for_remove.append(&empty_label);
                projects_list_for_remove.set_selection_mode(gtk::SelectionMode::None);
            }
        },
    );

    menu.popup_below(anchor);
}

/// The "New Project" dialog: name, location, and Git/Apple-Notes options —
/// deliberately without any language/build-system pickers, since those
/// don't apply to a plain-text lyrics workspace.
fn show_new_project_dialog<F>(parent: &ApplicationWindow, on_workspace_ready: Rc<F>)
where
    F: Fn(PathBuf) + 'static,
{
    let dialog = Window::builder()
        .title("New Project")
        .transient_for(parent)
        .modal(true)
        .default_width(480)
        .resizable(false)
        .build();

    let content = Box::new(Orientation::Vertical, 16);
    content.set_margin_top(24);
    content.set_margin_bottom(24);
    content.set_margin_start(24);
    content.set_margin_end(24);

    let grid = Grid::builder().row_spacing(10).column_spacing(12).build();

    let name_label = Label::builder()
        .label("Name:")
        .halign(Align::End)
        .css_classes(vec!["field-label"])
        .build();
    let name_entry = Entry::builder()
        .text("RhymrWorkspace")
        .hexpand(true)
        .build();
    grid.attach(&name_label, 0, 0, 1, 1);
    grid.attach(&name_entry, 1, 0, 2, 1);

    let default_location = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let location_label = Label::builder()
        .label("Location:")
        .halign(Align::End)
        .css_classes(vec!["field-label"])
        .build();
    let location_entry = Entry::builder()
        .text(default_location.to_string_lossy().as_ref())
        .hexpand(true)
        .build();
    let browse_btn = Button::builder().label("Browse…").build();
    grid.attach(&location_label, 0, 1, 1, 1);
    grid.attach(&location_entry, 1, 1, 1, 1);
    grid.attach(&browse_btn, 2, 1, 1, 1);

    let preview_label = Label::builder()
        .halign(Align::Start)
        .css_classes(vec!["project-preview"])
        .build();
    grid.attach(&preview_label, 1, 2, 2, 1);

    let git_toggle = CheckButton::builder()
        .label("Create Git repository")
        .active(true)
        .build();
    grid.attach(&git_toggle, 1, 3, 2, 1);

    let notes_toggle = CheckButton::builder()
        .label("Import Apple Notes as text files")
        .active(false)
        .build();
    grid.attach(&notes_toggle, 1, 4, 2, 1);

    // Buttons Row
    let action_box = Box::new(Orientation::Horizontal, 12);
    action_box.set_halign(Align::End);
    action_box.set_margin_top(8);

    let cancel_btn = Button::builder().label("Cancel").build();
    let create_btn = Button::builder()
        .label("Create")
        .css_classes(vec!["suggested-action"])
        .build();
    action_box.append(&cancel_btn);
    action_box.append(&create_btn);

    content.append(&grid);
    content.append(&action_box);
    dialog.set_child(Some(&content));

    // Keep the "Project will be created in: ..." preview in sync
    let update_preview = Rc::new({
        let preview_label = preview_label.clone();
        let name_entry = name_entry.clone();
        let location_entry = location_entry.clone();
        move || {
            let location = location_entry.text();
            let name = name_entry.text();
            let name = if name.trim().is_empty() {
                "untitled"
            } else {
                name.trim()
            };
            preview_label.set_text(&format!("Project will be created in: {location}/{name}"));
        }
    });
    update_preview();

    let f = update_preview.clone();
    name_entry.connect_changed(move |_| f());
    let f = update_preview.clone();
    location_entry.connect_changed(move |_| f());

    // Browse Button Handler
    let dialog_for_browse = dialog.clone();
    let location_entry_for_browse = location_entry.clone();
    let update_preview_for_browse = update_preview.clone();
    browse_btn.connect_clicked(move |_| {
        let file_dialog = FileDialog::new();
        file_dialog.set_title("Choose Project Location");

        let entry = location_entry_for_browse.clone();
        let update_preview = update_preview_for_browse.clone();
        file_dialog.select_folder(
            Some(&dialog_for_browse),
            gio::Cancellable::NONE,
            move |result| {
                if let Ok(folder) = result
                    && let Some(path) = folder.path()
                {
                    entry.set_text(&path.to_string_lossy());
                    update_preview();
                }
            },
        );
    });

    // Cancel Button Handler
    let dialog_for_cancel = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_for_cancel.close();
    });

    // Create Button Handler
    let dialog_for_create = dialog.clone();
    let parent_for_create = parent.clone();
    create_btn.connect_clicked(move |_| {
        let name = name_entry.text().to_string();
        let name = if name.trim().is_empty() {
            "untitled".to_string()
        } else {
            name.trim().to_string()
        };
        let location = PathBuf::from(location_entry.text().to_string());
        let workspace_path = location.join(&name);

        if let Ok(manager) =
            WorkspaceManager::init_workspace(&workspace_path, git_toggle.is_active())
        {
            if notes_toggle.is_active()
                && let Ok(notes) = fetch_apple_notes()
            {
                for (title, body) in notes {
                    let note_path = manager.root_path.join(format!("{title}.txt"));
                    let _ = std::fs::write(note_path, body);
                }
                let git = crate::git::ops::GitController::new(&manager.root_path);
                let _ = git.commit_all("Initial import from Apple Notes");
            }
            dialog_for_create.close();
            // Also close the welcome window behind this dialog — otherwise
            // it lingers alongside the newly opened workspace window.
            parent_for_create.close();
            on_workspace_ready(manager.root_path);
        }
    });

    dialog.present();
}

/// One click handler for a `sidebar_dropdown` item. (`Box` is aliased to
/// `gtk::Box` in this module, hence the explicit `std::boxed`.)
type MenuAction = std::boxed::Box<dyn Fn()>;

fn menu_action(f: impl Fn() + 'static) -> MenuAction {
    std::boxed::Box::new(f)
}

/// A flat sidebar dropdown ("Configure" / "Help") whose popover is a plain
/// vertical stack of flat buttons — matches the app's other menus without
/// pulling in `gio` actions the welcome window doesn't have yet.
fn sidebar_dropdown(label: &str, items: Vec<(&str, MenuAction)>) -> MenuButton {
    let list = Box::new(Orientation::Vertical, 0);
    list.set_css_classes(&["context-menu"]);

    let popover = Popover::builder().has_arrow(false).build();
    popover.set_child(Some(&list));

    for (text, action) in items {
        let item = Button::builder()
            .label(text)
            .css_classes(vec!["flat", "context-menu-item"])
            .build();
        if let Some(item_label) = item.child().and_downcast::<Label>() {
            item_label.set_xalign(0.0);
            item_label.set_halign(Align::Start);
        }
        let popover_for_item = popover.clone();
        item.connect_clicked(move |_| {
            popover_for_item.popdown();
            action();
        });
        list.append(&item);
    }

    let button = MenuButton::builder()
        .label(label)
        .css_classes(vec!["flat", "welcome-sidebar-menu"])
        .build();
    button.set_popover(Some(&popover));
    button
}

fn open_uri(parent: &ApplicationWindow, uri: &str) {
    let launcher = gtk::UriLauncher::new(uri);
    let parent = parent.clone();
    let uri = uri.to_string();
    glib::MainContext::default().spawn_local(async move {
        if let Err(e) = launcher.launch_future(Some(&parent)).await {
            eprintln!("Failed to open {uri}: {e}");
        }
    });
}

fn show_about(parent: &ApplicationWindow) {
    let dialog = gtk::AboutDialog::builder()
        .program_name("Rhymr")
        .version("2026.1")
        .website("https://rhymr.app")
        .website_label("Visit Website")
        .authors(vec!["Rhymr Team".to_string()])
        .logo_icon_name("text.svg")
        .modal(true)
        .transient_for(parent)
        .build();
    dialog.present();
}
