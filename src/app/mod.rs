pub mod about;
pub mod chrome;
pub mod context_menu;
pub mod layout;
pub mod menu;
pub mod splash;
pub mod vertical_label;
pub mod welcome;

use gtk::prelude::*;

/// External links, in one place rather than scattered as string literals.
pub const DOCS_URL: &str = "https://github.com/Rhymr/win-mac";
pub const ISSUES_URL: &str = "https://github.com/Rhymr/win-mac/issues";

/// Open `uri` in the user's browser, parented to `window`. One place so the
/// Help menu and the welcome window's Help dropdown don't each hand-roll a
/// `UriLauncher` + `spawn_local`.
pub fn open_uri(window: &impl IsA<gtk::Window>, uri: &str) {
    let launcher = gtk::UriLauncher::new(uri);
    let window = window.clone().upcast::<gtk::Window>();
    let uri = uri.to_string();
    gtk::glib::MainContext::default().spawn_local(async move {
        if let Err(e) = launcher.launch_future(Some(&window)).await {
            eprintln!("Failed to open {uri}: {e}");
        }
    });
}
