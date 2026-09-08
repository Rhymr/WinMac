use crate::workspace::controller::WorkspaceController;
use gio::Menu;
use gtk::gio;
use gtk::prelude::*;
use libadwaita::Application;
use std::path::PathBuf;
use std::rc::Rc;

// GTK's own "<Primary>" accelerator modifier is documented to map to Cmd on
// macOS and Ctrl elsewhere, but that mapping depends on the platform's GDK
// backend correctly reporting it — build the modifier explicitly instead so
// shortcuts (and their on-screen labels) are unambiguous: ⌘ on macOS, Ctrl
// on Windows/Linux.
#[cfg(target_os = "macos")]
const PRIMARY_MOD: &str = "<Meta>";
#[cfg(not(target_os = "macos"))]
const PRIMARY_MOD: &str = "<Control>";

/// Build an accelerator string with the platform's primary modifier plus
/// any extra modifiers (e.g. "<Shift>", "<Alt>") and the key, e.g.
/// `accel("<Shift>", "n")` => "<Meta><Shift>n" on macOS, "<Control><Shift>n" elsewhere.
fn accel(extra_mods: &str, key: &str) -> String {
    format!("{PRIMARY_MOD}{extra_mods}{key}")
}

/// Builds the File/Git/Help menu model and wires up every action it
/// invokes. Returns the model so callers can also feed it to an in-window
/// `MenuButton` — `set_menubar` alone only surfaces it via native shell
/// integration (e.g. macOS's menu bar), which doesn't exist on
/// Windows/Linux, so the header bar's hamburger menu is the only way
/// those platforms can reach File/Git/Help at all.
pub fn setup_menu(app: &Application, workspace_controller: Rc<WorkspaceController>) -> Menu {
    let file = file(app, workspace_controller.clone());
    // edit(app, workspace_controller);
    let git = git(app, workspace_controller.clone());
    let help = help(app);

    // Create the menu bar and add menus
    let menu_bar = Menu::new();
    menu_bar.append_submenu(Some("File"), &file);
    menu_bar.append_submenu(Some("Git"), &git);
    menu_bar.append_submenu(Some("Help"), &help);

    // Set menu bar in app
    app.set_menubar(Some(&menu_bar));

    menu_bar
}

fn git(app: &Application, workspace_controller: Rc<WorkspaceController>) -> Menu {
    let git_menu = gio::Menu::new();

    let commit_section = gio::Menu::new();
    commit_section.append(Some("Commit…"), Some("app.git-commit"));
    git_menu.insert_section(0, None, &commit_section);

    let sync_section = gio::Menu::new();
    sync_section.append(Some("Push…"), Some("app.git-push"));
    sync_section.append(Some("Pull…"), Some("app.git-pull"));
    sync_section.append(Some("Fetch"), Some("app.git-fetch"));
    git_menu.insert_section(1, None, &sync_section);

    let controller = workspace_controller.clone();
    let app_weak = app.downgrade();
    let commit_action = gio::SimpleAction::new("git-commit", None);
    commit_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade() {
            crate::git::dialog::show_commit_dialog(&app, controller.clone());
        }
    });
    app.add_action(&commit_action);

    let controller = workspace_controller.clone();
    let app_weak = app.downgrade();
    let push_action = gio::SimpleAction::new("git-push", None);
    push_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade() {
            crate::git::dialog::show_push_dialog(&app, controller.clone());
        }
    });
    app.add_action(&push_action);

    let controller = workspace_controller.clone();
    let app_weak = app.downgrade();
    let pull_action = gio::SimpleAction::new("git-pull", None);
    pull_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade() {
            crate::git::dialog::show_pull_dialog(&app, controller.clone());
        }
    });
    app.add_action(&pull_action);

    let controller = workspace_controller.clone();
    let app_weak = app.downgrade();
    let fetch_action = gio::SimpleAction::new("git-fetch", None);
    fetch_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade() {
            crate::git::dialog::show_fetch_dialog(&app, controller.clone());
        }
    });
    app.add_action(&fetch_action);

    app.set_accels_for_action("app.git-commit", &[&accel("", "k")]);
    app.set_accels_for_action("app.git-push", &[&accel("<Shift>", "k")]);
    app.set_accels_for_action("app.git-pull", &[&accel("", "t")]);

    git_menu
}

