use gio::Resource;
use gtk::prelude::*;
use gtk::{gio, glib};
use libadwaita as adw;
use rhymr_rs::setting::Settings;
use rhymr_rs::{app, css};
use std::path::PathBuf;

pub const APP_ID: &str = "org.gtk_rs.Rhymr";

/// Build the main editor window for `workspace_path` and remember it as a
/// recent project. Shared by the welcome picker and `open` (a folder passed
/// on the command line or via the OS "Open With").
fn open_workspace(app: &adw::Application, workspace_path: PathBuf) {
    rhymr_rs::workspace::recent::record_recent_workspace(&workspace_path);
    let (_window, controller) = app::layout::build_ui(app);
    controller.set_root_path(workspace_path);
}

fn main() -> glib::ExitCode {
    // Register the resource bundle from the compiled resource file
    let resource_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/compiled.gresource"));
    let resource_data = glib::Bytes::from(&resource_bytes[..]);
    gio::resources_register(
        &Resource::from_data(&resource_data).expect("Failed to load resources"),
    );

    println!("Current dir = {:?}", std::env::current_dir().unwrap());

    // Compile scss files into css files
    if let Err(e) = css::compile_sass() {
        eprintln!("compile_sass failed: {e}");
        panic!("{e}");
    }

    // adw::Application initializes both GTK and libadwaita on startup.
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let apply_theme = || {
        let settings = Settings::load();
        let css_provider = css::init(&settings);
        css::apply_css_to_app(&css_provider);
        css::sync_style_manager(&settings);
    };

    // No path given: splash, then the workspace picker.
    app.connect_activate(move |app| {
        apply_theme();

        let splash = rhymr_rs::app::splash::show(app);
        let app_for_welcome = app.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(1600), move || {
            let app_for_workspace = app_for_welcome.clone();
            rhymr_rs::app::welcome::show_welcome_dialog(&app_for_welcome, move |workspace_path| {
                open_workspace(&app_for_workspace, workspace_path);
            });
            splash.close();
        });
    });

    // A folder passed on the command line / via "Open With": go straight to
    // the editor for it.
    app.connect_open(move |app, files, _| {
        apply_theme();
        if let Some(path) = files.first().and_then(|f| f.path()) {
            open_workspace(app, path);
        } else {
            app.activate();
        }
    });

    // Run application!
    app.run()
}
