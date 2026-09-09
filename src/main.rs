use gio::Resource;
use gtk::prelude::*;
use gtk::{gio, glib};
use libadwaita as adw;
use rhymr_rs::setting::Settings;
use rhymr_rs::{app, css};
use std::path::{Path, PathBuf};

pub const APP_ID: &str = "org.gtk_rs.Rhymr";

/// Build the main editor window for `workspace_path` and remember it as a
/// recent project. Shared by the welcome picker and `open` (a folder passed
/// on the command line or via the OS "Open With").
fn open_workspace(app: &adw::Application, target: PathBuf) {
    // Accept either a folder or a file — a file opens its parent folder as
    // the workspace and the file itself in the editor.
    let (root, file) = if target.is_file() {
        (
            target
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(target.clone()),
            Some(target),
        )
    } else {
        (target, None)
    };

    rhymr_rs::workspace::recent::record_recent_workspace(&root);
    let (_window, controller) = app::layout::build_ui(app);
    controller.set_root_path(root.clone());

    // Open a file so the editor isn't staring at an empty state — the one
    // passed, else the first text file, matching how a JetBrains project
    // reopens with something visible.
    let to_open = file.or_else(|| first_text_file(&root));
    if let Some(path) = to_open
        && let Some(workspace) = controller.get_workspace()
    {
        workspace.open_path(path);
    }
}

/// Alphabetically-first `.txt` directly in `dir`, if any.
fn first_text_file(dir: &std::path::Path) -> Option<PathBuf> {
    let mut txts: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("txt")))
        .collect();
    txts.sort();
    txts.into_iter().next()
}

fn main() -> glib::ExitCode {
    // Verbosity flags (`-v` / `--verbose`, repeatable) and `RHYMR_LOG` drive
    // stderr logging. Parse and strip our flags before GTK sees argv —
    // a HANDLES_OPEN app rejects options it doesn't recognise.
    let mut args: Vec<String> = std::env::args().collect();
    let verbosity = rhymr_rs::logging::parse_verbosity(&mut args);
    rhymr_rs::logging::init(verbosity);

    // Register the resource bundle from the compiled resource file
    let resource_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/compiled.gresource"));
    let resource_data = glib::Bytes::from(&resource_bytes[..]);
    gio::resources_register(
        &Resource::from_data(&resource_data).expect("Failed to load resources"),
    );

    log::debug!("current dir = {:?}", std::env::current_dir());

    // Compile scss files into css files
    if let Err(e) = css::compile_sass() {
        log::error!("compile_sass failed: {e}");
        panic!("{e}");
    }

    glib::set_application_name("Rhymr");

    // adw::Application initializes both GTK and libadwaita on startup.
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    // Once GTK is up, point the icon theme at the bundled resources and
    // name the app's icon by its id so `rhymr-icon.svg` (aliased to
    // `org.gtk_rs.Rhymr.svg` in resources.xml) is the window / app icon.
    // A real macOS Dock icon still needs the `.app` bundle from
    // `Build/bundle-mac.sh`.
    app.connect_startup(|_| {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::IconTheme::for_display(&display).add_resource_path("/org/gtk_rs/rhymr/icons");
        }
        gtk::Window::set_default_icon_name(APP_ID);
    });

    let apply_theme = || {
        let settings = Settings::load();
        app::icons::set_variant(&settings);
        let css_provider = css::init(&settings);
        css::apply_css_to_app(&css_provider);
        css::sync_style_manager(&settings);
    };

    // No path given: (optionally) splash, then the workspace picker — or,
    // when "reopen last project" is set, straight into the last workspace.
    app.connect_activate(move |app| {
        apply_theme();
        let startup = Settings::load();

        if startup.reopen_last_project
            && let Some(recent) = rhymr_rs::workspace::recent::load_recent_workspaces()
                .into_iter()
                .next()
        {
            open_workspace(app, recent);
            return;
        }

        let show_welcome = {
            let app = app.clone();
            move || {
                let app_for_workspace = app.clone();
                rhymr_rs::app::welcome::show_welcome_dialog(&app, move |workspace_path| {
                    open_workspace(&app_for_workspace, workspace_path);
                });
            }
        };

        if startup.show_splash {
            let splash = rhymr_rs::app::splash::show(app);
            glib::timeout_add_local_once(
                std::time::Duration::from_millis(u64::from(startup.splash_duration_ms)),
                move || {
                    show_welcome();
                    splash.close();
                },
            );
        } else {
            show_welcome();
        }
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

    // Run application! (argv with our verbosity flags already stripped)
    app.run_with_args(&args)
}
