use std::fs;
use std::path::{Path, PathBuf};

fn recent_workspaces_file() -> Option<PathBuf> {
    let mut dir = dirs::config_dir()?;
    dir.push("rhymr");
    fs::create_dir_all(&dir).ok()?;
    dir.push("recent_workspaces.txt");
    Some(dir)
}

/// Recently opened workspace root folders, most recent first. Entries that
/// no longer exist on disk are dropped.
pub fn load_recent_workspaces() -> Vec<PathBuf> {
    let Some(path) = recent_workspaces_file() else {
        return Vec::new();
    };

    let Ok(contents) = fs::read_to_string(&path) else {
        return Vec::new();
    };

    contents
        .lines()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .collect()
}

/// Record `workspace_root` as the most recently opened workspace, moving it
/// to the front if already tracked and capping the list length.
pub fn record_recent_workspace(workspace_root: &Path) {
    let Some(path) = recent_workspaces_file() else {
        return;
    };

    let mut recents = load_recent_workspaces();
    recents.retain(|p| p != workspace_root);
    recents.insert(0, workspace_root.to_path_buf());
    recents.truncate(
        crate::setting::Settings::load()
            .recent_projects_limit
            .max(1) as usize,
    );

    let contents = recents
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    let _ = fs::write(path, contents);
}

/// Drop `workspace_root` from the tracked recent list, if present.
pub fn remove_recent_workspace(workspace_root: &Path) {
    let Some(path) = recent_workspaces_file() else {
        return;
    };

    let mut recents = load_recent_workspaces();
    recents.retain(|p| p != workspace_root);

    let contents = recents
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    let _ = fs::write(path, contents);
}
