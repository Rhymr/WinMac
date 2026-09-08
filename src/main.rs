use gio::Resource;
use gtk::prelude::*;
use gtk::{gio, glib};
use libadwaita as adw;
use rhymr_rs::setting::Settings;
use rhymr_rs::{app, css};

pub const APP_ID: &str = "org.gtk_rs.Rhymr";

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

    // Connect activate signal
    app.connect_activate(|app| {
        // Load CSS + theme once, up front — the welcome window is shown
        // before the main layout is ever built, so it needs both applied
        // here too.
        let settings = Settings::load();
        let css_provider = css::init(&settings);
        css::apply_css_to_app(&css_provider);
        css::sync_style_manager(&settings);

        // Classic product splash, held for a beat, then the workspace picker
        // (the main editor layout is only built once a workspace is chosen).
        let splash = rhymr_rs::app::splash::show(app);
        let app_for_welcome = app.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(1600), move || {
            let app_for_workspace = app_for_welcome.clone();
            rhymr_rs::app::welcome::show_welcome_dialog(&app_for_welcome, move |workspace_path| {
                println!("Loaded workspace at: {:?}", workspace_path);
                rhymr_rs::workspace::recent::record_recent_workspace(&workspace_path);
                let (_window, controller) = app::layout::build_ui(&app_for_workspace);
                controller.set_root_path(workspace_path);
            });
            splash.close();
        });
    });

    // Run application!
    app.run()
}
