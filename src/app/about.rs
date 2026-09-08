//! The single About dialog — shared by the Help menu and the welcome
//! window's Help dropdown, so there's one place that knows the program
//! metadata and it always shows the git-derived version.

use gtk::prelude::*;

pub fn show(parent: &impl IsA<gtk::Window>) {
    gtk::AboutDialog::builder()
        .program_name("Rhymr")
        .version(crate::version::CORE)
        .comments("A JetBrains-grade editor for lyrics and poetry.")
        .website("https://rhymr.app")
        .website_label("Visit Website")
        .authors(vec!["Rhymr Team".to_string()])
        .modal(true)
        .transient_for(parent)
        .build()
        .present();
}
