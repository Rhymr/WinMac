use grass::Options;
use std::fs;
use std::path::Path;

// Kept in sync with `CSS_FILES` in src/css.rs — build.rs precompiles these
// to assets/css/*.css (unused by the app itself, which recompiles from
// source at runtime, but kept so the compiled output isn't stale).
const CSS_FILES: [&str; 12] = [
    "assets/{1}/base.{1}",
    "assets/{1}/context_menu.{1}",
    "assets/{1}/dialog.{1}",
    "assets/{1}/editor.{1}",
    "assets/{1}/empty_state.{1}",
    "assets/{1}/file_tree.{1}",
    "assets/{1}/layout.{1}",
    "assets/{1}/notebook.{1}",
    "assets/{1}/rhyme_search.{1}",
    "assets/{1}/settings.{1}",
    "assets/{1}/status_bar.{1}",
    "assets/{1}/welcome.{1}",
];

fn main() {
    // Tell cargo to re-run this build script if any asset changes
    println!("cargo:rerun-if-changed=assets");

    // 1. Compile SCSS -> CSS before building resources
    for css_file in CSS_FILES {
        let scss_path = css_file.replace("{1}", "scss");
        let css_path = css_file.replace("{1}", "css");

        if Path::new(&scss_path).exists()
            && let Ok(css_output) = grass::from_path(&scss_path, &Options::default())
        {
            if let Some(parent) = Path::new(&css_path).parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(css_path, css_output);
        }
    }

    // 2. Compile GTK Resources
    glib_build_tools::compile_resources(&["assets"], "assets/resources.xml", "compiled.gresource");
}
