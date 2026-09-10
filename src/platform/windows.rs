use crate::platform::{NoteRow, ShellSpec};
use std::path::PathBuf;

/// Apple Notes has no Windows equivalent to shell out to. Returns an empty
/// snapshot so the "Apple Notes" source cleanly reports `Unavailable`
/// rather than the app failing to build on Windows.
pub fn fetch_apple_notes() -> Result<Vec<NoteRow>, String> {
    Ok(Vec::new())
}

/// PowerShell when it's on `PATH`, else `%ComSpec%` (usually `cmd.exe`).
pub fn default_shell() -> ShellSpec {
    if let Some(comspec) = std::env::var_os("ComSpec") {
        return (PathBuf::from(comspec), Vec::new());
    }
    (PathBuf::from("powershell.exe"), Vec::new())
}