fn help(app: &Application) -> Menu {
    let help_menu = Menu::new();

    // Documentation section
    let docs_section = gio::Menu::new();
    docs_section.append(Some("Documentation"), Some("app.docs"));
    docs_section.append(Some("Report Issue"), Some("app.report-issue"));
    help_menu.insert_section(0, None, &docs_section);

    // About section
    let about_section = gio::Menu::new();
    about_section.append(Some("About"), Some("app.about"));
    help_menu.insert_section(1, None, &about_section);

    // Documentation action
    let docs_action = gio::SimpleAction::new("docs", None);
    let app_weak = app.downgrade();
    docs_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade()
            && let Some(window) = app.active_window()
        {
            let launcher = gtk::UriLauncher::new("https://github.com/rhymr/win-mac");
            let ctx = glib::MainContext::default();
            ctx.spawn_local(async move {
                if let Err(e) = launcher.launch_future(Some(&window)).await {
                    eprintln!("Failed to open docs: {}", e);
                }
            });
        }
    });
    app.add_action(&docs_action);

    // Report issue action
    let report_action = gio::SimpleAction::new("report-issue", None);
    let app_weak = app.downgrade();
    report_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade()
            && let Some(window) = app.active_window()
        {
            let launcher = gtk::UriLauncher::new("https://github.com/rhymr/win-mac/issues");
            let ctx = glib::MainContext::default();
            ctx.spawn_local(async move {
                if let Err(e) = launcher.launch_future(Some(&window)).await {
                    eprintln!("Failed to open issue tracker: {}", e);
                }
            });
        }
    });
    app.add_action(&report_action);

    // About action
    let about_action = gio::SimpleAction::new("about", None);
    let app_weak = app.downgrade();
    about_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade()
            && let Some(window) = app.active_window()
        {
            let dialog = gtk::AboutDialog::builder()
                .program_name("Rhymr")
                .version("2026.1")
                .website("https://rhymr.app")
                .website_label("Visit Website")
                .authors(vec!["Rhymr Team".to_string()])
                .logo_icon_name("text.svg")
                .modal(true)
                .transient_for(&window)
                .build();

            dialog.present();
        }
    });
    app.add_action(&about_action);

    help_menu
}

