use grass::Options;
use std::fs;
use std::path::Path;
use std::process::Command;

// Kept in sync with `CSS_FILES` in src/css.rs — SCSS stems under
// `assets/scss/`. Precompiled here to `assets/css/*.css` (unused by the app
// itself, which recompiles from source at runtime, but kept so the
// compiled output isn't stale).
const CSS_FILES: [&str; 16] = [
    "base",
    "chrome",
    "context_menu",
    "dialog",
    "dock",
    "editor",
    "empty_state",
    "file_tree",
    "git_log",
    "layout",
    "notebook",
    "rhyme_search",
    "settings",
    "splash",
    "status_bar",
    "welcome",
];

/// Ask `scripts/version.sh` for the git-derived version; fall back to
/// `CARGO_PKG_VERSION` if git or the script isn't available (e.g. a source
/// tarball). Exposed to the crate as `RHYMR_VERSION` / `RHYMR_VERSION_FULL`
/// (see `src/version.rs`).
fn emit_version() {
    let run = |args: &[&str]| -> Option<String> {
        let out = Command::new("bash")
            .arg("scripts/version.sh")
            .args(args)
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    };

    let pkg = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let core = run(&[]).unwrap_or_else(|| pkg.clone());
    let full = run(&["--full"]).unwrap_or_else(|| core.clone());
    println!("cargo:rustc-env=RHYMR_VERSION={core}");
    println!("cargo:rustc-env=RHYMR_VERSION_FULL={full}");
    // Re-run when the commit or working-tree state changes.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
    println!("cargo:rerun-if-changed=scripts/version.sh");
}

fn main() {
    emit_version();

    // Tell cargo to re-run this build script if any asset changes
    println!("cargo:rerun-if-changed=assets");

    // 1. Compile SCSS -> CSS before building resources
    let scss_opts = Options::default().load_path("assets/scss");
    for stem in CSS_FILES {
        let scss_path = format!("assets/scss/{stem}.scss");
        let css_path = format!("assets/css/{stem}.css");

        if Path::new(&scss_path).exists()
            && let Ok(css_output) = grass::from_path(&scss_path, &scss_opts)
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
