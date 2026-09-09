use std::fs;
use std::path::{Path, PathBuf};

pub struct WorkspaceManager {
    pub root_path: PathBuf,
}

impl WorkspaceManager {
    pub fn init_workspace(path: &Path, init_git: bool) -> Result<Self, String> {
        log::info!("initialising workspace at {path:?} (git={init_git})");
        // 1. Ensure root workspace folder exists
        fs::create_dir_all(path).map_err(|e| e.to_string())?;

        // 2. Create hidden system folder for app state (.rhymr/)
        let rhymr_dir = path.join(".rhymr");
        fs::create_dir_all(&rhymr_dir).map_err(|e| e.to_string())?;

        // 3. Create default .gitignore ignoring .rhymr/
        let gitignore_path = path.join(".gitignore");
        if !gitignore_path.exists() {
            fs::write(&gitignore_path, ".rhymr/\n").map_err(|e| e.to_string())?;
        }

        // 4. Initialize Git repo if enabled
        if init_git {
            git2::Repository::init(path).map_err(|e| {
                log::error!("workspace init: git repo init failed: {e}");
                e.to_string()
            })?;
        }

        Ok(Self {
            root_path: path.to_path_buf(),
        })
    }
}