pub fn file(app: &Application, workspace_controller: Rc<WorkspaceController>) -> Menu {
    let file_menu = gio::Menu::new();

    // New (submenu) + Open section
    let new_menu = gio::Menu::new();
    new_menu.append(Some("File"), Some("app.new"));
    new_menu.append(Some("Folder"), Some("app.new-folder"));

    let file_ops_section = gio::Menu::new();
    file_ops_section.append_submenu(Some("New"), &new_menu);
    file_ops_section.append(Some("Open..."), Some("app.open"));
    file_menu.insert_section(0, None, &file_ops_section);

    // Recent Projects (submenu) + Close Project
    let recent_menu = gio::Menu::new();
    for path in crate::workspace::recent::load_recent_workspaces() {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Workspace")
            .to_string();
        let item = gio::MenuItem::new(Some(&name), None);
        item.set_action_and_target_value(
            Some("app.open-recent"),
            Some(&path.to_string_lossy().to_variant()),
        );
        recent_menu.append_item(&item);
    }

    let recent_section = gio::Menu::new();
    recent_section.append_submenu(Some("Recent Projects"), &recent_menu);
    recent_section.append(Some("Close Project"), Some("app.close-project"));
    file_menu.insert_section(1, None, &recent_section);

    // Save operations section
    let save_ops_section = gio::Menu::new();
    save_ops_section.append(Some("Save"), Some("app.save"));
    save_ops_section.append(Some("Save As..."), Some("app.save-as"));
    save_ops_section.append(Some("Save All"), Some("app.save-all"));
    file_menu.insert_section(2, None, &save_ops_section);

    // Reload section
    let reload_section = gio::Menu::new();
    reload_section.append(Some("Reload All from Disk"), Some("app.reload-all"));
    file_menu.insert_section(3, None, &reload_section);

    // Tab operations section
    let tab_ops_section = gio::Menu::new();
    tab_ops_section.append(Some("Close Tab"), Some("app.close-tab"));
    file_menu.insert_section(4, None, &tab_ops_section);

    // Preferences section
    let preferences_section = gio::Menu::new();
    preferences_section.append(Some("Preferences…"), Some("app.preferences"));
    file_menu.insert_section(5, None, &preferences_section);

    // Preferences action
    let app_weak = app.downgrade();
    let controller = workspace_controller.clone();
    let preferences_action = gio::SimpleAction::new("preferences", None);
    preferences_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade() {
            crate::setting::dialog::show_settings_dialog(&app, Some(controller.clone()));
        }
    });
    app.add_action(&preferences_action);

    // New file action
    let controller = workspace_controller.clone();
    let new_action = gio::SimpleAction::new("new", None);
    new_action.connect_activate(move |_, _| {
        controller.handle_new_file();
    });
    app.add_action(&new_action);

    // New folder action
    let controller = workspace_controller.clone();
    let new_folder_action = gio::SimpleAction::new("new-folder", None);
    new_folder_action.connect_activate(move |_, _| {
        controller.handle_new_folder();
    });
    app.add_action(&new_folder_action);

    // Open file action
    let controller = workspace_controller.clone();
    let app_weak = app.downgrade();
    let open_action = gio::SimpleAction::new("open", None);
    open_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade()
            && let Some(window) = app.active_window()
        {
            controller.handle_open_file(&window);
        }
    });
    app.add_action(&open_action);

    // Open Recent Project action (target = workspace path as a string)
    let controller = workspace_controller.clone();
    let open_recent_action = gio::SimpleAction::new("open-recent", Some(glib::VariantTy::STRING));
    open_recent_action.connect_activate(move |_, param| {
        if let Some(path_str) = param.and_then(|v| v.get::<String>()) {
            controller.switch_workspace(PathBuf::from(path_str));
        }
    });
    app.add_action(&open_recent_action);

    // Close Project action — back to the workspace picker
    let app_weak = app.downgrade();
    let close_project_action = gio::SimpleAction::new("close-project", None);
    close_project_action.connect_activate(move |_, _| {
        let Some(app) = app_weak.upgrade() else {
            return;
        };
        if let Some(window) = app.active_window() {
            window.close();
        }
        let app_for_welcome = app.clone();
        crate::app::welcome::show_welcome_dialog(&app, move |workspace_path| {
            crate::workspace::recent::record_recent_workspace(&workspace_path);
            let (_window, controller) = crate::app::layout::build_ui(&app_for_welcome);
            controller.set_root_path(workspace_path);
        });
    });
    app.add_action(&close_project_action);

    // Save action
    let controller = workspace_controller.clone();
    let app_weak = app.downgrade();
    let save_action = gio::SimpleAction::new("save", None);
    save_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade()
            && let Some(window) = app.active_window()
        {
            controller.handle_save_file(&window);
        }
    });
    app.add_action(&save_action);

    // Save As action
    let controller = workspace_controller.clone();
    let app_weak = app.downgrade();
    let save_as_action = gio::SimpleAction::new("save-as", None);
    save_as_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade()
            && let Some(window) = app.active_window()
        {
            controller.handle_save_as_file(&window);
        }
    });
    app.add_action(&save_as_action);

    // Save All action
    let controller = workspace_controller.clone();
    let save_all_action = gio::SimpleAction::new("save-all", None);
    save_all_action.connect_activate(move |_, _| {
        if let Some(workspace) = controller.get_workspace() {
            workspace.save_all();
        }
    });
    app.add_action(&save_all_action);

    // Reload All from Disk action
    let controller = workspace_controller.clone();
    let reload_all_action = gio::SimpleAction::new("reload-all", None);
    reload_all_action.connect_activate(move |_, _| {
        if let Some(workspace) = controller.get_workspace() {
            workspace.reload_all();
        }
    });
    app.add_action(&reload_all_action);

    // Add close tab action
    let controller = workspace_controller.clone();
    let app_weak = app.downgrade();
    let close_action = gio::SimpleAction::new("close-tab", None);
    close_action.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade()
            && let Some(window) = app.active_window()
        {
            controller.handle_close_tab(&window);
        }
    });
    app.add_action(&close_action);

    // Add keyboard accelerators — ⌘ on macOS, Ctrl on Windows/Linux
    app.set_accels_for_action("app.new", &[&accel("", "n")]);
    app.set_accels_for_action("app.new-folder", &[&accel("<Shift>", "n")]);
    app.set_accels_for_action("app.open", &[&accel("", "o")]);
    app.set_accels_for_action("app.save", &[&accel("<Alt>", "s")]);
    app.set_accels_for_action("app.save-as", &[&accel("<Shift>", "s")]);
    app.set_accels_for_action("app.save-all", &[&accel("", "s")]);
    app.set_accels_for_action("app.reload-all", &[&accel("<Alt>", "y")]);
    app.set_accels_for_action("app.close-tab", &[&accel("", "w")]);
    app.set_accels_for_action("app.preferences", &[&accel("", "comma")]);

    file_menu
}
